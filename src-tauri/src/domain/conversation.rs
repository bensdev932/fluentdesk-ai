//! Transcript, conversation turn, tutor reply, and history summary domain models.

use crate::domain::{CorrectionMode, DomainValidationError, SessionId};
use serde::{Deserialize, Serialize};

/// Speech recognition result (`plan.md` Section 4).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Transcript {
    pub text: String,
    pub language: String,
    pub duration_ms: u32,
    pub confidence: Option<f32>,
}

impl Transcript {
    pub fn new(
        text: impl Into<String>,
        language: impl Into<String>,
        duration_ms: u32,
        confidence: Option<f32>,
    ) -> Result<Self, DomainValidationError> {
        validate_confidence(confidence)?;
        let cleaned_text = sanitize_untrusted_text(&text.into());
        let cleaned_lang = normalize_language(language.into());
        Ok(Self {
            text: cleaned_text,
            language: cleaned_lang,
            duration_ms,
            confidence,
        })
    }

    pub fn is_empty_or_whitespace(&self) -> bool {
        self.text.trim().is_empty()
    }
}

fn validate_confidence(confidence: Option<f32>) -> Result<(), DomainValidationError> {
    if let Some(c) = confidence {
        if !(0.0..=1.0).contains(&c) || c.is_nan() {
            return Err(DomainValidationError::InvalidConfidence(format!("{c}")));
        }
    }
    Ok(())
}

fn normalize_language(lang: String) -> String {
    let trimmed = lang.trim();
    if trimmed.is_empty() {
        "en".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Speaker role within a conversation turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConversationRole {
    User,
    Tutor,
}

/// Non-sensitive stage timing and mode metadata attached to a conversation turn.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnStageMetadata {
    pub stt_duration_ms: Option<u32>,
    pub llm_duration_ms: Option<u32>,
    pub tts_duration_ms: Option<u32>,
    pub correction_mode: Option<CorrectionMode>,
}

/// Persisted or in-memory conversation turn (`plan.md` Section 4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationTurn {
    pub id: String,
    pub session_id: SessionId,
    pub role: ConversationRole,
    pub text: String,
    pub correction: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proper_way: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
    pub follow_up_prompt: Option<String>,
    pub timestamp_unix_ms: i64,
    pub stage_metadata: Option<TurnStageMetadata>,
}

/// Summary metadata for a past or active speaking session (`T052`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    pub session_id: SessionId,
    pub started_at_unix_ms: i64,
    pub ended_at_unix_ms: Option<i64>,
    pub correction_mode: CorrectionMode,
    pub turn_count: u32,
}

/// Chronological session review payload without raw audio (`T052`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionHistoryView {
    pub session_id: SessionId,
    pub started_at_unix_ms: i64,
    pub ended_at_unix_ms: Option<i64>,
    pub turns: Vec<ConversationTurn>,
}

/// Validated tutor response (`plan.md` Section 4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TutorReply {
    pub spoken_reply: String,
    pub correction: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proper_way: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
    pub follow_up_prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub completed_goal_ids: Vec<String>,
}

impl TutorReply {
    pub fn new_validated(
        spoken_reply: impl Into<String>,
        correction: Option<String>,
        follow_up_prompt: Option<String>,
    ) -> Result<Self, DomainValidationError> {
        let cleaned_spoken = sanitize_untrusted_text(&spoken_reply.into());
        if cleaned_spoken.is_empty() {
            return Err(DomainValidationError::EmptyTutorReply);
        }
        Ok(Self {
            spoken_reply: cleaned_spoken,
            correction: clean_optional_text(correction),
            proper_way: None,
            why: None,
            follow_up_prompt: clean_optional_text(follow_up_prompt),
            completed_goal_ids: Vec::new(),
        })
    }

    pub fn with_structured_feedback(
        mut self,
        proper_way: Option<String>,
        why: Option<String>,
    ) -> Self {
        self.proper_way = clean_optional_text(proper_way);
        self.why = clean_optional_text(why);
        self
    }

    pub fn with_completed_goals(mut self, ids: Vec<String>) -> Self {
        self.completed_goal_ids = ids;
        self
    }
}

fn clean_optional_text(value: Option<String>) -> Option<String> {
    value
        .map(|s| sanitize_untrusted_text(&s))
        .filter(|s| !s.is_empty())
}

/// Strips control characters (except `\n` and `\t`) and trims whitespace from untrusted model output.
pub fn sanitize_untrusted_text(raw: &str) -> String {
    raw.chars()
        .filter(|&ch| !ch.is_control() || ch == '\n' || ch == '\t')
        .collect::<String>()
        .trim()
        .to_string()
}
