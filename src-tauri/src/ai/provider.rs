//! `AiProvider` trait boundary with bounded context, token streaming, and cancellation (`T006`, `T029`).

pub use crate::ai::turn_request::{
    generate_routed_with_cancellation, generate_turn_with_cancellation, generate_with_cancellation,
    open_routed_stream_with_cancellation, open_scaffolded_stream_with_cancellation,
    open_turn_stream_with_cancellation, recv_tutor_stream_item, TutorTurnRequest,
};
use crate::ai::{catalog::ProviderId, router_types::RoutedTutorReply};
use crate::domain::{
    ConversationTurn, CorrectionMode, PedagogicalScaffold, ProviderHealth, SessionError,
    SessionErrorKind, Transcript, TutorReply,
};
use crate::modules::speech::spoken_text::split_spoken_sentences;
use async_trait::async_trait;
use tokio::sync::mpsc;

/// Incremental item emitted during streaming tutor response generation.
#[derive(Debug, Clone, PartialEq)]
pub enum TutorStreamItem {
    Sentence(String),
    Final(RoutedTutorReply),
}

/// Bounded receiver yielding spoken sentences followed by the validated final routed reply.
pub type TutorReplyStream = mpsc::Receiver<Result<TutorStreamItem, SessionError>>;

/// Replaceable LLM conversation engine adapter interface (`plan.md` Section 2 & 3, `T029`).
#[async_trait]
pub trait AiProvider: Send + Sync {
    async fn health_check(&self) -> ProviderHealth;

    async fn generate_tutor_reply(
        &self,
        history: &[ConversationTurn],
        user_transcript: &Transcript,
        correction_mode: CorrectionMode,
    ) -> Result<TutorReply, SessionError>;

    async fn generate_routed_reply(
        &self,
        history: &[ConversationTurn],
        user_transcript: &Transcript,
        correction_mode: CorrectionMode,
    ) -> Result<RoutedTutorReply, SessionError> {
        let reply = self
            .generate_tutor_reply(history, user_transcript, correction_mode)
            .await?;
        Ok(RoutedTutorReply {
            reply,
            provider: ProviderId::Ollama,
            model: "local".to_string(),
            is_cloud: false,
            failover_note: None,
        })
    }

    async fn stream_routed_reply(
        &self,
        history: &[ConversationTurn],
        user_transcript: &Transcript,
        correction_mode: CorrectionMode,
    ) -> Result<TutorReplyStream, SessionError> {
        let routed = self
            .generate_routed_reply(history, user_transcript, correction_mode)
            .await?;
        Ok(stream_completed_routed_reply(routed))
    }

    async fn stream_scaffolded_reply(
        &self,
        history: &[ConversationTurn],
        user_transcript: &Transcript,
        correction_mode: CorrectionMode,
        scaffold: Option<&PedagogicalScaffold>,
    ) -> Result<TutorReplyStream, SessionError> {
        let _ = scaffold;
        self.stream_routed_reply(history, user_transcript, correction_mode)
            .await
    }

    async fn generate_turn(
        &self,
        req: &TutorTurnRequest<'_>,
    ) -> Result<RoutedTutorReply, SessionError> {
        if req.scaffold.is_some() {
            let mut stream = self.stream_turn(req).await?;
            while let Some(item) = stream.recv().await {
                if let TutorStreamItem::Final(routed) = item? {
                    return Ok(routed);
                }
            }
        }
        self.generate_routed_reply(req.history, req.transcript, req.correction_mode)
            .await
    }

    async fn stream_turn(
        &self,
        req: &TutorTurnRequest<'_>,
    ) -> Result<TutorReplyStream, SessionError> {
        self.stream_scaffolded_reply(
            req.history,
            req.transcript,
            req.correction_mode,
            req.scaffold,
        )
        .await
    }
}

/// Wraps a completed `RoutedTutorReply` into a sentence-by-sentence `TutorReplyStream`.
pub(crate) fn stream_completed_routed_reply(routed: RoutedTutorReply) -> TutorReplyStream {
    let (tx, rx) = mpsc::channel(8);
    tokio::spawn(async move {
        let sentences = split_spoken_sentences(&routed.reply.spoken_reply);
        for sentence in sentences {
            if tx
                .send(Ok(TutorStreamItem::Sentence(sentence)))
                .await
                .is_err()
            {
                return;
            }
        }
        let _ = tx.send(Ok(TutorStreamItem::Final(routed))).await;
    });
    rx
}

pub(crate) fn cancelled_inference_error() -> SessionError {
    SessionError::new(
        SessionErrorKind::Cancelled,
        "Tutor response generation was cancelled.",
        true,
        None,
    )
}
