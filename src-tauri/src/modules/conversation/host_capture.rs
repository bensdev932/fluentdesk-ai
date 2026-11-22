//! Live audio capture pump, device selection, and push-to-talk on `SpeakingSessionHost`.

use crate::domain::{AudioChunk, SessionError, SessionId, SessionState};
use crate::modules::conversation::{
    host::{verify_active_mut, SpeakingSessionHost},
    host_capture_state::PumpSession,
    host_turns::cancel_mgr_turn,
    turn::EndToEndTurnResult,
    types::SessionStatusSnapshot,
};
use crate::modules::speech::{
    audio_capture::{validate_audio_input_device, AudioDeviceId, AudioInputDeviceInfo},
    intake::CompletedUtterance,
};
use tokio::sync::mpsc;

impl SpeakingSessionHost {
    pub async fn list_input_devices(&self) -> Result<Vec<AudioInputDeviceInfo>, SessionError> {
        self.adapters.audio_input.list_input_devices().await
    }

    pub async fn set_input_device(
        &self,
        session_id: &SessionId,
        raw_device_id: &str,
    ) -> Result<AudioDeviceId, SessionError> {
        let validated =
            validate_audio_input_device(self.adapters.audio_input.as_ref(), raw_device_id).await?;
        let mut guard = self.active.lock().await;
        let mgr = verify_active_mut(guard.as_mut(), session_id)?;
        mgr.set_active_input_device(validated.clone()).await?;
        Ok(validated)
    }

    pub async fn set_ptt_held(
        &self,
        session_id: &SessionId,
        held: bool,
    ) -> Result<Option<CompletedUtterance>, SessionError> {
        let mut guard = self.active.lock().await;
        verify_active_mut(guard.as_mut(), session_id)?.set_ptt_held(held)
    }

    pub async fn ingest_audio_chunk(
        &self,
        session_id: &SessionId,
        chunk: &AudioChunk,
    ) -> Result<Option<CompletedUtterance>, SessionError> {
        let mut guard = self.active.lock().await;
        let mgr = verify_active_mut(guard.as_mut(), session_id)?;
        if mgr.state != SessionState::Listening {
            return Ok(None);
        }
        mgr.handle_pumped_chunk(chunk)
    }

    pub async fn barge_in_session(
        &self,
        session_id: &SessionId,
    ) -> Result<SessionStatusSnapshot, SessionError> {
        self.barge_in_with_progress(session_id, None, None).await
    }

    pub async fn barge_in_with_progress(
        &self,
        session_id: &SessionId,
        heard_sentences: Option<&[String]>,
        played_ms: Option<u32>,
    ) -> Result<SessionStatusSnapshot, SessionError> {
        let mut guard = self.active.lock().await;
        let mgr = verify_active_mut(guard.as_mut(), session_id)?;
        let _ = cancel_mgr_turn(mgr).await;
        reconcile_barge_in_mgr(mgr, heard_sentences, played_ms).await?;
        Ok(mgr.status_snapshot())
    }

    pub async fn pump_capture_turn(
        &self,
        session_id: &SessionId,
        now_unix_ms: i64,
    ) -> Result<Option<EndToEndTurnResult>, SessionError> {
        let Some(utt) = self.capture_next_utterance(session_id).await? else {
            return Ok(None);
        };
        self.process_spoken_utterance(session_id, &utt, now_unix_ms)
            .await
            .map(Some)
    }

    async fn capture_next_utterance(
        &self,
        sid: &SessionId,
    ) -> Result<Option<CompletedUtterance>, SessionError> {
        let Some(pump) = self.begin_capture_pump(sid).await? else {
            return Ok(None);
        };
        self.run_capture_pump_loop(sid, pump).await
    }

    async fn run_capture_pump_loop(
        &self,
        sid: &SessionId,
        mut p: PumpSession,
    ) -> Result<Option<CompletedUtterance>, SessionError> {
        loop {
            if let Some(early) = self.check_cancel_flags(sid, &p).await {
                return early;
            }
            tokio::select! {
                biased;
                _ = p.cancel.wait_for_cancellation() => return self.abort_pump(sid, true).await,
                _ = p.capture_cancel.wait_for_cancellation() => return self.abort_pump(sid, false).await,
                _ = p.ptt_notify.notified() => {
                    if let Some(utt) = self.drain_and_release_ptt(sid, &mut p.rx).await? {
                        return self.complete_pump(sid, p.rx, utt).await;
                    }
                }
                maybe_chunk = p.rx.recv() => match maybe_chunk {
                    Some(chunk) => if let Some(utt) = self.feed_pumped_chunk(sid, &chunk).await? {
                        return self.complete_pump(sid, p.rx, utt).await;
                    },
                    None => return self.abort_pump(sid, false).await,
                }
            }
        }
    }

    async fn drain_and_release_ptt(
        &self,
        sid: &SessionId,
        rx: &mut mpsc::Receiver<AudioChunk>,
    ) -> Result<Option<CompletedUtterance>, SessionError> {
        while let Ok(chunk) = rx.try_recv() {
            if let Some(utt) = self.feed_pumped_chunk(sid, &chunk).await? {
                return Ok(Some(utt));
            }
        }
        self.release_pending_ptt_on_host(sid).await
    }
}

async fn reconcile_barge_in_mgr(
    mgr: &mut crate::modules::conversation::manager::SessionManager,
    heard_sentences: Option<&[String]>,
    played_ms: Option<u32>,
) -> Result<(), SessionError> {
    if heard_sentences.is_none() && played_ms.is_none() {
        return Ok(());
    }
    let joined = heard_sentences.map(join_clean_sentences);
    mgr.reconcile_tutor_turn_playback(joined.as_deref(), played_ms)
        .await
}

fn join_clean_sentences(sentences: &[String]) -> String {
    sentences
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}
