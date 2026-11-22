//! Streaming AI-to-TTS coordination helpers for `SpeakingSessionHost` spoken turns.

use crate::ai::provider::{recv_tutor_stream_item, TutorReplyStream, TutorStreamItem};
use crate::ai::router_types::RoutedTutorReply;
use crate::domain::{SessionError, SessionErrorKind, SessionId, SessionState, TutorReply};
use crate::modules::conversation::{
    cancellation::SessionCancellationToken,
    host::SpeakingSessionHost,
    host_spoken_events::{emit_spoken_ev, emit_state_ev, SpokenTurnEvent},
    host_turns::verify_still_active,
};
use crate::modules::speech::{
    playback::AudioOutput,
    synthesis::{TutorPlaybackEvent, TutorPlaybackOutcome},
    synthesis_stream::{
        synthesize_and_play_sentence_channel_with_progress, PlaybackProgress,
        StreamedPlaybackSummary,
    },
};
use crate::tts::provider::{SpeakingSpeed, TextToSpeech};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

pub(super) type TtsSpawnArgs = (Arc<dyn TextToSpeech>, Arc<dyn AudioOutput>, SpeakingSpeed);
pub(super) type TtsJoinHandle =
    tokio::task::JoinHandle<Result<StreamedPlaybackSummary, SessionError>>;

pub(super) struct AbortOnDropTts(Option<TtsJoinHandle>);

impl AbortOnDropTts {
    pub(super) fn new(handle: TtsJoinHandle) -> Self {
        Self(Some(handle))
    }

    pub(super) fn into_inner(mut self) -> TtsJoinHandle {
        self.0.take().expect("tts handle present")
    }
}

impl Drop for AbortOnDropTts {
    fn drop(&mut self) {
        if let Some(h) = self.0.take() {
            h.abort();
        }
    }
}

impl SpeakingSessionHost {
    pub(super) async fn pump_ai_to_tts(
        &self,
        sid: &SessionId,
        mut ai_stream: TutorReplyStream,
        sent_tx: mpsc::Sender<String>,
        ev_tx: Option<&mpsc::Sender<SpokenTurnEvent>>,
        cancel: &mut SessionCancellationToken,
    ) -> Result<RoutedTutorReply, SessionError> {
        let mut emitted_any = false;
        while let Some(item) = recv_tutor_stream_item(&mut ai_stream, cancel).await? {
            if let Some(routed) = self
                .dispatch_stream_item(sid, item, &sent_tx, ev_tx, &mut emitted_any)
                .await?
            {
                return Ok(routed);
            }
        }
        Err(incomplete_stream_err())
    }

    async fn dispatch_stream_item(
        &self,
        sid: &SessionId,
        item: TutorStreamItem,
        sent_tx: &mpsc::Sender<String>,
        ev_tx: Option<&mpsc::Sender<SpokenTurnEvent>>,
        emitted_any: &mut bool,
    ) -> Result<Option<RoutedTutorReply>, SessionError> {
        match item {
            TutorStreamItem::Sentence(s) => {
                self.emit_tts_sentence(sid, s, sent_tx, ev_tx, emitted_any)
                    .await?;
                Ok(None)
            }
            TutorStreamItem::Final(routed) => {
                if !*emitted_any {
                    let s = routed.reply.spoken_reply.clone();
                    self.emit_tts_sentence(sid, s, sent_tx, ev_tx, emitted_any)
                        .await?;
                }
                Ok(Some(routed))
            }
        }
    }

    async fn emit_tts_sentence(
        &self,
        sid: &SessionId,
        text: String,
        sent_tx: &mpsc::Sender<String>,
        ev_tx: Option<&mpsc::Sender<SpokenTurnEvent>>,
        emitted_any: &mut bool,
    ) -> Result<(), SessionError> {
        self.mark_speaking_if_first(sid, emitted_any, ev_tx).await?;
        emit_spoken_ev(ev_tx, SpokenTurnEvent::Sentence(text.clone())).await;
        let _ = sent_tx.send(text).await;
        Ok(())
    }

    async fn mark_speaking_if_first(
        &self,
        sid: &SessionId,
        emitted_any: &mut bool,
        ev_tx: Option<&mpsc::Sender<SpokenTurnEvent>>,
    ) -> Result<(), SessionError> {
        if *emitted_any {
            return Ok(());
        }
        *emitted_any = true;
        let mut guard = self.active.lock().await;
        let mgr = verify_still_active(guard.as_mut(), sid)?;
        if mgr.state == SessionState::Thinking {
            mgr.transition_to(SessionState::Speaking)?;
            emit_state_ev(ev_tx, SessionState::Speaking).await;
        }
        Ok(())
    }
}

fn incomplete_stream_err() -> SessionError {
    SessionError::new(
        SessionErrorKind::AiInferenceFailed,
        "Tutor response stream ended unexpectedly.",
        true,
        None,
    )
}

pub(super) fn spawn_tts_consumer(
    handles: &TtsSpawnArgs,
    sent_rx: mpsc::Receiver<String>,
    cancel: SessionCancellationToken,
    progress: Option<Arc<Mutex<PlaybackProgress>>>,
) -> AbortOnDropTts {
    let h = tokio::spawn(synthesize_and_play_sentence_channel_with_progress(
        handles.0.clone(),
        handles.1.clone(),
        sent_rx,
        handles.2,
        cancel,
        progress,
    ));
    AbortOnDropTts::new(h)
}

pub(super) async fn join_tts_task(
    tts_guard: AbortOnDropTts,
    sid: &SessionId,
    reply: &TutorReply,
) -> Result<TutorPlaybackEvent, SessionError> {
    let summary = tts_guard
        .into_inner()
        .await
        .map_err(|_| incomplete_playback_err())?;
    Ok(summary?.into_event(sid, reply))
}

pub(super) fn played_duration_ms(outcome: &TutorPlaybackOutcome) -> Option<u32> {
    match outcome {
        TutorPlaybackOutcome::PlayedAudibly { duration_ms } => Some(*duration_ms),
        TutorPlaybackOutcome::TextOnlyDegraded { .. } => None,
    }
}

fn incomplete_playback_err() -> SessionError {
    SessionError::new(
        SessionErrorKind::AiInferenceFailed,
        "Tutor playback task ended unexpectedly.",
        true,
        None,
    )
}
