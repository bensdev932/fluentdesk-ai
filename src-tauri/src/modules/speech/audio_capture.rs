//! Audio input capture boundary, device validation (`AudioDeviceId`), and `NonBlockingAudioSink`
//! (`T006`, `T014`, `T016`, `T017`).

pub use crate::domain::RawAudioRetentionPolicy;
pub use crate::modules::speech::audio_capture_mock::{LiveAudioFeeder, MockAudioInput};

use crate::domain::{AudioChunk, AudioFormat, SessionError, SessionErrorKind};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tokio::sync::mpsc;

/// Bounded channel capacity for audio chunks (~64 frames).
pub const DEFAULT_AUDIO_CHANNEL_CAPACITY: usize = 64;

/// Value object wrapping a validated audio device identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AudioDeviceId(String);

impl AudioDeviceId {
    pub fn try_new(raw: impl Into<String>) -> Result<Self, SessionError> {
        let trimmed = raw.into().trim().to_string();
        if trimmed.is_empty() {
            return Err(SessionError::new(
                SessionErrorKind::AudioDeviceUnavailable,
                "Audio input device ID must not be empty.",
                true,
                None,
            ));
        }
        Ok(Self(trimmed))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Safe metadata describing an available audio input device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioInputDeviceInfo {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

/// Validates a requested device identifier against enumerated `AudioInput` devices.
pub async fn validate_audio_input_device(
    audio_input: &dyn AudioInput,
    raw_device_id: &str,
) -> Result<AudioDeviceId, SessionError> {
    let candidate = AudioDeviceId::try_new(raw_device_id)?;
    let devices = audio_input.list_input_devices().await?;
    if !devices.iter().any(|d| d.id == candidate.as_str()) {
        return Err(missing_device_error(candidate.as_str()));
    }
    Ok(candidate)
}

pub(crate) fn missing_device_error(device_id: &str) -> SessionError {
    SessionError::new(
        SessionErrorKind::AudioDeviceUnavailable,
        "Selected microphone device was not found.",
        true,
        Some(format!(
            "Device '{device_id}' not found in enumerated devices"
        )),
    )
}

/// Non-blocking producer used inside real-time audio callbacks (`T017`).
///
/// Uses `try_send` so the OS audio thread never awaits or blocks; dropped frames
/// are counted atomically if the consumer falls behind.
#[derive(Clone)]
pub struct NonBlockingAudioSink {
    sender: mpsc::Sender<AudioChunk>,
    dropped_chunks: Arc<AtomicUsize>,
}

impl NonBlockingAudioSink {
    pub fn new_bounded(capacity: usize) -> (Self, mpsc::Receiver<AudioChunk>) {
        let (sender, receiver) = mpsc::channel(capacity.max(1));
        (
            Self {
                sender,
                dropped_chunks: Arc::new(AtomicUsize::new(0)),
            },
            receiver,
        )
    }

    pub fn push_from_callback(&self, chunk: AudioChunk) -> bool {
        if self.sender.try_send(chunk).is_ok() {
            true
        } else {
            self.dropped_chunks.fetch_add(1, Ordering::Relaxed);
            false
        }
    }

    pub fn dropped_chunk_count(&self) -> usize {
        self.dropped_chunks.load(Ordering::Relaxed)
    }
}

/// Hardware-independent audio capture contract (`T014`).
#[async_trait]
pub trait AudioInput: Send + Sync {
    async fn list_input_devices(&self) -> Result<Vec<AudioInputDeviceInfo>, SessionError>;

    async fn start_capture(
        &self,
        device_id: Option<&str>,
        format: AudioFormat,
    ) -> Result<mpsc::Receiver<AudioChunk>, SessionError>;

    async fn stop_capture(&self) -> Result<(), SessionError>;

    fn is_capturing(&self) -> bool;
}
