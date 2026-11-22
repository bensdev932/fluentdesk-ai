//! Speech-to-Text provider contract and transcript result schema (`T022`).

use crate::domain::{ProviderHealth, SessionError, Transcript};
use crate::modules::speech::utterance::CompletedUtterance;
pub use crate::stt::prompt_context::SttPromptContext;
use async_trait::async_trait;

/// Replaceable Speech-to-Text adapter interface (`T022`, `plan.md` Section 2 & 3).
#[async_trait]
pub trait SpeechToText: Send + Sync {
    /// Returns the current health of the speech recognition engine.
    async fn health_check(&self) -> ProviderHealth;

    /// Transcribes a normalized 16 kHz mono utterance directly into a validated `Transcript`.
    async fn transcribe(&self, utterance: &CompletedUtterance) -> Result<Transcript, SessionError>;

    /// Transcribes an utterance with optional topic and vocabulary biasing (`--prompt`).
    async fn transcribe_with_context(
        &self,
        utterance: &CompletedUtterance,
        context: Option<&SttPromptContext>,
    ) -> Result<Transcript, SessionError> {
        let _ = context;
        self.transcribe(utterance).await
    }
}
