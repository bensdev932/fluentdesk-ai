//! Audio format and bounded PCM audio chunk value objects.

use crate::domain::{DomainValidationError, LearnerPreferences, SessionError, SessionErrorKind};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Maximum number of PCM samples permitted in a single capture `AudioChunk` (~5 seconds at 16 kHz mono).
pub const MAX_AUDIO_CHUNK_SAMPLES: usize = 16_000 * 5;

/// Maximum number of PCM samples permitted in synthesized TTS playback audio (60 seconds up to 48 kHz).
pub const MAX_SYNTHESIZED_AUDIO_SAMPLES: usize = 48_000 * 60;

/// Supported PCM sample representations at module boundaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SampleRepresentation {
    PcmI16,
    PcmF32,
}

/// Validated audio stream format (`plan.md` Section 4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioFormat {
    pub sample_rate_hz: u32,
    pub channels: u8,
    pub sample_format: SampleRepresentation,
}

impl AudioFormat {
    /// Standard Whisper-compatible 16 kHz mono PCM format.
    pub const WHISPER_16KHZ_MONO_F32: Self = Self {
        sample_rate_hz: 16_000,
        channels: 1,
        sample_format: SampleRepresentation::PcmF32,
    };

    pub fn validate(&self) -> Result<(), DomainValidationError> {
        let valid_rate = (8_000..=96_000).contains(&self.sample_rate_hz);
        let valid_channels = (1..=2).contains(&self.channels);
        if !valid_rate || !valid_channels {
            return Err(DomainValidationError::InvalidAudioFormat {
                sample_rate_hz: self.sample_rate_hz,
                channels: self.channels,
            });
        }
        Ok(())
    }
}

/// Bounded PCM audio chunk plus timing metadata (`plan.md` Section 4).
///
/// Custom `Debug` implementation ensures raw PCM samples are NEVER written to logs (`T004`).
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioChunk {
    pub format: AudioFormat,
    pub samples_f32: Vec<f32>,
    pub start_timestamp_ms: u64,
    pub duration_ms: u32,
}

impl AudioChunk {
    pub fn new_bounded(
        format: AudioFormat,
        samples_f32: Vec<f32>,
        start_timestamp_ms: u64,
        duration_ms: u32,
    ) -> Result<Self, DomainValidationError> {
        Self::with_sample_cap(
            format,
            samples_f32,
            start_timestamp_ms,
            duration_ms,
            MAX_AUDIO_CHUNK_SAMPLES,
        )
    }

    pub fn new_synthesized(
        format: AudioFormat,
        samples_f32: Vec<f32>,
        start_timestamp_ms: u64,
        duration_ms: u32,
    ) -> Result<Self, DomainValidationError> {
        Self::with_sample_cap(
            format,
            samples_f32,
            start_timestamp_ms,
            duration_ms,
            MAX_SYNTHESIZED_AUDIO_SAMPLES,
        )
    }

    fn with_sample_cap(
        format: AudioFormat,
        samples_f32: Vec<f32>,
        start_timestamp_ms: u64,
        duration_ms: u32,
        max_samples: usize,
    ) -> Result<Self, DomainValidationError> {
        format.validate()?;
        if samples_f32.len() > max_samples {
            return Err(DomainValidationError::AudioChunkExceedsBound {
                sample_count: samples_f32.len(),
                max_samples,
            });
        }
        Ok(Self {
            format,
            samples_f32,
            start_timestamp_ms,
            duration_ms,
        })
    }
}

impl fmt::Debug for AudioChunk {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AudioChunk")
            .field("format", &self.format)
            .field("sample_count", &self.samples_f32.len())
            .field("samples_f32", &"<REDACTED_RAW_AUDIO>")
            .field("start_timestamp_ms", &self.start_timestamp_ms)
            .field("duration_ms", &self.duration_ms)
            .finish()
    }
}

/// Guard enforcing raw audio non-persistence unless explicit consent is granted (`T054`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RawAudioRetentionPolicy(bool);

impl RawAudioRetentionPolicy {
    pub fn from_preferences(prefs: &LearnerPreferences) -> Self {
        Self(prefs.raw_audio_recording_opt_in)
    }

    pub fn from_opt_in(raw_audio_recording_opt_in: bool) -> Self {
        Self(raw_audio_recording_opt_in)
    }

    pub fn is_raw_audio_enabled(self) -> bool {
        self.0
    }

    pub fn require_explicit_recording_consent(self) -> Result<(), SessionError> {
        if !self.0 {
            return Err(SessionError::new(
                SessionErrorKind::PersistenceFailed,
                "Raw audio recording is disabled unless explicitly enabled by the learner.",
                true,
                None,
            ));
        }
        Ok(())
    }
}
