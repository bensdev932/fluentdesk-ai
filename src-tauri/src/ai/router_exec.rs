//! Per-provider dispatch helpers for `TutorRouter` (`Ollama`, `Groq`, `Gemini`, `OpenRouter`).

use crate::ai::{
    catalog::{ProviderId, ProviderPreference},
    cloud_http::CloudFailure,
    gemini::GeminiProvider,
    ollama::{OllamaAiProvider, OllamaConfig},
    ollama_discovery::discover_local_provider,
    openai_compat::OpenAiCompatProvider,
    provider::AiProvider,
    router_types::{CircuitBreakers, RoutedTutorReply},
    secrets::RuntimeSecretStore,
};
use crate::domain::{
    ConversationTurn, CorrectionMode, SessionError, SessionErrorKind, Transcript, TutorReply,
};
use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Mutex,
};

pub(crate) const TRANSIENT_COOLDOWN_SECS: u64 = 60;

pub(crate) fn build_candidate_list(
    pref: ProviderPreference,
    cloud_opt_in: bool,
    start_cursor: usize,
    secrets: &RuntimeSecretStore,
    is_healthy_fn: impl Fn(ProviderId) -> bool,
) -> Result<Vec<ProviderId>, SessionError> {
    if let Some(pinned) = pref.as_pinned() {
        if pinned.is_cloud() && !cloud_opt_in {
            return Err(consent_err());
        }
        return Ok(vec![pinned]);
    }
    let mut list: Vec<ProviderId> = if cloud_opt_in {
        let len = ProviderId::CLOUD_ORDER.len();
        (0..len)
            .map(|offset| ProviderId::CLOUD_ORDER[(start_cursor + offset) % len])
            .filter(|&id| secrets.resolve(id).is_some() && is_healthy_fn(id))
            .collect()
    } else {
        Vec::new()
    };
    list.push(ProviderId::Ollama);
    Ok(list)
}

pub(crate) fn consent_err() -> SessionError {
    SessionError::new(
        SessionErrorKind::AiProviderUnavailable,
        "Cloud AI is disabled. Enable Cloud Opt-In to use Groq, Gemini, or OpenRouter.",
        true,
        None,
    )
}

pub(crate) struct TurnInput<'a> {
    pub history: &'a [ConversationTurn],
    pub transcript: &'a Transcript,
    pub mode: CorrectionMode,
}

pub(crate) struct DispatchEnv<'a> {
    pub ollama_base: &'a OllamaConfig,
    pub endpoints: &'a HashMap<ProviderId, String>,
    pub overrides: &'a HashMap<ProviderId, String>,
    pub secrets: &'a RuntimeSecretStore,
    pub breakers: &'a Mutex<CircuitBreakers>,
    pub rr_cursor: &'a AtomicUsize,
}

pub(crate) async fn execute_candidates(
    candidates: &[ProviderId],
    env: &DispatchEnv<'_>,
    turn: &TurnInput<'_>,
    req_model: Option<&str>,
    is_auto: bool,
) -> Result<RoutedTutorReply, SessionError> {
    let (mut notes, mut last_err) = (Vec::new(), None);
    for (idx, &id) in candidates.iter().enumerate() {
        if is_auto && id.is_cloud() {
            advance_cursor(env.rr_cursor, id);
        }
        let model = (idx == 0)
            .then_some(req_model)
            .flatten()
            .or_else(|| env.overrides.get(&id).map(String::as_str));
        match invoke_provider(id, model, env, turn).await {
            Ok((reply, used)) => return Ok(ok_routed(reply, id, used, &notes)),
            Err((err, failure)) => {
                env.breakers
                    .lock()
                    .expect("lock")
                    .record(id, failure, TRANSIENT_COOLDOWN_SECS);
                notes.push(format!("{}: {}", id.as_str(), err.user_message));
                last_err = Some(err);
            }
        }
    }
    Err(last_err.unwrap_or_else(consent_err))
}

pub(crate) fn advance_cursor(rr_cursor: &AtomicUsize, attempted: ProviderId) {
    if let Some(pos) = ProviderId::CLOUD_ORDER.iter().position(|&p| p == attempted) {
        rr_cursor.store((pos + 1) % ProviderId::CLOUD_ORDER.len(), Ordering::Relaxed);
    }
}

pub(crate) async fn invoke_provider(
    id: ProviderId,
    model_override: Option<&str>,
    env: &DispatchEnv<'_>,
    turn: &TurnInput<'_>,
) -> Result<(TutorReply, String), (SessionError, CloudFailure)> {
    let clean = model_override.map(str::trim).filter(|s| !s.is_empty());
    let model = clean.unwrap_or_else(|| id.default_model()).to_string();
    if id == ProviderId::Ollama {
        return call_ollama(clean, env.ollama_base, turn).await;
    }
    let Some((key, _)) = env.secrets.resolve(id) else {
        return Err((missing_key_err(id), CloudFailure::None));
    };
    let url = env
        .endpoints
        .get(&id)
        .cloned()
        .unwrap_or_else(|| id.resolve_endpoint_url());
    let reply = match id {
        ProviderId::Gemini => {
            GeminiProvider::new(url, &model, key)
                .generate_with_error(turn.history, turn.transcript, turn.mode)
                .await?
        }
        ProviderId::Groq | ProviderId::Openrouter => {
            OpenAiCompatProvider::new(id, url, &model, key)
                .generate_with_error(turn.history, turn.transcript, turn.mode)
                .await?
        }
        ProviderId::Ollama => unreachable!(),
    };
    Ok((reply, model))
}

pub(crate) async fn resolve_ollama_provider(
    clean_override: Option<&str>,
    ollama_base: &OllamaConfig,
) -> Result<OllamaAiProvider, (SessionError, CloudFailure)> {
    if let Some(m) = clean_override {
        let cfg = OllamaConfig::try_new(ollama_base.host(), ollama_base.port(), m)
            .map_err(|e| (e, CloudFailure::None))?;
        Ok(OllamaAiProvider::new(cfg))
    } else {
        discover_local_provider(ollama_base)
            .await
            .map_err(|e| (e, CloudFailure::None))
    }
}

async fn call_ollama(
    clean_override: Option<&str>,
    ollama_base: &OllamaConfig,
    turn: &TurnInput<'_>,
) -> Result<(TutorReply, String), (SessionError, CloudFailure)> {
    let provider = resolve_ollama_provider(clean_override, ollama_base).await?;
    let used = provider.config().model_name().to_string();
    let reply = provider
        .generate_tutor_reply(turn.history, turn.transcript, turn.mode)
        .await
        .map_err(|e| (e, CloudFailure::None))?;
    Ok((reply, used))
}

pub(crate) fn ok_routed(
    reply: TutorReply,
    provider: ProviderId,
    model: String,
    notes: &[String],
) -> RoutedTutorReply {
    let failover_note = (!notes.is_empty()).then(|| format!("Failover: {}", notes.join("; ")));
    RoutedTutorReply {
        reply,
        provider,
        model,
        is_cloud: provider.is_cloud(),
        failover_note,
    }
}

pub(crate) fn missing_key_err(id: ProviderId) -> SessionError {
    let var = id.env_var_name().unwrap_or("API_KEY");
    SessionError::new(
        SessionErrorKind::AiProviderUnavailable,
        format!("{} API key is not configured ({var}).", id.label()),
        true,
        None,
    )
}
