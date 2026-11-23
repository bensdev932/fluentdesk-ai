//! Validated English voice identifier and speaking speed value objects.

use crate::domain::{sanitize_untrusted_text, SessionError, SessionErrorKind};
use serde::{Deserialize, Deserializer, Serialize};

pub const DEFAULT_ENGLISH_VOICE_ID: &str = "en-US";

/// Validated speaking speed multiplier (`0.5..=2.0`) (`T038`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(transparent)]
pub struct SpeakingSpeed(f32);

impl Default for SpeakingSpeed {
    fn default() -> Self {
        Self(1.0)
    }
}

impl SpeakingSpeed {
    pub fn try_new(speed: f32) -> Result<Self, SessionError> {
        if !(0.5..=2.0).contains(&speed) || speed.is_nan() {
            return Err(invalid_speed_error(speed));
        }
        Ok(Self(speed))
    }

    pub fn as_f32(self) -> f32 {
        self.0
    }
}

impl<'de> Deserialize<'de> for SpeakingSpeed {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = f32::deserialize(deserializer)?;
        Self::try_new(raw).map_err(|e| serde::de::Error::custom(e.user_message))
    }
}

fn invalid_speed_error(speed: f32) -> SessionError {
    SessionError::new(
        SessionErrorKind::TtsFailed,
        "Speaking speed must be between 0.5x and 2.0x.",
        true,
        Some(format!("speaking_speed={speed}")),
    )
}

/// Validates that a voice identifier belongs to an English (`en-*` or `default`) voice preset.
pub fn validate_english_voice_id(raw: &str) -> Result<String, SessionError> {
    let cleaned = sanitize_untrusted_text(raw);
    let trimmed = cleaned.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("default") {
        return Ok(DEFAULT_ENGLISH_VOICE_ID.to_string());
    }
    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("en") || lower.contains("english") {
        return Ok(trimmed.chars().take(80).collect());
    }
    Err(SessionError::new(
        SessionErrorKind::TtsFailed,
        "Only English (en-*) voices are supported in FluentDesk AI.",
        true,
        Some(format!("english_voice_id={trimmed}")),
    ))
}
