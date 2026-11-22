//! Deterministic mock STT provider (`T006`).

use crate::domain::{ProviderHealth, SessionError, SessionErrorKind, Transcript};
use crate::modules::speech::utterance::CompletedUtterance;
use crate::stt::provider::{SpeechToText, SttPromptContext};
use async_trait::async_trait;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

/// Deterministic mock implementation of `SpeechToText` (`T006`).
#[derive(Clone)]
pub struct MockSttProvider {
    health: Arc<Mutex<ProviderHealth>>,
    responses: Arc<Mutex<VecDeque<Result<Transcript, SessionError>>>>,
    calls: Arc<Mutex<usize>>,
    utterance_stats: Arc<Mutex<Vec<(usize, u32)>>>,
    prompt_contexts: Arc<Mutex<Vec<Option<String>>>>,
}

impl Default for MockSttProvider {
    fn default() -> Self {
        Self::new(ProviderHealth::Available, Vec::new())
    }
}

impl MockSttProvider {
    pub fn new(health: ProviderHealth, responses: Vec<Result<Transcript, SessionError>>) -> Self {
        Self {
            health: Arc::new(Mutex::new(health)),
            responses: Arc::new(Mutex::new(VecDeque::from(responses))),
            calls: Arc::new(Mutex::new(0)),
            utterance_stats: Arc::new(Mutex::new(Vec::new())),
            prompt_contexts: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn call_count(&self) -> usize {
        *self.calls.lock().expect("lock stt call count")
    }

    pub fn recorded_utterance_stats(&self) -> Vec<(usize, u32)> {
        self.utterance_stats
            .lock()
            .expect("lock stt utterance stats")
            .clone()
    }

    pub fn recorded_prompts(&self) -> Vec<Option<String>> {
        self.prompt_contexts
            .lock()
            .expect("lock stt prompts")
            .clone()
    }

    async fn record_and_transcribe(
        &self,
        utterance: &CompletedUtterance,
        prompt: Option<String>,
    ) -> Result<Transcript, SessionError> {
        *self.calls.lock().expect("lock stt call count") += 1;
        self.utterance_stats
            .lock()
            .expect("lock stt utterance stats")
            .push((utterance.sample_count(), utterance.duration_ms()));
        self.prompt_contexts
            .lock()
            .expect("lock stt prompts")
            .push(prompt);
        if self.health_check().await != ProviderHealth::Available {
            return Err(unavailable_stt_error());
        }
        if let Some(next) = self
            .responses
            .lock()
            .expect("lock stt responses")
            .pop_front()
        {
            return next;
        }
        default_mock_transcript(utterance)
    }
}

fn default_mock_transcript(utterance: &CompletedUtterance) -> Result<Transcript, SessionError> {
    Transcript::new(
        "Mock recognized speech.",
        "en",
        utterance.duration_ms(),
        Some(0.95),
    )
    .map_err(|err| {
        SessionError::new(
            SessionErrorKind::SttFailed,
            "Failed to construct transcript.",
            true,
            Some(err.to_string()),
        )
    })
}

#[async_trait]
impl SpeechToText for MockSttProvider {
    async fn health_check(&self) -> ProviderHealth {
        *self.health.lock().expect("lock stt health")
    }

    async fn transcribe(&self, utterance: &CompletedUtterance) -> Result<Transcript, SessionError> {
        self.record_and_transcribe(utterance, None).await
    }

    async fn transcribe_with_context(
        &self,
        utterance: &CompletedUtterance,
        context: Option<&SttPromptContext>,
    ) -> Result<Transcript, SessionError> {
        let prompt = context.and_then(SttPromptContext::format_whisper_prompt);
        self.record_and_transcribe(utterance, prompt).await
    }
}

fn unavailable_stt_error() -> SessionError {
    SessionError::new(
        SessionErrorKind::SttUnavailable,
        "Speech recognition model is unavailable.",
        true,
        None,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mock_stt_returns_scripted_transcript_and_records_direct_utterance() {
        let expected =
            Transcript::new("I would like a cup of coffee.", "en", 1500, Some(0.98)).unwrap();
        let stt = MockSttProvider::new(ProviderHealth::Available, vec![Ok(expected.clone())]);

        assert_eq!(stt.health_check().await, ProviderHealth::Available);
        let utt = CompletedUtterance::try_new(vec![0.05; 24_000], 1500, false).unwrap();

        let actual = stt.transcribe(&utt).await.expect("transcribe");
        assert_eq!(actual, expected);
        assert_eq!(stt.call_count(), 1);
        assert_eq!(stt.recorded_utterance_stats(), vec![(24_000, 1500)]);
    }
}
