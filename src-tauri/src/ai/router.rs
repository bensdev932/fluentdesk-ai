//! Deep `TutorRouter` module behind `AiProvider`: enforces `cloud_opt_in`, resolves credentials,
//! cycles `Groq`/`Gemini`/`OpenRouter` with `Ollama` fallback, and tracks circuit breakers.

use crate::ai::{
    catalog::{ProviderId, ProviderPreference},
    ollama::OllamaConfig,
    router_exec::{build_candidate_list, execute_candidates, DispatchEnv, TurnInput},
    router_health::{build_providers_snapshot, evaluate_overall_health},
    router_types::{
        CircuitBreakers, ProvidersSnapshot, RoutedTutorReply, RouterConfigPatch, RouterHealthView,
        RouterState,
    },
    secrets::RuntimeSecretStore,
};
use crate::database::repository::SessionRepository;
use crate::domain::{ConversationTurn, CorrectionMode, SessionError, Transcript};
use crate::preferences::LearnerPreferencesStore;
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex, RwLock,
};

#[derive(Clone)]
pub struct TutorRouter {
    pub(crate) ollama_base: OllamaConfig,
    pub(crate) secrets: RuntimeSecretStore,
    pub(crate) state: Arc<RwLock<RouterState>>,
    pub(crate) breakers: Arc<Mutex<CircuitBreakers>>,
    pub(crate) rr_cursor: Arc<AtomicUsize>,
    prefs_store: Option<LearnerPreferencesStore>,
}

impl TutorRouter {
    /// Creates an isolated pure in-memory router (no SQLite DB or disk vault).
    pub fn new(ollama_base: OllamaConfig) -> Self {
        Self::build(
            ollama_base,
            RuntimeSecretStore::new(),
            RouterState::default(),
            None,
        )
    }

    /// Creates a persistent router backed by SQLite `session_settings` and DPAPI `credentials.dpapi`.
    pub async fn new_with_persistence(
        ollama_base: OllamaConfig,
        repository: SessionRepository,
        secret_dir: PathBuf,
    ) -> Result<Self, SessionError> {
        let store = LearnerPreferencesStore::new(repository);
        let prefs = store.get_preferences().await?;
        let state = RouterState::from_preferences(prefs);
        let secrets = RuntimeSecretStore::persistent(secret_dir);
        Ok(Self::build(ollama_base, secrets, state, Some(store)))
    }

    fn build(
        ollama_base: OllamaConfig,
        secrets: RuntimeSecretStore,
        state: RouterState,
        prefs_store: Option<LearnerPreferencesStore>,
    ) -> Self {
        Self {
            ollama_base,
            secrets,
            state: Arc::new(RwLock::new(state)),
            breakers: Arc::new(Mutex::new(CircuitBreakers::default())),
            rr_cursor: Arc::new(AtomicUsize::new(0)),
            prefs_store,
        }
    }

    pub fn has_persistence(&self) -> bool {
        self.prefs_store.is_some()
    }

    pub fn preferences_store(&self) -> Option<&LearnerPreferencesStore> {
        self.prefs_store.as_ref()
    }

    pub fn sync_preferences(&self, prefs: &crate::domain::LearnerPreferences) {
        self.state
            .write()
            .expect("lock")
            .sync_from_preferences(prefs);
    }

    pub fn set_endpoint_override(&self, id: ProviderId, url: impl Into<String>) {
        self.state
            .write()
            .expect("lock")
            .endpoint_overrides
            .insert(id, url.into());
    }

    pub async fn apply_config_patch(&self, patch: RouterConfigPatch) -> Result<(), SessionError> {
        self.breakers
            .lock()
            .expect("lock")
            .apply_patch_resets(&patch);
        RouterState::apply_keys(&patch, &self.secrets);
        let routing_patch = patch.to_preferences_patch();
        if let Some(store) = &self.prefs_store {
            let updated = store.patch_preferences(routing_patch).await?;
            self.state
                .write()
                .expect("lock")
                .sync_from_preferences(&updated);
        } else {
            self.state
                .write()
                .expect("lock")
                .apply_routing_patch(routing_patch);
        }
        Ok(())
    }

    pub async fn providers_snapshot(&self) -> ProvidersSnapshot {
        let st = self.load_routing_state_or_cached().await;
        build_providers_snapshot(
            &self.ollama_base,
            &self.secrets,
            st.prefs.cloud_opt_in,
            st.prefs.preferred_provider,
            &st.prefs.model_overrides,
            |id| self.breakers.lock().expect("lock").status(id),
        )
        .await
    }

    pub async fn overall_health(&self) -> RouterHealthView {
        let st = self.load_routing_state_or_cached().await;
        evaluate_overall_health(
            &self.ollama_base,
            &self.secrets,
            st.prefs.cloud_opt_in,
            st.prefs.preferred_provider,
            &st.prefs.model_overrides,
            |id| self.breakers.lock().expect("lock").status(id),
        )
        .await
    }

    async fn load_routing_state_or_cached(&self) -> RouterState {
        self.load_routing_state().await.unwrap_or_else(|err| {
            tracing::warn!(error = %err.user_message, "Failed refreshing SQLite routing preferences");
            self.state.read().expect("lock").clone()
        })
    }

    pub async fn route_turn(
        &self,
        history: &[ConversationTurn],
        transcript: &Transcript,
        mode: CorrectionMode,
        req_provider: Option<ProviderPreference>,
        req_model: Option<&str>,
    ) -> Result<RoutedTutorReply, SessionError> {
        let st = self.load_routing_state().await?;
        let effective = req_provider.unwrap_or(st.prefs.preferred_provider);
        let cursor = self.rr_cursor.load(Ordering::Relaxed);
        let cands = build_candidate_list(
            effective,
            st.prefs.cloud_opt_in,
            cursor,
            &self.secrets,
            |id| self.breakers.lock().expect("lock").is_healthy(id),
        )?;
        let env = DispatchEnv {
            ollama_base: &self.ollama_base,
            endpoints: &st.endpoint_overrides,
            overrides: &st.prefs.model_overrides,
            secrets: &self.secrets,
            breakers: &self.breakers,
            rr_cursor: &self.rr_cursor,
        };
        let turn = TurnInput {
            history,
            transcript,
            mode,
        };
        execute_candidates(
            &cands,
            &env,
            &turn,
            req_model,
            effective == ProviderPreference::Auto,
        )
        .await
    }

    pub(crate) async fn load_routing_state(&self) -> Result<RouterState, SessionError> {
        if let Some(store) = &self.prefs_store {
            let prefs = store.get_preferences().await?;
            let mut guard = self.state.write().expect("lock");
            guard.sync_from_preferences(&prefs);
            return Ok(guard.clone());
        }
        Ok(self.state.read().expect("lock").clone())
    }
}
