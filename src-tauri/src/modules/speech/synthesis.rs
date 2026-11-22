//! Text-to-speech playback lifecycle events and graceful text-only degradation (`T042`, `T043`, `T045`).

use crate::domain::{AudioChunk, SessionError, SessionErrorKind, SessionId, TutorReply};
use crate::modules::conversation::cancellation::SessionCancellationToken;
use crate::modules::speech::playback::{play_with_cancellation, AudioOutput};
use crate::tts::provider::{
    synthesize_stream_with_cancellation, SpeakingSpeed, TextToSpeech, TtsAudioStream,
};
use serde::{Deserialize, Serialize};

/// Lifecycle stages emitted while synthesizing and playing a tutor response (`T043`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlaybackLifecycleStage {
    Synthesizing,
    PlaybackStarted,
    PlaybackCompleted,
    DegradedTextOnly,
}

/// Outcome of attempting to synthesize and play a tutor response (`T043`, `T045`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TutorPlaybackOutcome {
    PlayedAudibly { duration_ms: u32 },
    TextOnlyDegraded { warning: SessionError },
}

/// Event emitted when tutor speech playback finishes or degrades gracefully to text-only (`T043`, `T045`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TutorPlaybackEvent {
    pub session_id: SessionId,
    pub reply: TutorReply,
    pub stages: Vec<PlaybackLifecycleStage>,
    pub outcome: TutorPlaybackOutcome,
}

/// Synthesizes and streams playback of a tutor reply with cancellation and graceful fallback.
pub async fn synthesize_and_play_tutor_reply(
    tts: &dyn TextToSpeech,
    audio_output: &dyn AudioOutput,
    session_id: &SessionId,
    reply: &TutorReply,
    speed: SpeakingSpeed,
    cancellation: &mut SessionCancellationToken,
) -> Result<TutorPlaybackEvent, SessionError> {
    let mut stages = vec![PlaybackLifecycleStage::Synthesizing];
    match synthesize_and_play(
        tts,
        audio_output,
        &reply.spoken_reply,
        speed,
        &mut stages,
        cancellation,
    )
    .await
    {
        Ok(duration_ms) => Ok(build_audible_event(session_id, reply, stages, duration_ms)),
        Err(err) if err.kind == SessionErrorKind::Cancelled => {
            let _ = audio_output.stop_playback().await;
            Err(err)
        }
        Err(warning) => {
            Ok(degrade_to_text_only(audio_output, session_id, reply, stages, warning).await)
        }
    }
}

async fn synthesize_and_play(
    tts: &dyn TextToSpeech,
    audio_output: &dyn AudioOutput,
    spoken_text: &str,
    speed: SpeakingSpeed,
    stages: &mut Vec<PlaybackLifecycleStage>,
    cancellation: &mut SessionCancellationToken,
) -> Result<u32, SessionError> {
    let mut rx = synthesize_stream_with_cancellation(tts, spoken_text, speed, cancellation).await?;
    let total_ms = drain_audio_stream(&mut rx, audio_output, stages, cancellation).await?;
    let started = stages.contains(&PlaybackLifecycleStage::PlaybackStarted);
    finish_stream_playback(stages, started, total_ms)
}

pub(crate) async fn drain_audio_stream(
    rx: &mut TtsAudioStream,
    audio_output: &dyn AudioOutput,
    stages: &mut Vec<PlaybackLifecycleStage>,
    cancellation: &mut SessionCancellationToken,
) -> Result<u32, SessionError> {
    let mut total_ms = 0_u32;
    let mut started = stages.contains(&PlaybackLifecycleStage::PlaybackStarted);
    while let Some(chunk) = recv_chunk_or_cancel(rx, cancellation).await? {
        mark_playback_started(stages, &mut started);
        play_with_cancellation(audio_output, &chunk, cancellation).await?;
        total_ms = total_ms.saturating_add(chunk.duration_ms);
    }
    Ok(total_ms)
}

pub(crate) async fn recv_chunk_or_cancel(
    rx: &mut TtsAudioStream,
    cancellation: &mut SessionCancellationToken,
) -> Result<Option<AudioChunk>, SessionError> {
    if cancellation.is_cancelled() {
        return Err(cancelled_playback_err());
    }
    tokio::select! {
        biased;
        _ = cancellation.wait_for_cancellation() => Err(cancelled_playback_err()),
        msg = rx.recv() => unwrap_stream_item(msg),
    }
}

fn unwrap_stream_item(
    msg: Option<Result<AudioChunk, SessionError>>,
) -> Result<Option<AudioChunk>, SessionError> {
    match msg {
        Some(Ok(chunk)) => Ok(Some(chunk)),
        Some(Err(err)) => Err(err),
        None => Ok(None),
    }
}

pub(crate) fn mark_playback_started(stages: &mut Vec<PlaybackLifecycleStage>, started: &mut bool) {
    if !*started {
        stages.push(PlaybackLifecycleStage::PlaybackStarted);
        *started = true;
    }
}

pub(crate) fn finish_stream_playback(
    stages: &mut Vec<PlaybackLifecycleStage>,
    started: bool,
    total_ms: u32,
) -> Result<u32, SessionError> {
    if !started {
        return Err(SessionError::new(
            SessionErrorKind::TtsFailed,
            "Local voice synthesis failed.",
            true,
            Some("TTS stream closed with zero audio chunks.".to_string()),
        ));
    }
    stages.push(PlaybackLifecycleStage::PlaybackCompleted);
    Ok(total_ms)
}

pub(crate) fn cancelled_playback_err() -> SessionError {
    SessionError::new(
        SessionErrorKind::Cancelled,
        "Audio playback was cancelled.",
        true,
        None,
    )
}

async fn degrade_to_text_only(
    audio_output: &dyn AudioOutput,
    session_id: &SessionId,
    reply: &TutorReply,
    mut stages: Vec<PlaybackLifecycleStage>,
    warning: SessionError,
) -> TutorPlaybackEvent {
    let _ = audio_output.stop_playback().await;
    stages.push(PlaybackLifecycleStage::DegradedTextOnly);
    TutorPlaybackEvent {
        session_id: session_id.clone(),
        reply: reply.clone(),
        stages,
        outcome: TutorPlaybackOutcome::TextOnlyDegraded { warning },
    }
}

fn build_audible_event(
    session_id: &SessionId,
    reply: &TutorReply,
    stages: Vec<PlaybackLifecycleStage>,
    duration_ms: u32,
) -> TutorPlaybackEvent {
    TutorPlaybackEvent {
        session_id: session_id.clone(),
        reply: reply.clone(),
        stages,
        outcome: TutorPlaybackOutcome::PlayedAudibly { duration_ms },
    }
}
