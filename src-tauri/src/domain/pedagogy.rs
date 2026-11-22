//! Domain value objects for CEFR proficiency, 4-phase pedagogical scaffolding, and turn bounds.

use crate::domain::{sanitize_untrusted_text, DomainValidationError};
use serde::{Deserialize, Serialize};
use std::str::FromStr;

pub const DEFAULT_MAX_SESSION_TURNS: u32 = 6;
pub const MIN_SESSION_TURNS: u32 = 2;
pub const MAX_SESSION_TURNS: u32 = 20;

/// Common European Framework of Reference proficiency level (`A1`–`C2`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CefrLevel {
    A1,
    A2,
    #[default]
    B1,
    B2,
    C1,
    C2,
}

impl CefrLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::A1 => "A1",
            Self::A2 => "A2",
            Self::B1 => "B1",
            Self::B2 => "B2",
            Self::C1 => "C1",
            Self::C2 => "C2",
        }
    }

    pub fn difficulty_hint(self) -> &'static str {
        match self {
            Self::A1 => {
                "CEFR A1 beginner: use very simple words, present tense, and short clauses."
            }
            Self::A2 => {
                "CEFR A2 elementary: use common everyday phrases and slow, clear sentences."
            }
            Self::B1 => "CEFR B1 intermediate: use natural daily conversational English cleanly.",
            Self::B2 => {
                "CEFR B2 upper-intermediate: encourage fluent idioms and follow-up reasoning."
            }
            Self::C1 => "CEFR C1 advanced: challenge nuance, abstract ideas, and precise phrasing.",
            Self::C2 => "CEFR C2 proficient: converse at natural native pace and subtle nuance.",
        }
    }
}

impl FromStr for CefrLevel {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, ()> {
        match s.trim().to_ascii_uppercase().as_str() {
            "A1" => Ok(Self::A1),
            "A2" => Ok(Self::A2),
            "B1" => Ok(Self::B1),
            "B2" => Ok(Self::B2),
            "C1" => Ok(Self::C1),
            "C2" => Ok(Self::C2),
            _ => Err(()),
        }
    }
}

/// Four-phase conversational pedagogy scaffold plus terminal `Completed` state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SessionPhase {
    #[default]
    Opener,
    FirstQuestion,
    TopicExchange,
    Closer,
    Completed,
}

impl SessionPhase {
    pub fn label(self) -> &'static str {
        match self {
            Self::Opener => "Opener",
            Self::FirstQuestion => "First Question",
            Self::TopicExchange => "Topic Exchange",
            Self::Closer => "Closer",
            Self::Completed => "Completed",
        }
    }
}

/// Validated upper turn bound (`2..=20`) preventing learner cognitive fatigue & drift.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MaxSessionTurns(u32);

impl Default for MaxSessionTurns {
    fn default() -> Self {
        Self(DEFAULT_MAX_SESSION_TURNS)
    }
}

impl MaxSessionTurns {
    pub fn try_new(raw: u32) -> Result<Self, DomainValidationError> {
        if !(MIN_SESSION_TURNS..=MAX_SESSION_TURNS).contains(&raw) {
            return Err(DomainValidationError::InvalidSessionTurnBound(raw));
        }
        Ok(Self(raw))
    }

    pub fn as_u32(self) -> u32 {
        self.0
    }
}

/// Validated sanitized practice topic (`<= 120` chars).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SessionTopic(String);

impl Default for SessionTopic {
    fn default() -> Self {
        Self("Everyday Conversation".to_string())
    }
}

impl SessionTopic {
    pub fn try_new(raw: impl Into<String>) -> Result<Self, DomainValidationError> {
        let cleaned = sanitize_untrusted_text(&raw.into());
        let clipped: String = cleaned.chars().take(120).collect();
        let normalized = clipped.trim().to_string();
        if normalized.is_empty() {
            return Err(DomainValidationError::EmptySessionTopic);
        }
        Ok(Self(normalized))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Encapsulates active CEFR level, Roleplay Scenario, Communicative Goals, and turn progression.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PedagogicalScaffold {
    pub cefr_level: CefrLevel,
    pub topic: SessionTopic,
    pub target_vocabulary: Vec<String>,
    pub max_turns: MaxSessionTurns,
    pub user_turns_completed: u32,
    #[serde(default)]
    pub scenario_id: String,
    #[serde(default)]
    pub goals: Vec<crate::domain::CommunicativeGoal>,
    #[serde(default)]
    pub completed_goal_ids: Vec<String>,
    #[serde(default)]
    pub learner_facts: Vec<String>,
}

impl PedagogicalScaffold {
    pub fn current_phase(&self) -> SessionPhase {
        let max = self.max_turns.as_u32();
        let done = self.user_turns_completed;
        if done >= max {
            SessionPhase::Completed
        } else if done == 0 {
            SessionPhase::Opener
        } else if done == 1 && max > 2 {
            SessionPhase::FirstQuestion
        } else if done + 1 >= max {
            SessionPhase::Closer
        } else {
            SessionPhase::TopicExchange
        }
    }

    pub fn advance_after_turn(&mut self) -> SessionPhase {
        let max = self.max_turns.as_u32();
        self.user_turns_completed = self.user_turns_completed.saturating_add(1).min(max);
        self.current_phase()
    }

    pub fn is_completed(&self) -> bool {
        self.current_phase() == SessionPhase::Completed
    }

    pub fn remaining_turns(&self) -> u32 {
        self.max_turns
            .as_u32()
            .saturating_sub(self.user_turns_completed)
    }
}
