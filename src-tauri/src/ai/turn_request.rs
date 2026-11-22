//! Unified turn-request parameter object and cancellation wrappers for `AiProvider`.

use crate::ai::provider::{
    cancelled_inference_error, AiProvider, TutorReplyStream, TutorStreamItem,
};
use crate::ai::router_types::RoutedTutorReply;
use crate::domain::{
    ConversationTurn, CorrectionMode, PedagogicalScaffold, SessionError, Transcript, TutorReply,
};
use crate::modules::conversation::cancellation::SessionCancellationToken;

/// Parameter object collapsing conversation history, transcript, correction mode, and scaffold.
#[derive(Debug, Clone, Copy)]
pub struct TutorTurnRequest<'a> {
    pub history: &'a [ConversationTurn],
    pub transcript: &'a Transcript,
    pub correction_mode: CorrectionMode,
    pub scaffold: Option<&'a PedagogicalScaffold>,
}

impl<'a> TutorTurnRequest<'a> {
    pub fn new(
        history: &'a [ConversationTurn],
        transcript: &'a Transcript,
        correction_mode: CorrectionMode,
        scaffold: Option<&'a PedagogicalScaffold>,
    ) -> Self {
        Self {
            history,
            transcript,
            correction_mode,
            scaffold,
        }
    }
}

/// Executes `AiProvider::generate_tutor_reply` with cooperative cancellation support (`T029`).
pub async fn generate_with_cancellation(
    provider: &dyn AiProvider,
    history: &[ConversationTurn],
    user_transcript: &Transcript,
    correction_mode: CorrectionMode,
    cancellation: &mut SessionCancellationToken,
) -> Result<TutorReply, SessionError> {
    let req = TutorTurnRequest::new(history, user_transcript, correction_mode, None);
    generate_turn_with_cancellation(provider, &req, cancellation)
        .await
        .map(|routed| routed.reply)
}

/// Executes `AiProvider::generate_routed_reply` with cooperative cancellation support.
pub async fn generate_routed_with_cancellation(
    provider: &dyn AiProvider,
    history: &[ConversationTurn],
    user_transcript: &Transcript,
    correction_mode: CorrectionMode,
    cancellation: &mut SessionCancellationToken,
) -> Result<RoutedTutorReply, SessionError> {
    let req = TutorTurnRequest::new(history, user_transcript, correction_mode, None);
    generate_turn_with_cancellation(provider, &req, cancellation).await
}

/// Executes `AiProvider::generate_turn` with cooperative cancellation support.
pub async fn generate_turn_with_cancellation(
    provider: &dyn AiProvider,
    req: &TutorTurnRequest<'_>,
    cancellation: &mut SessionCancellationToken,
) -> Result<RoutedTutorReply, SessionError> {
    if cancellation.is_cancelled() {
        return Err(cancelled_inference_error());
    }
    tokio::select! {
        biased;
        _ = cancellation.wait_for_cancellation() => Err(cancelled_inference_error()),
        result = provider.generate_turn(req) => result,
    }
}

/// Opens `AiProvider::stream_routed_reply` with cooperative cancellation support.
pub async fn open_routed_stream_with_cancellation(
    provider: &dyn AiProvider,
    history: &[ConversationTurn],
    user_transcript: &Transcript,
    correction_mode: CorrectionMode,
    cancellation: &mut SessionCancellationToken,
) -> Result<TutorReplyStream, SessionError> {
    let req = TutorTurnRequest::new(history, user_transcript, correction_mode, None);
    open_turn_stream_with_cancellation(provider, &req, cancellation).await
}

/// Opens `AiProvider::stream_scaffolded_reply` with cooperative cancellation support.
pub async fn open_scaffolded_stream_with_cancellation(
    provider: &dyn AiProvider,
    history: &[ConversationTurn],
    user_transcript: &Transcript,
    correction_mode: CorrectionMode,
    scaffold: Option<&PedagogicalScaffold>,
    cancellation: &mut SessionCancellationToken,
) -> Result<TutorReplyStream, SessionError> {
    let req = TutorTurnRequest::new(history, user_transcript, correction_mode, scaffold);
    open_turn_stream_with_cancellation(provider, &req, cancellation).await
}

/// Opens `AiProvider::stream_turn` with cooperative cancellation support.
pub async fn open_turn_stream_with_cancellation(
    provider: &dyn AiProvider,
    req: &TutorTurnRequest<'_>,
    cancellation: &mut SessionCancellationToken,
) -> Result<TutorReplyStream, SessionError> {
    if cancellation.is_cancelled() {
        return Err(cancelled_inference_error());
    }
    tokio::select! {
        biased;
        _ = cancellation.wait_for_cancellation() => Err(cancelled_inference_error()),
        result = provider.stream_turn(req) => result,
    }
}

/// Receives the next `TutorStreamItem` while monitoring cooperative cancellation.
pub async fn recv_tutor_stream_item(
    stream: &mut TutorReplyStream,
    cancellation: &mut SessionCancellationToken,
) -> Result<Option<TutorStreamItem>, SessionError> {
    if cancellation.is_cancelled() {
        return Err(cancelled_inference_error());
    }
    tokio::select! {
        biased;
        _ = cancellation.wait_for_cancellation() => Err(cancelled_inference_error()),
        msg = stream.recv() => match msg {
            Some(Ok(item)) => Ok(Some(item)),
            Some(Err(err)) => Err(err),
            None => Ok(None),
        },
    }
}
