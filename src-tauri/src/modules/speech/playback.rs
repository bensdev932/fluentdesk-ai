//! Audio output playback boundary (`AudioOutput` trait and `MockAudioOutput` for `T006`).

use crate::domain::{AudioChunk, SessionError, SessionErrorKind};
use crate::modules::conversation::cancellation::SessionCancellationToken;
use async_trait::async_trait;
use std::sync::{Arc, Mutex};

/// Replaceable audio playback contract (`plan.md` Section 3, `T038`).
#[async_trait]
pub trait AudioOutput: Send + Sync {
    async fn play_audio(&self, audio: &AudioChunk) -> Result<(), SessionError>;

    async fn stop_playback(&self) -> Result<(), SessionError>;

    fn is_playing(&self) -> bool;
}

/// Plays a synthesized `AudioChunk` with cooperative cancellation and immediate teardown (`T038`, `T042`).
pub async fn play_with_cancellation(
    output: &dyn AudioOutput,
    audio: &AudioChunk,
    cancellation: &mut SessionCancellationToken,
) -> Result<(), SessionError> {
    if cancellation.is_cancelled() {
        let _ = output.stop_playback().await;
        return Err(cancelled_playback_error());
    }
    tokio::select! {
        biased;
        _ = cancellation.wait_for_cancellation() => {
            let _ = output.stop_playback().await;
            Err(cancelled_playback_error())
        }
        res = output.play_audio(audio) => res,
    }
}

fn cancelled_playback_error() -> SessionError {
    SessionError::new(
        SessionErrorKind::Cancelled,
        "Audio playback was cancelled.",
        true,
        None,
    )
}

/// Deterministic mock implementation of `AudioOutput` (`T006`).
#[derive(Clone, Default)]
pub struct MockAudioOutput {
    played_chunks: Arc<Mutex<Vec<AudioChunk>>>,
    playing: Arc<Mutex<bool>>,
    stopped_count: Arc<Mutex<usize>>,
    fail_with: Arc<Mutex<Option<SessionError>>>,
}

impl MockAudioOutput {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_failure(&self, error: Option<SessionError>) {
        *self.fail_with.lock().expect("lock playback error") = error;
    }

    pub fn played_chunks(&self) -> Vec<AudioChunk> {
        self.played_chunks
            .lock()
            .expect("lock played chunks")
            .clone()
    }

    pub fn stopped_count(&self) -> usize {
        *self.stopped_count.lock().expect("lock stopped count")
    }
}

#[async_trait]
impl AudioOutput for MockAudioOutput {
    async fn play_audio(&self, audio: &AudioChunk) -> Result<(), SessionError> {
        if let Some(err) = self.fail_with.lock().expect("lock playback error").clone() {
            return Err(err);
        }
        *self.playing.lock().expect("lock playing state") = true;
        self.played_chunks
            .lock()
            .expect("lock played chunks")
            .push(audio.clone());
        *self.playing.lock().expect("lock playing state") = false;
        Ok(())
    }

    async fn stop_playback(&self) -> Result<(), SessionError> {
        *self.playing.lock().expect("lock playing state") = false;
        *self.stopped_count.lock().expect("lock stopped count") += 1;
        Ok(())
    }

    fn is_playing(&self) -> bool {
        *self.playing.lock().expect("lock playing state")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::AudioFormat;

    #[tokio::test]
    async fn mock_audio_output_records_played_chunks_and_handles_stop() {
        let output = MockAudioOutput::new();
        let chunk = AudioChunk::new_bounded(
            AudioFormat::WHISPER_16KHZ_MONO_F32,
            vec![0.1, -0.1, 0.05],
            0,
            20,
        )
        .unwrap();

        output.play_audio(&chunk).await.expect("play audio");
        assert_eq!(output.played_chunks().len(), 1);

        output.stop_playback().await.expect("stop playback");
        assert!(!output.is_playing());
        assert_eq!(output.stopped_count(), 1);
    }
}
