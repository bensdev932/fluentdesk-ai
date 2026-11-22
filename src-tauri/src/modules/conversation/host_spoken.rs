//! Non-blocking spoken utterance pipeline (`process_spoken_utterance`) on `SpeakingSessionHost`.

use crate::ai::provider::{open_turn_stream_with_cancellation, TutorReplyStream, TutorTurnRequest};
use crate::ai::router_types::RoutedTutorReply;
use crate::domain::{SessionError, SessionId, SessionState};
use crate::modules::conversation::{
    cancellation::SessionCancellationToken,
    host::SpeakingSessionHost,
    host_spoken_events::{
        abort_tts_on_err, emit_spoken_ev, emit_state_ev, empty_utterance_result,
        transcribe_with_cancel, SpokenTurnEvent,
    },
    host_spoken_stream::{spawn_tts_consumer, AbortOnDropTts},
    host_turn_guards::cancelled_turn_err,
    host_turns::PreparedThinking,
    turn::EndToEndTurnResult,
};
use crate::modules::speech::{
    playback::AudioOutput, synthesis_stream::PlaybackProgress,
    transcription::UserTranscriptReadyEvent, utterance::CompletedUtterance,
};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex,
};
use std::time::Instant;
use tokio::sync::{mpsc, oneshot};

pub(crate) struct SpokenTurnAck<'a> {
    pub started_tx: oneshot::Sender<()>,
    pub turn_gen: &'a AtomicU64,
}

impl SpeakingSessionHost {
    pub async fn process_spoken_utterance(
        &self,
        sid: &SessionId,
        utt: &CompletedUtterance,
        now_ms: i64,
    ) -> Result<EndToEndTurnResult, SessionError> {
        self.run_spoken_turn_internal(sid, utt, now_ms, None, None)
            .await
    }

    pub async fn process_spoken_utterance_with_events(
        &self,
        sid: &SessionId,
        utt: &CompletedUtterance,
        now_ms: i64,
        ev_tx: Option<mpsc::Sender<SpokenTurnEvent>>,
    ) -> Result<EndToEndTurnResult, SessionError> {
        self.run_spoken_turn_internal(sid, utt, now_ms, ev_tx, None)
            .await
    }

    pub(crate) async fn process_spoken_utterance_with_ack(
        &self,
        sid: &SessionId,
        utt: &CompletedUtterance,
        now_ms: i64,
        ev_tx: Option<mpsc::Sender<SpokenTurnEvent>>,
        ack: SpokenTurnAck<'_>,
    ) -> Result<EndToEndTurnResult, SessionError> {
        self.run_spoken_turn_internal(sid, utt, now_ms, ev_tx, Some(ack))
            .await
    }

    async fn run_spoken_turn_internal(
        &self,
        sid: &SessionId,
        utt: &CompletedUtterance,
        now_ms: i64,
        ev_tx: Option<mpsc::Sender<SpokenTurnEvent>>,
        ack: Option<SpokenTurnAck<'_>>,
    ) -> Result<EndToEndTurnResult, SessionError> {
        let (stt, prompt_ctx, mut cancel, turn_gen) = self.prepare_stt_stage(sid).await?;
        if let Some(a) = ack {
            a.turn_gen.store(turn_gen, Ordering::Release);
            let _ = a.started_tx.send(());
        }
        let tx_ref = ev_tx.as_ref();
        emit_state_ev(tx_ref, SessionState::Processing).await;
        let res = transcribe_with_cancel(stt.as_ref(), utt, &prompt_ctx, &mut cancel).await;
        let (tx_ev, cont) = self.after_stt_stage(sid, res, &cancel).await?;
        self.dispatch_after_stt(sid, tx_ev, cont, now_ms, tx_ref, &cancel)
            .await
    }

