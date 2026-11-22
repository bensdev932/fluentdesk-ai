//! Bounded utterance value objects and internal 16 kHz mono buffer (`T015`, `T018`, `T026`).

use crate::domain::{AudioChunk, SessionError, SessionErrorKind};
use crate::modules::speech::conversion::normalize_chunk_to_16khz_mono;
use std::fmt;

/// Default maximum duration of a single spoken turn (30 seconds).
pub const DEFAULT_MAX_UTTERANCE_MS: u32 = 30_000;

/// Maximum samples at 16 kHz mono for the default 30-second turn cap.
pub const MAX_UTTERANCE_SAMPLES_16KHZ: usize = 16_000 * 30;

/// Value object representing maximum allowed utterance duration in milliseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MaxUtteranceDurationMs(u32);

impl MaxUtteranceDurationMs {
    pub fn try_new(ms: u32) -> Result<Self, SessionError> {
        if !(500..=120_000).contains(&ms) {
            return Err(SessionError::new(
                SessionErrorKind::InvalidStateTransition,
                "Maximum utterance duration must be between 0.5 and 120 seconds.",
                true,
                None,
            ));
        }
        Ok(Self(ms))
    }

    pub fn default_cap() -> Self {
        Self(DEFAULT_MAX_UTTERANCE_MS)
    }

    pub fn as_u32(self) -> u32 {
        self.0
    }

    pub fn max_samples_at_16khz(self) -> usize {
        (self.0 as usize) * 16
    }
}

/// Value object representing a completed, normalized 16 kHz mono learner utterance (`T015`, `T026`).
#[derive(Clone, PartialEq)]
pub struct CompletedUtterance {
    samples_16khz_mono: Vec<f32>,
    duration_ms: u32,
    truncated_by_max_duration: bool,
}

impl CompletedUtterance {
    pub fn try_new(
        samples_16khz_mono: Vec<f32>,
        duration_ms: u32,
        truncated_by_max_duration: bool,
    ) -> Result<Self, SessionError> {
        if samples_16khz_mono.is_empty() || duration_ms == 0 {
            return Err(empty_utterance_error());
        }
        Ok(Self {
            samples_16khz_mono,
            duration_ms,
            truncated_by_max_duration,
        })
    }

    pub fn samples_16khz_mono(&self) -> &[f32] {
        &self.samples_16khz_mono
    }

    pub fn sample_count(&self) -> usize {
        self.samples_16khz_mono.len()
    }

    pub fn duration_ms(&self) -> u32 {
        self.duration_ms
    }

    pub fn is_truncated(&self) -> bool {
        self.truncated_by_max_duration
    }
}

impl fmt::Debug for CompletedUtterance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CompletedUtterance")
            .field("sample_count", &self.samples_16khz_mono.len())
            .field("samples_16khz_mono", &"<REDACTED_RAW_AUDIO>")
            .field("duration_ms", &self.duration_ms)
            .field("truncated_by_max_duration", &self.truncated_by_max_duration)
            .finish()
    }
}

fn empty_utterance_error() -> SessionError {
    SessionError::new(
        SessionErrorKind::SttFailed,
        "Cannot process an empty spoken utterance.",
        true,
        None,
    )
}

/// Bounded internal buffer accumulating normalized 16 kHz mono PCM samples for one utterance.
#[derive(Debug, Clone)]
pub(crate) struct BoundedUtteranceBuffer {
    samples: Vec<f32>,
    duration_ms: u32,
    max_samples: usize,
    truncated: bool,
}

impl BoundedUtteranceBuffer {
    pub(crate) fn new(max_samples: usize) -> Self {
        Self {
            samples: Vec::new(),
            duration_ms: 0,
            max_samples: max_samples.max(1),
            truncated: false,
        }
    }

    pub(crate) fn push_chunk(&mut self, chunk: &AudioChunk) -> Result<bool, SessionError> {
        let normalized = normalize_chunk_to_16khz_mono(chunk)?;
        Ok(self.push_normalized_samples(&normalized, chunk.duration_ms))
    }

    pub(crate) fn push_normalized_samples(&mut self, normalized: &[f32], duration_ms: u32) -> bool {
        let remaining = self.max_samples.saturating_sub(self.samples.len());
        let to_take = normalized.len().min(remaining);
        self.samples.extend_from_slice(&normalized[..to_take]);
        self.duration_ms = self.duration_ms.saturating_add(duration_ms);
        self.truncated = self.truncated || to_take < normalized.len();
        self.is_full()
    }

    pub(crate) fn trim_trailing_silence(&mut self, silence_ms: u32, keep_ms: u32) {
        if silence_ms <= keep_ms {
            return;
        }
        let excess_ms = silence_ms - keep_ms;
        let remove_samples = ((excess_ms as usize) * 16).min(self.samples.len());
        self.samples.truncate(self.samples.len() - remove_samples);
        let removed_ms = (remove_samples / 16) as u32;
        self.duration_ms = self.duration_ms.saturating_sub(removed_ms);
    }

    pub(crate) fn duration_ms(&self) -> u32 {
        self.duration_ms
    }

    pub(crate) fn is_full(&self) -> bool {
        self.samples.len() >= self.max_samples
    }

    pub(crate) fn take_utterance(&mut self) -> Option<CompletedUtterance> {
        let samples = std::mem::take(&mut self.samples);
        let duration_ms = self.duration_ms;
        let truncated = self.truncated;
        self.clear();
        CompletedUtterance::try_new(samples, duration_ms, truncated).ok()
    }

    pub(crate) fn clear(&mut self) {
        self.samples.clear();
        self.duration_ms = 0;
        self.truncated = false;
    }
}
