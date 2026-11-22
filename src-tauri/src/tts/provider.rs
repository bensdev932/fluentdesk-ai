//! `TextToSpeech` trait boundary with streaming synthesis, speed validation, and cancellation (`T006`, `T038`).

pub use crate::domain::SpeakingSpeed;
use crate::domain::{AudioChunk, ProviderHealth, SessionError, SessionErrorKind};
use crate::modules::conversation::cancellation::SessionCancellationToken;
use async_trait::async_trait;
use tokio::sync::mpsc;

/// Bounded stream of synthesized sentence audio chunks.
pub type TtsAudioStream = mpsc::Receiver<Result<AudioChunk, SessionError>>;

/// Replaceable Text-to-Speech adapter interface (`plan.md` Section 2 & 3, `T038`).
#[async_trait]
pub trait TextToSpeech: Send + Sync {
    async fn health_check(&self) -> ProviderHealth;

    async fn synthesize(
        &self,
        spoken_text: &str,
        speaking_speed: f32,
    ) -> Result<AudioChunk, SessionError>;

    async fn synthesize_stream(
        &self,
        spoken_text: &str,
        speaking_speed: f32,
    ) -> Result<TtsAudioStream, SessionError> {
        let chunk = self.synthesize(spoken_text, speaking_speed).await?;
        let (tx, rx) = mpsc::channel(1);
        let _ = tx.send(Ok(chunk)).await;
        Ok(rx)
    }
}

/// Executes `TextToSpeech::synthesize` with cooperative cancellation support (`T038`).
pub async fn synthesize_with_cancellation(
    tts: &dyn TextToSpeech,
    spoken_text: &str,
    speaking_speed: SpeakingSpeed,
    cancellation: &mut SessionCancellationToken,
) -> Result<AudioChunk, SessionError> {
    if cancellation.is_cancelled() {
        return Err(cancelled_tts_error());
    }
    tokio::select! {
        biased;
        _ = cancellation.wait_for_cancellation() => Err(cancelled_tts_error()),
        res = tts.synthesize(spoken_text, speaking_speed.as_f32()) => res,
    }
}

/// Opens `TextToSpeech::synthesize_stream` with cooperative cancellation support.
pub async fn synthesize_stream_with_cancellation(
    tts: &dyn TextToSpeech,
    spoken_text: &str,
    speaking_speed: SpeakingSpeed,
    cancellation: &mut SessionCancellationToken,
) -> Result<TtsAudioStream, SessionError> {
    if cancellation.is_cancelled() {
        return Err(cancelled_tts_error());
    }
    tokio::select! {
        biased;
        _ = cancellation.wait_for_cancellation() => Err(cancelled_tts_error()),
        res = tts.synthesize_stream(spoken_text, speaking_speed.as_f32()) => res,
    }
}

fn cancelled_tts_error() -> SessionError {
    SessionError::new(
        SessionErrorKind::Cancelled,
        "Voice synthesis was cancelled.",
        true,
        None,
    )
}
