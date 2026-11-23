//! Session identity, state, correction mode, voice settings, and provider health value types.

use crate::domain::DomainValidationError;
use serde::{Deserialize, Serialize};

/// Unique identifier for a speaking session (Value Object with private inner storage).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SessionId(String);

impl SessionId {
    pub fn new(id: impl Into<String>) -> Result<Self, DomainValidationError> {
        let trimmed = id.into().trim().to_string();
        if trimmed.is_empty() {
            return Err(DomainValidationError::EmptySessionId);
        }
        Ok(Self(trimmed))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Explicit states of a voice-first speaking session (`plan.md` Section 4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SessionState {
    Idle,
    Initializing,
    Listening,
    Processing,
    Thinking,
    Speaking,
    Paused,
    Error,
    Ended,
}

impl SessionState {
    /// Returns true if and only if the microphone is actively capturing user speech.
    pub fn is_microphone_capturing(self) -> bool {
        matches!(self, Self::Listening)
    }

    /// Returns true when the session is active (neither `Idle` nor `Ended`).
    pub fn is_active(self) -> bool {
        !matches!(self, Self::Idle | Self::Ended)
    }

    /// Validates whether transitioning from `self` to `target` is permitted (`T008`).
    pub fn can_transition_to(self, target: Self) -> bool {
        matches!(
            (self, target),
            (Self::Idle, Self::Initializing)
                | (
                    Self::Initializing,
                    Self::Listening | Self::Error | Self::Ended
                )
                | (
                    Self::Listening,
                    Self::Processing | Self::Paused | Self::Error | Self::Ended
                )
                | (
                    Self::Processing,
                    Self::Listening | Self::Thinking | Self::Paused | Self::Error | Self::Ended
                )
                | (
                    Self::Thinking,
                    Self::Speaking | Self::Listening | Self::Paused | Self::Error | Self::Ended
                )
                | (
                    Self::Speaking,
                    Self::Listening | Self::Paused | Self::Error | Self::Ended
                )
                | (Self::Paused, Self::Listening | Self::Ended | Self::Error)
                | (
                    Self::Error,
                    Self::Initializing | Self::Listening | Self::Ended | Self::Idle
                )
                | (Self::Ended, Self::Idle | Self::Initializing)
        )
    }
}

/// Tutor correction intensity mode (`plan.md` Section 4, `FR-015`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CorrectionMode {
    Natural,
    #[default]
    Gentle,
    Detailed,
    Off,
}

/// Health status of a local or optional provider (`plan.md` Section 4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderHealth {
    Available,
    Unavailable,
    Misconfigured,
}
