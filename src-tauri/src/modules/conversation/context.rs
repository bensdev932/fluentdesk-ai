//! Bounded conversation context management (`T030`).

use crate::domain::{ConversationRole, ConversationTurn, SessionError, SessionErrorKind};
use std::collections::VecDeque;

/// Outcome of reconciling the most recent tutor turn after barge-in or completed playback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TurnReconcileAction {
    Removed(String),
    Truncated(ConversationTurn),
    DurationUpdated(String, u32),
    NoChange,
}

const MAX_ALLOWED_CONTEXT_TURNS: usize = 50;

/// Value object representing the maximum number of turns retained in active LLM context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MaxContextTurns(usize);

impl MaxContextTurns {
    pub fn try_new(max_turns: usize) -> Result<Self, SessionError> {
        if !(1..=MAX_ALLOWED_CONTEXT_TURNS).contains(&max_turns) {
            return Err(invalid_context_bound_error(max_turns));
        }
        Ok(Self(max_turns))
    }

    pub fn as_usize(self) -> usize {
        self.0
    }
}

fn invalid_context_bound_error(max_turns: usize) -> SessionError {
    SessionError::new(
        SessionErrorKind::InvalidStateTransition,
        "Invalid conversation context window size.",
        true,
        Some(format!(
            "max_turns={max_turns} outside 1..={MAX_ALLOWED_CONTEXT_TURNS}"
        )),
    )
}

/// Bounded FIFO window of recent conversation turns (`T030`).
#[derive(Debug, Clone)]
pub struct BoundedConversationContext {
    max_turns: MaxContextTurns,
    turns: VecDeque<ConversationTurn>,
}

impl BoundedConversationContext {
    pub fn new(max_turns: MaxContextTurns) -> Self {
        Self {
            max_turns,
            turns: VecDeque::with_capacity(max_turns.as_usize()),
        }
    }

    pub fn push_turn(&mut self, turn: ConversationTurn) {
        if self.turns.len() >= self.max_turns.as_usize() {
            self.turns.pop_front();
        }
        self.turns.push_back(turn);
    }

    pub fn recent_turns(&self) -> Vec<ConversationTurn> {
        self.turns.iter().cloned().collect()
    }

    pub fn turn_count(&self) -> usize {
        self.turns.len()
    }

    pub fn clear(&mut self) {
        self.turns.clear();
    }

    pub fn record_last_tutor_tts_duration(&mut self, duration_ms: u32) -> Option<String> {
        let turn = self
            .turns
            .back_mut()
            .filter(|t| t.role == ConversationRole::Tutor)?;
        set_turn_tts_ms(turn, duration_ms);
        Some(turn.id.clone())
    }

    pub fn reconcile_last_tutor_turn(
        &mut self,
        heard_text: Option<&str>,
        played_ms: Option<u32>,
    ) -> TurnReconcileAction {
        let Some(turn) = self
            .turns
            .back_mut()
            .filter(|t| t.role == ConversationRole::Tutor)
        else {
            return TurnReconcileAction::NoChange;
        };
        if heard_text.is_some_and(|raw| raw.trim().is_empty()) {
            let removed = self.turns.pop_back().expect("tutor turn present");
            return TurnReconcileAction::Removed(removed.id);
        }
        apply_partial_reconciliation(turn, heard_text, played_ms)
    }
}

fn apply_partial_reconciliation(
    turn: &mut ConversationTurn,
    heard_text: Option<&str>,
    played_ms: Option<u32>,
) -> TurnReconcileAction {
    if let Some(ms) = played_ms {
        set_turn_tts_ms(turn, ms);
    }
    if let Some(clean) = heard_text
        .map(str::trim)
        .filter(|s| s.len() < turn.text.len())
    {
        turn.text = clean.to_string();
        turn.follow_up_prompt = None;
        return TurnReconcileAction::Truncated(turn.clone());
    }
    played_ms
        .map(|ms| TurnReconcileAction::DurationUpdated(turn.id.clone(), ms))
        .unwrap_or(TurnReconcileAction::NoChange)
}

fn set_turn_tts_ms(turn: &mut ConversationTurn, duration_ms: u32) {
    turn.stage_metadata
        .get_or_insert_with(Default::default)
        .tts_duration_ms = Some(duration_ms);
}
