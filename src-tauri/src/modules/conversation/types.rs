//! Shared session command and status types for Speaking Session.

use crate::ai::router_types::RoutedTutorReply;
use crate::domain::{
    build_scenario_opener_for_cefr, CefrLevel, ConversationTurn, CorrectionMode,
    PedagogicalScaffold, PedagogicalScaffoldSnapshot, SessionId, SessionReview, SessionState,
    TutorReply,
};
use serde::{Deserialize, Serialize};
use std::ops::Deref;

/// Validated configuration passed to start a speaking session.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartSpeakingSessionConfig {
    #[serde(default)]
    pub correction_mode: CorrectionMode,
    #[serde(default)]
    pub push_to_talk: bool,
    #[serde(default)]
    pub input_device_id: Option<String>,
    #[serde(default)]
    pub cefr_level: Option<CefrLevel>,
    #[serde(default)]
    pub scenario_id: Option<String>,
    #[serde(default)]
    pub topic: Option<String>,
    #[serde(default)]
    pub target_vocabulary: Vec<String>,
    #[serde(default)]
    pub max_turns: Option<u32>,
}

/// Snapshot of the current speaking session status returned to callers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionStatusSnapshot {
    pub session_id: SessionId,
    pub state: SessionState,
    pub correction_mode: CorrectionMode,
    pub push_to_talk: bool,
    #[serde(flatten)]
    pub scaffold: PedagogicalScaffoldSnapshot,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opener_reply: Option<TutorReply>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_review: Option<SessionReview>,
}

impl Deref for SessionStatusSnapshot {
    type Target = PedagogicalScaffoldSnapshot;
    fn deref(&self) -> &Self::Target {
        &self.scaffold
    }
}

impl SessionStatusSnapshot {
    pub(crate) fn from_manager(
        session_id: SessionId,
        state: SessionState,
        correction_mode: CorrectionMode,
        push_to_talk: bool,
        sc: &PedagogicalScaffold,
        session_review: Option<SessionReview>,
    ) -> Self {
        let opener_reply =
            (sc.user_turns_completed == 0 && state == SessionState::Listening).then(|| {
                build_scenario_opener_for_cefr(
                    &sc.scenario_id,
                    sc.topic.as_str(),
                    &sc.learner_facts,
                    sc.cefr_level,
                )
            });
        Self {
            session_id,
            state,
            correction_mode,
            push_to_talk,
            scaffold: PedagogicalScaffoldSnapshot::from_scaffold(sc),
            opener_reply,
            session_review,
        }
    }
}

/// Completed text-submitted turn outcome including routed provider metadata and final state.
#[derive(Debug, Clone)]
pub struct TextTurnOutcome {
    pub session_id: SessionId,
    pub user_turn: ConversationTurn,
    pub tutor_turn: ConversationTurn,
    pub routed: RoutedTutorReply,
    pub state: SessionState,
    pub scaffold: PedagogicalScaffoldSnapshot,
    pub session_review: Option<SessionReview>,
}

impl Deref for TextTurnOutcome {
    type Target = PedagogicalScaffoldSnapshot;
    fn deref(&self) -> &Self::Target {
        &self.scaffold
    }
}
