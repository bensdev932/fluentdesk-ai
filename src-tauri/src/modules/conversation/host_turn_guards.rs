//! Cancellation, state-transition, and stage-guard helpers for `SpeakingSessionHost`.

use crate::domain::{SessionError, SessionErrorKind, SessionId, SessionState};
use crate::modules::conversation::{
    cancellation::SessionCancellationToken,
    host::{verify_active_mut, SpeakingSessionHost},
    manager::SessionManager,
    persistence::invalid_transition_err,
    turn::{EndToEndTurnResult, TutorTextReadyEvent},
};
use crate::modules::speech::{
    synthesis::TutorPlaybackEvent, transcription::UserTranscriptReadyEvent,
};
use crate::stt::provider::{SpeechToText, SttPromptContext};
use std::sync::Arc;
use std::time::Instant;

impl SpeakingSessionHost {
    pub(super) async fn prepare_stt_stage(
        &self,
        sid: &SessionId,
    ) -> Result<
        (
            Arc<dyn SpeechToText>,
            SttPromptContext,
            SessionCancellationToken,
        ),
        SessionError,
    > {
        let mut guard = self.active.lock().await;
        let mgr = verify_active_mut(guard.as_mut(), sid)?;
        require_state(mgr, SessionState::Listening, SessionState::Processing)?;
        mgr.speech_intake.reset();
        mgr.transition_to(SessionState::Processing)?;
        Ok((
            mgr.adapters.stt.clone(),
            mgr.stt_prompt_context(),
            mgr.cancellation.clone(),
        ))
    }

    pub(super) async fn after_stt_stage(
        &self,
        sid: &SessionId,
        res: Result<UserTranscriptReadyEvent, SessionError>,
        cancel: &SessionCancellationToken,
    ) -> Result<(UserTranscriptReadyEvent, bool), SessionError> {
        let mut guard = self.active.lock().await;
        let mgr = verify_turn_active(guard.as_mut(), sid, cancel)?;
        let ev = fail_if_err(mgr, res)?;
        if ev.is_empty_or_hallucinated {
            mgr.transition_to(SessionState::Listening)?;
            return Ok((ev, false));
        }
        mgr.transition_to(SessionState::Thinking)?;
        Ok((ev, true))
    }

    pub(super) async fn finish_spoken_stage(
        &self,
        sid: &SessionId,
        tx_ev: UserTranscriptReadyEvent,
        tutor_ev: TutorTextReadyEvent,
        res: Result<TutorPlaybackEvent, SessionError>,
        now_ms: i64,
        cancel: &SessionCancellationToken,
    ) -> Result<EndToEndTurnResult, SessionError> {
        let mut guard = self.active.lock().await;
        let mgr = verify_turn_active(guard.as_mut(), sid, cancel)?;
        let pb = fail_if_err(mgr, res)?;
        let trans = mgr.complete_turn_transition(now_ms).await;
        fail_if_err(mgr, trans)?;
        Ok(EndToEndTurnResult {
            transcript_event: tx_ev,
            tutor_text_event: Some(tutor_ev),
            playback_event: Some(pb),
            final_state: mgr.state,
        })
    }
}

pub(crate) fn verify_turn_active<'a>(
    mgr: Option<&'a mut SessionManager>,
    sid: &SessionId,
    cancel: &SessionCancellationToken,
) -> Result<&'a mut SessionManager, SessionError> {
    if cancel.is_cancelled() {
        return Err(cancelled_turn_err());
    }
    verify_still_active(mgr, sid)
}

pub(crate) fn verify_still_active<'a>(
    mgr: Option<&'a mut SessionManager>,
    sid: &SessionId,
) -> Result<&'a mut SessionManager, SessionError> {
    let mgr = verify_active_mut(mgr, sid)?;
    if mgr.is_cancelled() || mgr.state == SessionState::Ended {
        return Err(cancelled_turn_err());
    }
    Ok(mgr)
}

pub(crate) fn require_state(
    mgr: &SessionManager,
    expected: SessionState,
    target: SessionState,
) -> Result<(), SessionError> {
    if mgr.state != expected {
        return Err(invalid_transition_err(mgr.state, target));
    }
    Ok(())
}

pub(crate) fn fail_if_err<T>(
    mgr: &mut SessionManager,
    res: Result<T, SessionError>,
) -> Result<T, SessionError> {
    if let Err(ref err) = res {
        if err.kind != SessionErrorKind::Cancelled && mgr.state != SessionState::Ended {
            let _ = mgr.transition_to(SessionState::Error);
        }
    }
    res
}

pub(crate) fn elapsed_ms(started: Instant) -> u32 {
    u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX)
}

pub(crate) async fn cancel_mgr_turn(mgr: &mut SessionManager) -> bool {
    if !mgr.state().is_active() {
        return false;
    }
    mgr.cancellation.cancel();
    mgr.cancellation = SessionCancellationToken::new();
    reset_mgr_listening(mgr).await;
    true
}

async fn reset_mgr_listening(mgr: &mut SessionManager) {
    let _ = mgr.adapters.audio_output.stop_playback().await;
    mgr.speech_intake.reset();
    if matches!(
        mgr.state,
        SessionState::Processing | SessionState::Thinking | SessionState::Speaking
    ) {
        let _ = mgr.transition_to(SessionState::Listening);
    }
}

pub(crate) fn cancelled_turn_err() -> SessionError {
    SessionError::new(
        SessionErrorKind::Cancelled,
        "Speaking turn was cancelled.",
        true,
        None,
    )
}
