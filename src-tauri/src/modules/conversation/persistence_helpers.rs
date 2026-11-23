//! Turn builders and stage metadata helpers split from `persistence.rs`.

use crate::domain::{
    ConversationRole, ConversationTurn, CorrectionMode, SessionError, SessionErrorKind, SessionId,
    SessionState, TurnStageMetadata, TutorReply,
};

#[derive(Default)]
pub(crate) struct TurnCoaching {
    pub correction: Option<String>,
    pub proper_way: Option<String>,
    pub why: Option<String>,
}

impl TurnCoaching {
    pub fn from_reply(r: &TutorReply) -> Self {
        Self {
            correction: r.correction.clone(),
            proper_way: r.proper_way.clone(),
            why: r.why.clone(),
        }
    }

    pub fn legacy_only(correction: Option<String>) -> Self {
        Self {
            correction,
            ..Default::default()
        }
    }
}

pub(crate) fn stage_meta(
    stt: Option<u32>,
    llm: Option<u32>,
    mode: CorrectionMode,
) -> TurnStageMetadata {
    TurnStageMetadata {
        stt_duration_ms: stt,
        llm_duration_ms: llm,
        tts_duration_ms: None,
        correction_mode: Some(mode),
    }
}

pub(crate) fn invalid_transition_err(from: SessionState, to: SessionState) -> SessionError {
    SessionError::new(
        SessionErrorKind::InvalidStateTransition,
        "Cannot perform that session action in the current state.",
        true,
        Some(format!("Invalid transition from {from:?} to {to:?}")),
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn build_turn(
    sid: &SessionId,
    seq: u32,
    role: ConversationRole,
    text: &str,
    coaching: TurnCoaching,
    follow_up_prompt: Option<String>,
    timestamp_unix_ms: i64,
    meta: TurnStageMetadata,
) -> ConversationTurn {
    ConversationTurn {
        id: format!("{}-turn-{seq}", sid.as_str()),
        session_id: sid.clone(),
        role,
        text: text.to_string(),
        correction: coaching.correction,
        proper_way: coaching.proper_way,
        why: coaching.why,
        follow_up_prompt,
        timestamp_unix_ms,
        stage_metadata: Some(meta),
    }
}

pub(crate) fn current_unix_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