    async fn dispatch_after_stt(
        &self,
        sid: &SessionId,
        tx_ev: UserTranscriptReadyEvent,
        cont: bool,
        now_ms: i64,
        tx_ref: Option<&mpsc::Sender<SpokenTurnEvent>>,
        cancel: &SessionCancellationToken,
    ) -> Result<EndToEndTurnResult, SessionError> {
        if !cont {
            emit_state_ev(tx_ref, SessionState::Listening).await;
            return Ok(empty_utterance_result(tx_ev));
        }
        emit_spoken_ev(tx_ref, SpokenTurnEvent::UserTranscript(tx_ev.clone())).await;
        emit_state_ev(tx_ref, SessionState::Thinking).await;
        self.run_spoken_ai_and_tts(sid, tx_ev, now_ms, tx_ref, cancel)
            .await
    }

    async fn run_spoken_ai_and_tts(
        &self,
        sid: &SessionId,
        tx_ev: UserTranscriptReadyEvent,
        now_ms: i64,
        ev_tx: Option<&mpsc::Sender<SpokenTurnEvent>>,
        stt_cancel: &SessionCancellationToken,
    ) -> Result<EndToEndTurnResult, SessionError> {
        let mut prep = self
            .prepare_thinking_turn(
                sid,
                &tx_ev.transcript,
                None,
                now_ms,
                false,
                Some(stt_cancel),
            )
            .await?;
        let (handles, started) = (self.tts_handles(sid).await?, Instant::now());
        let stream = self.open_turn_stream(sid, &mut prep, &tx_ev).await?;
        let (sent_tx, sent_rx) = mpsc::channel(8);
        let progress = Arc::new(Mutex::new(PlaybackProgress::default()));
        let tts_guard = spawn_tts_consumer(
            &handles,
            sent_rx,
            prep.cancel.clone(),
            Some(progress.clone()),
        );
        let pumped = self
            .pump_or_abort_tts(
                sid,
                stream,
                sent_tx,
                ev_tx,
                &mut prep.cancel,
                &*handles.1,
                tts_guard,
            )
            .await;
        let (routed, tts_guard) = match pumped {
            Ok(pair) => pair,
            Err(err) => {
                self.record_partial_playback_if_cancelled(
                    sid, prep.mode, now_ms, started, &progress, None, &err,
                )
                .await;
                return Err(err);
            }
        };
        self.complete_spoken_playback(
            sid, tx_ev, prep, started, routed, tts_guard, &progress, now_ms,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn pump_or_abort_tts(
        &self,
        sid: &SessionId,
        stream: TutorReplyStream,
        sent_tx: mpsc::Sender<String>,
        ev_tx: Option<&mpsc::Sender<SpokenTurnEvent>>,
        cancel: &mut SessionCancellationToken,
        out: &dyn AudioOutput,
        tts_guard: AbortOnDropTts,
    ) -> Result<(RoutedTutorReply, AbortOnDropTts), SessionError> {
        let ai_res = self
            .pump_ai_to_tts(sid, stream, sent_tx, ev_tx, cancel)
            .await;
        let routed = match self.unwrap_or_fail_turn(sid, ai_res, cancel).await {
            Ok(r) => r,
            Err(err) => return abort_tts_on_err(out, tts_guard, err).await,
        };
        if cancel.is_cancelled() {
            return abort_tts_on_err(out, tts_guard, cancelled_turn_err()).await;
        }
        Ok((routed, tts_guard))
    }

    async fn open_turn_stream(
        &self,
        sid: &SessionId,
        prep: &mut PreparedThinking,
        tx_ev: &UserTranscriptReadyEvent,
    ) -> Result<TutorReplyStream, SessionError> {
        let sc = Some(&prep.scaffold);
        let req = TutorTurnRequest::new(&prep.history, &tx_ev.transcript, prep.mode, sc);
        let res = open_turn_stream_with_cancellation(&*prep.ai, &req, &mut prep.cancel).await;
        self.unwrap_or_fail_turn(sid, res, &prep.cancel).await
    }
}
