//! Local Ollama HTTP adapter and health/model checks (`T031`).

pub use crate::ai::ollama_config::OllamaConfig;
use crate::ai::{
    ollama_discovery, ollama_http,
    ollama_stream::stream_ollama_completion,
    provider::{AiProvider, TutorReplyStream},
};
use crate::domain::{
    ConversationTurn, CorrectionMode, PedagogicalScaffold, ProviderHealth, SessionError,
    Transcript, TutorReply,
};
use crate::modules::tutor::{
    feedback::parse_tutor_response_or_fallback,
    prompts::{assemble_scaffolded_tutor_prompt, assemble_voice_tutor_prompt, TutorPrompt},
};
use async_trait::async_trait;

/// Local Ollama adapter implementing `AiProvider` over localhost HTTP (`T031`).
#[derive(Clone)]
pub struct OllamaAiProvider {
    config: OllamaConfig,
}

impl OllamaAiProvider {
    pub fn new(config: OllamaConfig) -> Self {
        Self { config }
    }

    pub fn with_local_http(
        host: impl Into<String>,
        port: u16,
        model_name: impl Into<String>,
    ) -> Result<Self, SessionError> {
        Ok(Self::new(OllamaConfig::try_new(host, port, model_name)?))
    }

    pub fn config(&self) -> &OllamaConfig {
        &self.config
    }

    /// Resolves `OLLAMA_HOST` and auto-discovers an installed model via `/api/tags`.
    pub async fn discover_from_env() -> Result<Self, SessionError> {
        let base = ollama_discovery::ollama_config_from_env()?;
        ollama_discovery::discover_local_provider(&base).await
    }

    async fn generate_with_model_fallback(
        &self,
        prompt: &TutorPrompt,
    ) -> Result<String, SessionError> {
        match ollama_http::generate_completion(&self.config, prompt).await {
            Ok(raw) => Ok(raw),
            Err(err) if is_http_400(&err) && self.config.disable_thinking() => {
                let plain = self.config.clone().with_disable_thinking(false);
                ollama_http::generate_completion(&plain, prompt).await
            }
            Err(err) if is_http_404(&err) => self.retry_with_discovered_model(prompt, err).await,
            Err(err) => Err(err),
        }
    }

    async fn retry_with_discovered_model(
        &self,
        prompt: &TutorPrompt,
        original: SessionError,
    ) -> Result<String, SessionError> {
        let discovered = ollama_discovery::discover_local_provider(&self.config).await?;
        if discovered.config().model_name() == self.config.model_name() {
            return Err(original);
        }
        ollama_http::generate_completion(discovered.config(), prompt).await
    }

    async fn stream_with_model_fallback(
        &self,
        prompt: &TutorPrompt,
        mode: CorrectionMode,
    ) -> Result<TutorReplyStream, SessionError> {
        match stream_ollama_completion(&self.config, prompt, mode).await {
            Ok(rx) => Ok(rx),
            Err(err) if is_http_400(&err) && self.config.disable_thinking() => {
                let plain = self.config.clone().with_disable_thinking(false);
                stream_ollama_completion(&plain, prompt, mode).await
            }
            Err(err) if is_http_404(&err) => {
                let discovered = ollama_discovery::discover_local_provider(&self.config).await?;
                if discovered.config().model_name() == self.config.model_name() {
                    return Err(err);
                }
                stream_ollama_completion(discovered.config(), prompt, mode).await
            }
            Err(err) => Err(err),
        }
    }
}

fn is_http_400(err: &SessionError) -> bool {
    err.internal_detail.as_deref() == Some("HTTP 400")
}

fn is_http_404(err: &SessionError) -> bool {
    err.internal_detail.as_deref() == Some("HTTP 404")
}

#[async_trait]
impl AiProvider for OllamaAiProvider {
    async fn health_check(&self) -> ProviderHealth {
        match ollama_http::list_installed_models(&self.config).await {
            Ok(models) => evaluate_installed_models(&models, self.config.model_name()),
            Err(_) => ProviderHealth::Unavailable,
        }
    }

    async fn generate_tutor_reply(
        &self,
        history: &[ConversationTurn],
        user_transcript: &Transcript,
        correction_mode: CorrectionMode,
    ) -> Result<TutorReply, SessionError> {
        let prompt = assemble_voice_tutor_prompt(history, user_transcript, correction_mode);
        let raw = self.generate_with_model_fallback(&prompt).await?;
        parse_tutor_response_or_fallback(&raw, correction_mode)
    }

    async fn stream_routed_reply(
        &self,
        history: &[ConversationTurn],
        user_transcript: &Transcript,
        correction_mode: CorrectionMode,
    ) -> Result<TutorReplyStream, SessionError> {
        self.stream_scaffolded_reply(history, user_transcript, correction_mode, None)
            .await
    }

    async fn stream_scaffolded_reply(
        &self,
        history: &[ConversationTurn],
        user_transcript: &Transcript,
        correction_mode: CorrectionMode,
        scaffold: Option<&PedagogicalScaffold>,
    ) -> Result<TutorReplyStream, SessionError> {
        let prompt =
            assemble_scaffolded_tutor_prompt(history, user_transcript, correction_mode, scaffold);
        self.stream_with_model_fallback(&prompt, correction_mode)
            .await
    }
}

fn evaluate_installed_models(models: &[String], target: &str) -> ProviderHealth {
    let found = models
        .iter()
        .any(|m| m == target || m.starts_with(&format!("{target}:")));
    if found {
        ProviderHealth::Available
    } else {
        ProviderHealth::Misconfigured
    }
}
