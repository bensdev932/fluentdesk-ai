//! Deterministic mock AI provider (`T006`).

use crate::ai::provider::{stream_completed_routed_reply, AiProvider, TutorReplyStream};
use crate::ai::{catalog::ProviderId, router_types::RoutedTutorReply};
use crate::domain::{
    ConversationTurn, CorrectionMode, PedagogicalScaffold, ProviderHealth, SessionError,
    SessionErrorKind, Transcript, TutorReply,
};
use async_trait::async_trait;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

/// Deterministic mock implementation of `AiProvider` (`T006`).
#[derive(Clone)]
pub struct MockAiProvider {
    health: Arc<Mutex<ProviderHealth>>,
    responses: Arc<Mutex<VecDeque<Result<TutorReply, SessionError>>>>,
    recorded_prompts: Arc<Mutex<Vec<(usize, String, CorrectionMode)>>>,
    recorded_scaffolds: Arc<Mutex<Vec<Option<PedagogicalScaffold>>>>,
}

impl Default for MockAiProvider {
    fn default() -> Self {
        Self::new(ProviderHealth::Available, Vec::new())
    }
}

impl MockAiProvider {
    pub fn new(health: ProviderHealth, responses: Vec<Result<TutorReply, SessionError>>) -> Self {
        Self {
            health: Arc::new(Mutex::new(health)),
            responses: Arc::new(Mutex::new(VecDeque::from(responses))),
            recorded_prompts: Arc::new(Mutex::new(Vec::new())),
            recorded_scaffolds: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn recorded_calls(&self) -> Vec<(usize, String, CorrectionMode)> {
        self.recorded_prompts
            .lock()
            .expect("lock recorded ai calls")
            .clone()
    }

    pub fn recorded_scaffolds(&self) -> Vec<Option<PedagogicalScaffold>> {
        self.recorded_scaffolds
            .lock()
            .expect("lock recorded scaffolds")
            .clone()
    }
}

#[async_trait]
impl AiProvider for MockAiProvider {
    async fn health_check(&self) -> ProviderHealth {
        *self.health.lock().expect("lock ai health")
    }

    async fn generate_tutor_reply(
        &self,
        history: &[ConversationTurn],
        user_transcript: &Transcript,
        correction_mode: CorrectionMode,
    ) -> Result<TutorReply, SessionError> {
        self.recorded_prompts
            .lock()
            .expect("lock recorded ai calls")
            .push((history.len(), user_transcript.text.clone(), correction_mode));

        if self.health_check().await != ProviderHealth::Available {
            return Err(SessionError::new(
                SessionErrorKind::AiProviderUnavailable,
                "Local AI tutor provider is unavailable.",
                true,
                None,
            ));
        }

        if let Some(next) = self
            .responses
            .lock()
            .expect("lock ai responses")
            .pop_front()
        {
            return next;
        }

        TutorReply::new_validated(
            format!("That is interesting! You said: {}", user_transcript.text),
            None,
            Some("Could you tell me more about that?".to_string()),
        )
        .map_err(|err| {
            SessionError::new(
                SessionErrorKind::AiInferenceFailed,
                "Failed to validate mock tutor reply.",
                true,
                Some(err.to_string()),
            )
        })
    }

    async fn stream_scaffolded_reply(
        &self,
        history: &[ConversationTurn],
        user_transcript: &Transcript,
        correction_mode: CorrectionMode,
        scaffold: Option<&PedagogicalScaffold>,
    ) -> Result<TutorReplyStream, SessionError> {
        self.recorded_scaffolds
            .lock()
            .expect("lock recorded scaffolds")
            .push(scaffold.cloned());
        let reply = self
            .generate_tutor_reply(history, user_transcript, correction_mode)
            .await?;
        Ok(stream_completed_routed_reply(RoutedTutorReply {
            reply,
            provider: ProviderId::Ollama,
            model: "local".to_string(),
            is_cloud: false,
            failover_note: None,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mock_ai_provider_returns_scripted_tutor_reply_and_tracks_context() {
        let reply = TutorReply::new_validated(
            "Sure! Would you like your coffee hot or iced?",
            Some("Great polite phrasing!".to_string()),
            Some("Do you want any milk or sugar?".to_string()),
        )
        .unwrap();

        let ai = MockAiProvider::new(ProviderHealth::Available, vec![Ok(reply.clone())]);
        let transcript =
            Transcript::new("I would like a cup of coffee.", "en", 1400, Some(0.95)).unwrap();

        let result = ai
            .generate_tutor_reply(&[], &transcript, CorrectionMode::Gentle)
            .await
            .expect("generate reply");

        assert_eq!(result, reply);
        assert_eq!(
            ai.recorded_calls(),
            vec![(
                0,
                "I would like a cup of coffee.".to_string(),
                CorrectionMode::Gentle
            )]
        );
    }
}
