//! Cancellation-aware STT & event-emission helpers for spoken utterance processing.

use crate::domain::{SessionError, SessionErrorKind, SessionState};
use crate::modules::conversation::{
    cancellation::SessionCancellationToken,
    host_spoken_stream::{AbortOnDropTts, TtsJoinHandle},
    host_turns::cancelled_turn_err,
    turn::EndToEndTurnResult,
};
use crate::modules::speech::{
    playback::AudioOutput,
    transcription::{transcribe_completed_utterance_with_context, UserTranscriptReadyEvent},
    utterance::CompletedUtterance,
};
use crate::stt::provider::{SpeechToText, SttPromptContext};
use tokio::sync::mpsc;

/// Incremental stage/sentence event emitted during `process_spoken_utterance_with_events`.
#[derive(Debug, Clone)]
pub enum SpokenTurnEvent {
    StateChanged(SessionState),
    UserTranscript(UserTranscriptReadyEvent),
    Sentence(String),
}

pub(super) async fn emit_spoken_ev(
    tx: Option<&mpsc::Sender<SpokenTurnEvent>>,
    ev: SpokenTurnEvent,
) {
    if let Some(sender) = tx {
        let _ = sender.send(ev).await;
    }
}

pub(super) async fn emit_state_ev(tx: Option<&mpsc::Sender<SpokenTurnEvent>>, st: SessionState) {
    emit_spoken_ev(tx, SpokenTurnEvent::StateChanged(st)).await;
}

pub(super) async fn abort_tts_on_err<T>(
    out: &dyn AudioOutput,
    tts_guard: AbortOnDropTts,
    err: SessionError,
) -> Result<T, SessionError> {
    if err.kind != SessionErrorKind::Cancelled {
        let _ = out.stop_playback().await;
    }
    let handle: TtsJoinHandle = tts_guard.into_inner();
    if err.kind == SessionErrorKind::Cancelled {
        handle.abort();
    } else {
        let _ = handle.await;
    }
    Err(err)
}

pub(super) async fn transcribe_with_cancel(
    stt: &dyn SpeechToText,
    utt: &CompletedUtterance,
    ctx: &SttPromptContext,
    cancel: &mut SessionCancellationToken,
) -> Result<UserTranscriptReadyEvent, SessionError> {
    if cancel.is_cancelled() {
        return Err(cancelled_turn_err());
    }
    tokio::select! {
        biased;
        _ = cancel.wait_for_cancellation() => Err(cancelled_turn_err()),
        res = transcribe_completed_utterance_with_context(stt, utt, Some(ctx)) => res,
    }
}

pub(super) fn empty_utterance_result(
    transcript_event: UserTranscriptReadyEvent,
) -> EndToEndTurnResult {
    EndToEndTurnResult {
        transcript_event,
        tutor_text_event: None,
        playback_event: None,
        final_state: SessionState::Listening,
    }
}
