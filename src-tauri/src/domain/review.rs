//! Post-session AI-estimate review, text-only Learner Fact, Learner Progress, and scaffold snapshot models.

use crate::domain::{CefrLevel, CommunicativeGoal, PedagogicalScaffold, SessionId, SessionPhase};
use serde::{Deserialize, Serialize};

/// Asynchronous post-session pedagogical reflection (explicitly labeled as an AI Estimate).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionReview {
    pub session_id: SessionId,
    pub scenario_id: String,
    pub estimate_label: String,
    pub summary: String,
    pub cefr_signal: CefrLevel,
    pub completed_goal_ids: Vec<String>,
    pub grammar_focuses: Vec<String>,
    pub vocabulary_used: Vec<String>,
    pub turn_count: u32,
    pub learner_word_count: u32,
    pub created_at_unix_ms: i64,
}

/// Concise, text-only, user-deletable Learner Fact extracted from a completed session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LearnerFact {
    pub id: String,
    pub fact_text: String,
    pub source_session_id: Option<String>,
    pub created_at_unix_ms: i64,
}

/// Cumulative pure-English Learner Progress snapshot distilled from completed sessions and reviews.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LearnerProgress {
    pub total_sessions: u32,
    pub practice_minutes: u32,
    pub completed_goals_count: u32,
    pub vocabulary_used: Vec<String>,
    pub day_streak: u32,
}

/// Unified serializable snapshot of `PedagogicalScaffold` shared across status, HTTP, and WS seams.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PedagogicalScaffoldSnapshot {
    pub phase: SessionPhase,
    pub turn_count: u32,
    pub max_turns: u32,
    pub cefr_level: CefrLevel,
    pub topic: String,
    pub scenario_id: String,
    #[serde(default)]
    pub target_vocabulary: Vec<String>,
    pub goals: Vec<CommunicativeGoal>,
    pub completed_goal_ids: Vec<String>,
}

impl PedagogicalScaffoldSnapshot {
    pub fn from_scaffold(sc: &PedagogicalScaffold) -> Self {
        Self {
            phase: sc.current_phase(),
            turn_count: sc.user_turns_completed,
            max_turns: sc.max_turns.as_u32(),
            cefr_level: sc.cefr_level,
            topic: sc.topic.as_str().to_string(),
            scenario_id: sc.scenario_id.clone(),
            target_vocabulary: sc.target_vocabulary.clone(),
            goals: sc.goals.clone(),
            completed_goal_ids: sc.completed_goal_ids.clone(),
        }
    }
}
