//! Non-blocking local Ollama model preload (`prompt: ""`) to prevent cold-load turn timeouts.

use crate::ai::{
    catalog::{ProviderId, ProviderPreference},
    ollama::OllamaConfig,
    ollama_discovery::discover_local_provider,
    ollama_http::{send_http_request, OllamaOptions, OLLAMA_KEEP_ALIVE, OLLAMA_NUM_CTX},
    router::TutorRouter,
};
use crate::domain::SessionError;
use serde::Serialize;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const OLLAMA_WARMUP_TIMEOUT_SECS: u64 = 180;
const WARMUP_RETRY_BACKOFF: Duration = Duration::from_secs(30);
const WARMUP_FRESH_WINDOW: Duration = Duration::from_secs(480);

#[derive(Default)]
pub(crate) struct WarmupTracker {
    in_flight: bool,
    warmed_model: Option<String>,
    last_attempt: Option<Instant>,
}

impl WarmupTracker {
    fn try_claim(&mut self, target_override: Option<&str>, force: bool) -> bool {
        if self.in_flight {
            return false;
        }
        let now = Instant::now();
        if !force && self.is_recently_warmed(target_override, now) {
            return false;
        }
        if !force && self.is_in_backoff(now) {
            return false;
        }
        self.in_flight = true;
        self.last_attempt = Some(now);
        true
    }

    fn is_recently_warmed(&self, target_override: Option<&str>, now: Instant) -> bool {
        let Some(warmed) = self.warmed_model.as_deref() else {
            return false;
        };
        let same_model = target_override.is_none_or(|ov| ov.eq_ignore_ascii_case(warmed));
        let fresh = self
            .last_attempt
            .is_some_and(|t| now.saturating_duration_since(t) < WARMUP_FRESH_WINDOW);
        same_model && fresh
    }

    fn is_in_backoff(&self, now: Instant) -> bool {
        self.warmed_model.is_none()
            && self
                .last_attempt
                .is_some_and(|t| now.saturating_duration_since(t) < WARMUP_RETRY_BACKOFF)
    }

    fn finish(&mut self, res: &Result<String, SessionError>) {
        self.in_flight = false;
        self.last_attempt = Some(Instant::now());
        self.warmed_model = res.as_ref().ok().cloned();
    }
}

#[derive(Serialize)]
struct OllamaWarmupRequest<'a> {
    model: &'a str,
    prompt: &'static str,
    stream: bool,
    keep_alive: &'static str,
    options: OllamaOptions,
}

/// Sends Ollama's official empty-prompt preload request so the active model is warm in memory.
pub(crate) async fn warm_local_model(
    base: &OllamaConfig,
    model_override: Option<&str>,
) -> Result<String, SessionError> {
    let target = resolve_warmup_config(base, model_override).await?;
    let body = serde_json::to_string(&OllamaWarmupRequest {
        model: target.model_name(),
        prompt: "",
        stream: false,
        keep_alive: OLLAMA_KEEP_ALIVE,
        options: OllamaOptions {
            num_ctx: OLLAMA_NUM_CTX,
            num_predict: None,
        },
    })
    .unwrap_or_default();
    let _ = send_http_request(
        &target,
        "POST",
        "/api/generate",
        &body,
        OLLAMA_WARMUP_TIMEOUT_SECS,
    )
    .await?;
    Ok(target.model_name().to_string())
}

async fn resolve_warmup_config(
    base: &OllamaConfig,
    model_override: Option<&str>,
) -> Result<OllamaConfig, SessionError> {
    if let Some(m) = model_override.map(str::trim).filter(|s| !s.is_empty()) {
        return OllamaConfig::try_new(base.host(), base.port(), m);
    }
    Ok(discover_local_provider(base).await?.config().clone())
}

impl TutorRouter {
    /// Spawns a single non-blocking Ollama preload when local Ollama is the primary route.
    pub fn spawn_local_warmup_if_needed(&self) {
        spawn_router_local_warmup(self, &self.warmup_tracker, false);
    }

    /// Forces a non-blocking Ollama preload check after explicit provider/model config changes.
    pub fn spawn_local_warmup_forced(&self) {
        spawn_router_local_warmup(self, &self.warmup_tracker, true);
    }
}

/// Spawns a single deduplicated background warmup task when Ollama is the active local route.
pub(crate) fn spawn_router_local_warmup(
    router: &TutorRouter,
    tracker: &Arc<Mutex<WarmupTracker>>,
    force: bool,
) {
    let Some(override_model) = warmup_target_if_local_primary(router) else {
        return;
    };
    let clean_override = override_model
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    if !tracker
        .lock()
        .expect("lock")
        .try_claim(clean_override, force)
    {
        return;
    }
    let base = router.ollama_base.clone();
    let shared = Arc::clone(tracker);
    tokio::spawn(async move {
        let res = warm_local_model(&base, override_model.as_deref()).await;
        shared.lock().expect("lock").finish(&res);
        match res {
            Ok(model) => tracing::info!(model = %model, "Preloaded local Ollama model"),
            Err(err) => tracing::debug!(error = %err.user_message, "Skipped local Ollama warmup"),
        }
    });
}

fn warmup_target_if_local_primary(router: &TutorRouter) -> Option<Option<String>> {
    let st = router.state.read().expect("lock");
    let pref = st.prefs.preferred_provider;
    let has_cloud_key = ProviderId::CLOUD_ORDER
        .into_iter()
        .any(|id| router.secrets.resolve(id).is_some());
    let local_primary = !st.prefs.cloud_opt_in
        || pref == ProviderPreference::Ollama
        || (pref == ProviderPreference::Auto && !has_cloud_key);
    local_primary.then(|| st.prefs.model_overrides.get(&ProviderId::Ollama).cloned())
}
