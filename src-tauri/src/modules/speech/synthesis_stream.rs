//! Concurrent sentence-channel TTS synthesis & playback consumer (`voice-research-upgrades` #01).

use crate::domain::{SessionError, SessionErrorKind, SessionId, TutorReply};
use crate::modules::conversation::cancellation::SessionCancellationToken;
use crate::modules::speech::{
    playback::{play_with_cancellation, AudioOutput},
    synthesis::{
        cancelled_playback_err, finish_stream_playback, mark_playback_started,
        recv_chunk_or_cancel, PlaybackLifecycleStage, TutorPlaybackEvent, TutorPlaybackOutcome,
    },
};
use crate::tts::provider::{
    synthesize_stream_with_cancellation, SpeakingSpeed, TextToSpeech, TtsAudioStream,
};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

/// Shared progress tracker updated live as streamed TTS sentences play out.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlaybackProgress {
    pub started_sentences: Vec<String>,
    pub played_ms: u32,
}

/// Result of consuming a stream of spoken sentences concurrently with LLM generation.
pub struct StreamedPlaybackSummary {
    pub stages: Vec<PlaybackLifecycleStage>,
    pub outcome: TutorPlaybackOutcome,
}

impl StreamedPlaybackSummary {
    pub fn into_event(self, session_id: &SessionId, reply: &TutorReply) -> TutorPlaybackEvent {
        TutorPlaybackEvent {
            session_id: session_id.clone(),
            reply: reply.clone(),
            stages: self.stages,
            outcome: self.outcome,
        }
    }
}

/// Receives spoken sentences one at a time and plays synthesized audio immediately.
pub async fn synthesize_and_play_sentence_channel(
    tts: Arc<dyn TextToSpeech>,
    audio_output: Arc<dyn AudioOutput>,
    sentence_rx: mpsc::Receiver<String>,
    speed: SpeakingSpeed,
    cancellation: SessionCancellationToken,
) -> Result<StreamedPlaybackSummary, SessionError> {
    synthesize_and_play_sentence_channel_with_progress(
        tts,
        audio_output,
        sentence_rx,
        speed,
        cancellation,
        None,
    )
    .await
}

/// Receives spoken sentences and records incremental heard playback progress on barge-in.
pub async fn synthesize_and_play_sentence_channel_with_progress(
    tts: Arc<dyn TextToSpeech>,
    audio_output: Arc<dyn AudioOutput>,
    mut sentence_rx: mpsc::Receiver<String>,
    speed: SpeakingSpeed,
    mut cancellation: SessionCancellationToken,
    progress: Option<Arc<Mutex<PlaybackProgress>>>,
) -> Result<StreamedPlaybackSummary, SessionError> {
    let mut stages = vec![PlaybackLifecycleStage::Synthesizing];
    let res = run_sentence_loop(
        tts.as_ref(),
        audio_output.as_ref(),
        &mut sentence_rx,
        speed,
        &mut stages,
        &mut cancellation,
        progress.as_deref(),
    )
    .await;
    finalize_stream_summary(audio_output.as_ref(), stages, res).await
}

async fn run_sentence_loop(
    tts: &dyn TextToSpeech,
    audio_output: &dyn AudioOutput,
    sentence_rx: &mut mpsc::Receiver<String>,
    speed: SpeakingSpeed,
    stages: &mut Vec<PlaybackLifecycleStage>,
    cancellation: &mut SessionCancellationToken,
    progress: Option<&Mutex<PlaybackProgress>>,
) -> Result<u32, SessionError> {
    let mut total_ms = 0_u32;
    while let Some(sentence) = recv_sentence_or_cancel(sentence_rx, cancellation).await? {
        let mut rx =
            synthesize_stream_with_cancellation(tts, &sentence, speed, cancellation).await?;
        let chunk_ms = drain_tracked_audio(
            &sentence,
            &mut rx,
            audio_output,
            stages,
            cancellation,
            progress,
        )
        .await?;
        total_ms = total_ms.saturating_add(chunk_ms);
    }
    let started = stages.contains(&PlaybackLifecycleStage::PlaybackStarted);
    finish_stream_playback(stages, started, total_ms)
}

async fn drain_tracked_audio(
    sentence: &str,
    rx: &mut TtsAudioStream,
    audio_output: &dyn AudioOutput,
    stages: &mut Vec<PlaybackLifecycleStage>,
    cancellation: &mut SessionCancellationToken,
    progress: Option<&Mutex<PlaybackProgress>>,
) -> Result<u32, SessionError> {
    let (mut total_ms, mut recorded) = (0_u32, false);
    let mut started = stages.contains(&PlaybackLifecycleStage::PlaybackStarted);
    while let Some(chunk) = recv_chunk_or_cancel(rx, cancellation).await? {
        mark_playback_started(stages, &mut started);
        play_with_cancellation(audio_output, &chunk, cancellation).await?;
        record_chunk_progress(progress, sentence, chunk.duration_ms, &mut recorded);
        total_ms = total_ms.saturating_add(chunk.duration_ms);
    }
    Ok(total_ms)
}

fn record_chunk_progress(
    progress: Option<&Mutex<PlaybackProgress>>,
    sentence: &str,
    duration_ms: u32,
    recorded: &mut bool,
) {
    let Some(tracker) = progress else {
        return;
    };
    let mut guard = tracker.lock().expect("lock playback progress");
    if !*recorded {
        guard.started_sentences.push(sentence.to_string());
        *recorded = true;
    }
    guard.played_ms = guard.played_ms.saturating_add(duration_ms);
}

async fn recv_sentence_or_cancel(
    sentence_rx: &mut mpsc::Receiver<String>,
    cancellation: &mut SessionCancellationToken,
) -> Result<Option<String>, SessionError> {
    if cancellation.is_cancelled() {
        return Err(cancelled_playback_err());
    }
    tokio::select! {
        biased;
        _ = cancellation.wait_for_cancellation() => Err(cancelled_playback_err()),
        msg = sentence_rx.recv() => Ok(msg),
    }
}

async fn finalize_stream_summary(
    audio_output: &dyn AudioOutput,
    mut stages: Vec<PlaybackLifecycleStage>,
    res: Result<u32, SessionError>,
) -> Result<StreamedPlaybackSummary, SessionError> {
    match res {
        Ok(duration_ms) => Ok(StreamedPlaybackSummary {
            stages,
            outcome: TutorPlaybackOutcome::PlayedAudibly { duration_ms },
        }),
        Err(err) if err.kind == SessionErrorKind::Cancelled => {
            let _ = audio_output.stop_playback().await;
            Err(err)
        }
        Err(warning) => {
            let _ = audio_output.stop_playback().await;
            stages.push(PlaybackLifecycleStage::DegradedTextOnly);
            Ok(StreamedPlaybackSummary {
                stages,
                outcome: TutorPlaybackOutcome::TextOnlyDegraded { warning },
            })
        }
    }
}
