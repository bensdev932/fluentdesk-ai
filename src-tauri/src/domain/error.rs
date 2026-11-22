//! Typed session errors and domain validation errors.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Typed internal error category for a speaking session (`plan.md` Section 4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionErrorKind {
    AudioDeviceUnavailable,
    MicrophonePermissionDenied,
    SttUnavailable,
    SttFailed,
    AiProviderUnavailable,
    AiInferenceFailed,
    TtsUnavailable,
    TtsFailed,
    PlaybackFailed,
    PersistenceFailed,
    InvalidStateTransition,
    Cancelled,
}

/// Typed session error with a safe user-facing description (`plan.md` Section 4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Error)]
#[serde(rename_all = "camelCase")]
#[error("{user_message} ({kind:?})")]
pub struct SessionError {
    pub kind: SessionErrorKind,
    pub user_message: String,
    pub recoverable: bool,
    #[serde(skip)]
    pub internal_detail: Option<String>,
}

impl SessionError {
    pub fn new(
        kind: SessionErrorKind,
        user_message: impl Into<String>,
        recoverable: bool,
        internal_detail: Option<String>,
    ) -> Self {
        Self {
            kind,
            user_message: user_message.into(),
            recoverable,
            internal_detail,
        }
    }
}

impl From<DomainValidationError> for SessionError {
    fn from(err: DomainValidationError) -> Self {
        Self::new(
            SessionErrorKind::InvalidStateTransition,
            "Domain validation failed.",
            true,
            Some(err.to_string()),
        )
    }
}

/// Validation failures for domain boundary objects.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DomainValidationError {
    #[error("session ID must not be empty")]
    EmptySessionId,
    #[error("invalid audio format: sample_rate_hz={sample_rate_hz}, channels={channels}")]
    InvalidAudioFormat { sample_rate_hz: u32, channels: u8 },
    #[error("audio chunk length ({sample_count}) exceeds maximum bound ({max_samples})")]
    AudioChunkExceedsBound {
        sample_count: usize,
        max_samples: usize,
    },
    #[error("transcript confidence {0} must be in 0.0..=1.0")]
    InvalidConfidence(String),
    #[error("tutor spoken reply must not be empty")]
    EmptyTutorReply,
    #[error("session max turns {0} must be in 2..=20")]
    InvalidSessionTurnBound(u32),
    #[error("session topic must not be empty")]
    EmptySessionTopic,
}
