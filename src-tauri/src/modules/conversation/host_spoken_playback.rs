//! TTS consumer spawning, join handling, and post-playback turn reconciliation for spoken turns.

use crate::ai::router_types::RoutedTutorReply;
use crate::domain::{CorrectionMode, SessionError, SessionErrorKind, SessionId, SessionState};
use crate::modules::conversation::{
    cancellation::SessionCancellationToken,
    host::{verify_active_mut, SpeakingSessionHost},
    host_spoken_stream::{join_tts_task, played_duration_ms, AbortOnDropTts, TtsSpawnArgs},
    host_turns::{
        elapsed_ms, fail_if_err, verify_still_active, verify_turn_active, PreparedThinking,
    },
    turn::{EndToEndTurnResult, TutorTextReadyEvent},
};
use crate::modules::speech::{
    synthesis_stream::PlaybackProgress, transcription::UserTranscriptReadyEvent,
};
use std::sync::Mutex;
use std::time::Instant;

impl SpeakingSessionHost {
    pub(super) async fn tts_handles(&self, sid: &SessionId) -> Result<TtsSpawnArgs, SessionError> {
        let mut guard = self.active.lock().await;
        let mgr = verify_still_active(guard.as_mut(), sid)?;
        Ok((
            mgr.adapters.tts.clone(),
            mgr.adapters.audio_output.clone(),
            mgr.speaking_speed,
        ))
    }

    pub(super) async fn unwrap_or_fail_turn<T>(
        &self,
        sid: &SessionId,
        res: Result<T, SessionError>,
        cancel: &SessionCancellationToken,
    ) -> Result<T, SessionError> {
        let mut guard = self.active.lock().await;
        let mgr = verify_turn_active(guard.as_mut(), sid, cancel)?;
        fail_if_err(mgr, res)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) async fn complete_spoken_playback(
        &self,
        sid: &SessionId,
        tx_ev: UserTranscriptReadyEvent,
        prep: PreparedThinking,
        started: Instant,
        routed: RoutedTutorReply,
        tts_guard: AbortOnDropTts,
        progress: &Mutex<PlaybackProgress>,
        now_ms: i64,
    ) -> Result<EndToEndTurnResult, SessionError> {
        match join_tts_task(tts_guard, sid, &routed.reply).await {
            Ok(pb) => {
                let tts_ms = played_duration_ms(&pb.outcome);
                let ev = self
                    .record_completed_tutor_turn(
                        sid,
                        prep.user_turn,
                        prep.mode,
                        now_ms,
                        started,
                        routed,
                        tts_ms,
                        &prep.cancel,
                    )
                    .await?;
                self.finish_spoken_stage(sid, tx_ev, ev, Ok(pb), now_ms + 1, &prep.cancel)
                    .await
            }
            Err(err) => {
                self.fail_playback(
                    sid,
                    prep.mode,
                    now_ms,
                    started,
                    progress,
                    routed.reply.correction,
                    &prep.cancel,
                    err,
                )
                .await
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn fail_playback(
        &self,
        sid: &SessionId,
        mode: CorrectionMode,
        now_ms: i64,
        started: Instant,
        progress: &Mutex<PlaybackProgress>,
        correction: Option<String>,
        cancel: &SessionCancellationToken,
        err: SessionError,
    ) -> Result<EndToEndTurnResult, SessionError> {
        self.record_partial_playback_if_cancelled(
            sid, mode, now_ms, started, progress, correction, &err,
        )
        .await;
        let mut guard = self.active.lock().await;
        let mgr = verify_turn_active(guard.as_mut(), sid, cancel)?;
        fail_if_err(mgr, Err(err))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) async fn record_completed_tutor_turn(
        &self,
        sid: &SessionId,
        user_turn: crate::domain::ConversationTurn,
        mode: CorrectionMode,
        now_ms: i64,
        started: Instant,
        routed: RoutedTutorReply,
        tts_ms: Option<u32>,
        cancel: &SessionCancellationToken,
    ) -> Result<TutorTextReadyEvent, SessionError> {
        let mut guard = self.active.lock().await;
        let mgr = verify_turn_active(guard.as_mut(), sid, cancel)?;
        let res = mgr
            .record_tutor_turn(&routed.reply, now_ms + 1, elapsed_ms(started), mode)
            .await;
        let mut tutor_turn = fail_if_err(mgr, res)?;
        if let Some(ms) = tts_ms {
            let dur = mgr.record_tutor_tts_duration(ms).await;
            fail_if_err(mgr, dur)?;
            tutor_turn
                .stage_metadata
                .get_or_insert_with(Default::default)
                .tts_duration_ms = Some(ms);
        }
        Ok(TutorTextReadyEvent {
            session_id: sid.clone(),
            user_turn,
            tutor_turn,
            reply: routed.reply,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) async fn record_partial_playback_if_cancelled(
        &self,
        sid: &SessionId,
        mode: CorrectionMode,
        now_ms: i64,
        started: Instant,
        progress: &Mutex<PlaybackProgress>,
        correction: Option<String>,
        err: &SessionError,
    ) {
        if err.kind != SessionErrorKind::Cancelled {
            return;
        }
        let snap = progress.lock().expect("lock playback progress").clone();
        let heard = snap.started_sentences.join(" ");
        if heard.trim().is_empty() {
            return;
        }
        let mut guard = self.active.lock().await;
        if let Ok(mgr) = verify_active_mut(guard.as_mut(), sid) {
            if mgr.state != SessionState::Ended {
                let _ = mgr
                    .record_interrupted_tutor_turn(
                        &heard,
                        correction,
                        now_ms + 1,
                        elapsed_ms(started),
                        snap.played_ms,
                        mode,
                    )
                    .await;
            }
        }
    }
}
