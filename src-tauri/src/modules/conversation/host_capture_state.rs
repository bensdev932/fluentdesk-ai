//! Internal state synchronization helpers for `SpeakingSessionHost` capture pumping.

use crate::domain::{AudioChunk, SessionError, SessionId, SessionState};
use crate::modules::conversation::{
    cancellation::SessionCancellationToken,
    host::{verify_active_mut, SpeakingSessionHost},
    host_turns::{cancelled_turn_err, require_state, verify_still_active},
    manager::SessionManager,
    persistence::invalid_transition_err,
};
use crate::modules::speech::intake::CompletedUtterance;
use std::sync::Arc;
use tokio::sync::{mpsc, Notify};

pub(super) struct PumpSession {
    pub(super) rx: mpsc::Receiver<AudioChunk>,
    pub(super) cancel: SessionCancellationToken,
    pub(super) capture_cancel: SessionCancellationToken,
    pub(super) ptt_notify: Arc<Notify>,
}

impl SpeakingSessionHost {
    pub(super) async fn begin_capture_pump(
        &self,
        sid: &SessionId,
    ) -> Result<Option<PumpSession>, SessionError> {
        let mut guard = self.active.lock().await;
        let mgr = verify_active_mut(guard.as_mut(), sid)?;
        check_pump_startable(mgr)?;
        let Some(rx) = mgr.capture_rx.take() else {
            return Ok(None);
        };
        mgr.capture_pump_active = true;
        mgr.pending_ptt_release = false;
        Ok(Some(build_pump_session(mgr, rx)))
    }

    pub(super) async fn check_cancel_flags(
        &self,
        sid: &SessionId,
        p: &PumpSession,
    ) -> Option<Result<Option<CompletedUtterance>, SessionError>> {
        if p.cancel.is_cancelled() {
            return Some(self.abort_pump(sid, true).await);
        }
        if p.capture_cancel.is_cancelled() {
            return Some(self.abort_pump(sid, false).await);
        }
        None
    }

    pub(super) async fn release_pending_ptt_on_host(
        &self,
        sid: &SessionId,
    ) -> Result<Option<CompletedUtterance>, SessionError> {
        self.with_active_pump(sid, release_pending_ptt).await
    }

    pub(super) async fn feed_pumped_chunk(
        &self,
        sid: &SessionId,
        chunk: &AudioChunk,
    ) -> Result<Option<CompletedUtterance>, SessionError> {
        self.with_active_pump(sid, |mgr| {
            mgr.handle_pumped_chunk(chunk)
                .inspect_err(|_| reset_pump_state(mgr))
        })
        .await
    }

    pub(super) async fn complete_pump(
        &self,
        sid: &SessionId,
        rx: mpsc::Receiver<AudioChunk>,
        utt: CompletedUtterance,
    ) -> Result<Option<CompletedUtterance>, SessionError> {
        self.with_active_pump(sid, |mgr| {
            mgr.capture_pump_active = false;
            mgr.pending_ptt_release = false;
            if mgr.state != SessionState::Listening {
                return Ok(None);
            }
            mgr.capture_rx = Some(rx);
            Ok(Some(utt))
        })
        .await
    }

    pub(super) async fn abort_pump(
        &self,
        sid: &SessionId,
        cancelled: bool,
    ) -> Result<Option<CompletedUtterance>, SessionError> {
        let mut guard = self.active.lock().await;
        clear_pump_flags(guard.as_mut(), sid);
        if cancelled {
            Err(cancelled_turn_err())
        } else {
            Ok(None)
        }
    }

    async fn with_active_pump<F>(
        &self,
        sid: &SessionId,
        op: F,
    ) -> Result<Option<CompletedUtterance>, SessionError>
    where
        F: FnOnce(&mut SessionManager) -> Result<Option<CompletedUtterance>, SessionError>,
    {
        let mut guard = self.active.lock().await;
        let Ok(mgr) = verify_still_active(guard.as_mut(), sid) else {
            clear_pump_flags(guard.as_mut(), sid);
            return Err(cancelled_turn_err());
        };
        if mgr.capture_cancel.is_cancelled() {
            reset_pump_state(mgr);
            return Ok(None);
        }
        op(mgr)
    }
}

fn check_pump_startable(mgr: &SessionManager) -> Result<(), SessionError> {
    require_state(mgr, SessionState::Listening, SessionState::Processing)?;
    if mgr.is_cancelled() {
        return Err(cancelled_turn_err());
    }
    if mgr.capture_pump_active {
        return Err(invalid_transition_err(mgr.state, mgr.state));
    }
    Ok(())
}

fn release_pending_ptt(
    mgr: &mut SessionManager,
) -> Result<Option<CompletedUtterance>, SessionError> {
    if !mgr.pending_ptt_release {
        return Ok(None);
    }
    mgr.pending_ptt_release = false;
    mgr.speech_intake.set_ptt_held(false)
}

fn build_pump_session(mgr: &SessionManager, rx: mpsc::Receiver<AudioChunk>) -> PumpSession {
    PumpSession {
        rx,
        cancel: mgr.cancellation.clone(),
        capture_cancel: mgr.capture_cancel.clone(),
        ptt_notify: mgr.ptt_notify.clone(),
    }
}

fn reset_pump_state(mgr: &mut SessionManager) {
    mgr.capture_pump_active = false;
    mgr.pending_ptt_release = false;
    mgr.speech_intake.reset();
}

fn clear_pump_flags(mgr: Option<&mut SessionManager>, sid: &SessionId) {
    if let Some(m) = mgr.filter(|m| m.session_id() == sid) {
        reset_pump_state(m);
    }
}
