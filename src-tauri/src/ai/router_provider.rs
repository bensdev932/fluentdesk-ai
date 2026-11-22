//! `AiProvider` trait implementation for `TutorRouter`.

use crate::ai::{
    provider::{AiProvider, TutorReplyStream},
    router::TutorRouter,
    router_types::RoutedTutorReply,
};
use crate::domain::{
    ConversationTurn, CorrectionMode, PedagogicalScaffold, ProviderHealth, SessionError,
    Transcript, TutorReply,
};
use async_trait::async_trait;

#[async_trait]
impl AiProvider for TutorRouter {
    async fn health_check(&self) -> ProviderHealth {
        if self.overall_health().await.status == "available" {
            ProviderHealth::Available
        } else {
            ProviderHealth::Unavailable
        }
    }

    async fn generate_tutor_reply(
        &self,
        history: &[ConversationTurn],
        user_transcript: &Transcript,
        correction_mode: CorrectionMode,
    ) -> Result<TutorReply, SessionError> {
        self.generate_routed_reply(history, user_transcript, correction_mode)
            .await
            .map(|r| r.reply)
    }

    async fn generate_routed_reply(
        &self,
        history: &[ConversationTurn],
        user_transcript: &Transcript,
        correction_mode: CorrectionMode,
    ) -> Result<RoutedTutorReply, SessionError> {
        self.route_turn(history, user_transcript, correction_mode, None, None)
            .await
    }

    async fn stream_routed_reply(
        &self,
        history: &[ConversationTurn],
        user_transcript: &Transcript,
        correction_mode: CorrectionMode,
    ) -> Result<TutorReplyStream, SessionError> {
        self.route_stream_turn(history, user_transcript, correction_mode, None, None)
            .await
    }

    async fn stream_scaffolded_reply(
        &self,
        history: &[ConversationTurn],
        user_transcript: &Transcript,
        correction_mode: CorrectionMode,
        scaffold: Option<&PedagogicalScaffold>,
    ) -> Result<TutorReplyStream, SessionError> {
        self.route_stream_scaffolded_turn(
            history,
            user_transcript,
            correction_mode,
            scaffold,
            None,
            None,
        )
        .await
    }
}
