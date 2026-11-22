//! Deterministic mock TTS provider (`T006`).

use crate::domain::{AudioChunk, AudioFormat, ProviderHealth, SessionError, SessionErrorKind};
use crate::tts::provider::TextToSpeech;
use async_trait::async_trait;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

/// Deterministic mock implementation of `TextToSpeech` (`T006`).
#[derive(Clone)]
pub struct MockTtsProvider {
    health: Arc<Mutex<ProviderHealth>>,
    responses: Arc<Mutex<VecDeque<Result<AudioChunk, SessionError>>>>,
    synthesized_texts: Arc<Mutex<Vec<(String, f32)>>>,
}

impl Default for MockTtsProvider {
    fn default() -> Self {
        Self::new(ProviderHealth::Available, Vec::new())
    }
}

impl MockTtsProvider {
    pub fn new(health: ProviderHealth, responses: Vec<Result<AudioChunk, SessionError>>) -> Self {
        Self {
            health: Arc::new(Mutex::new(health)),
            responses: Arc::new(Mutex::new(VecDeque::from(responses))),
            synthesized_texts: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn synthesized_calls(&self) -> Vec<(String, f32)> {
        self.synthesized_texts
            .lock()
            .expect("lock tts calls")
            .clone()
    }
}

#[async_trait]
impl TextToSpeech for MockTtsProvider {
    async fn health_check(&self) -> ProviderHealth {
        *self.health.lock().expect("lock tts health")
    }

    async fn synthesize(
        &self,
        spoken_text: &str,
        speaking_speed: f32,
    ) -> Result<AudioChunk, SessionError> {
        self.synthesized_texts
            .lock()
            .expect("lock tts calls")
            .push((spoken_text.to_string(), speaking_speed));

        if self.health_check().await != ProviderHealth::Available {
            return Err(SessionError::new(
                SessionErrorKind::TtsUnavailable,
                "Local voice synthesis engine is unavailable.",
                true,
                None,
            ));
        }

        if spoken_text.trim().is_empty() {
            return Err(SessionError::new(
                SessionErrorKind::TtsFailed,
                "Cannot synthesize empty text.",
                true,
                None,
            ));
        }

        if let Some(next) = self
            .responses
            .lock()
            .expect("lock tts responses")
            .pop_front()
        {
            return next;
        }

        AudioChunk::new_bounded(
            AudioFormat::WHISPER_16KHZ_MONO_F32,
            vec![0.02_f32; 320],
            0,
            20,
        )
        .map_err(|err| {
            SessionError::new(
                SessionErrorKind::TtsFailed,
                "Failed to build synthesized audio chunk.",
                true,
                Some(err.to_string()),
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::AudioFormat;

    #[tokio::test]
    async fn mock_tts_synthesizes_audio_chunk_and_records_text() {
        let tts = MockTtsProvider::default();
        let chunk = tts
            .synthesize("Hello! How was your day?", 1.0)
            .await
            .expect("synthesize");

        assert_eq!(chunk.format, AudioFormat::WHISPER_16KHZ_MONO_F32);
        assert!(!chunk.samples_f32.is_empty());
        assert_eq!(
            tts.synthesized_calls(),
            vec![("Hello! How was your day?".to_string(), 1.0)]
        );
    }
}
