//! Voice-first tutor prompt assembly facade delegating to `RoleplayDirector`.

use crate::domain::{
    ConversationRole, ConversationTurn, CorrectionMode, PedagogicalScaffold, Transcript,
};
use crate::modules::tutor::roleplay_director::RoleplayDirector;

/// Single chronological dialogue message preserved for multi-turn chat completions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatMessage {
    pub role: ConversationRole,
    pub content: String,
}

/// Value object holding system instructions, structured turns, and formatted prompt text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TutorPrompt {
    system_instruction: String,
    messages: Vec<ChatMessage>,
    user_prompt: String,
}

impl TutorPrompt {
    /// Creates a `TutorPrompt` from system instructions and formatted user prompt text.
    pub fn new(system_instruction: String, user_prompt: String) -> Self {
        Self::with_messages(system_instruction, Vec::new(), user_prompt)
    }

    /// Creates a `TutorPrompt` preserving chronological `ChatMessage` turns alongside `user_prompt`.
    pub fn with_messages(
        system_instruction: String,
        messages: Vec<ChatMessage>,
        user_prompt: String,
    ) -> Self {
        Self {
            system_instruction,
            messages,
            user_prompt,
        }
    }

    pub fn system_instruction(&self) -> &str {
        &self.system_instruction
    }

    pub fn messages(&self) -> &[ChatMessage] {
        &self.messages
    }

    pub fn user_prompt(&self) -> &str {
        &self.user_prompt
    }
}

/// Assembles a voice-first tutor prompt from bounded conversation history and learner transcript.
pub fn assemble_voice_tutor_prompt(
    history: &[ConversationTurn],
    user_transcript: &Transcript,
    correction_mode: CorrectionMode,
) -> TutorPrompt {
    RoleplayDirector::build_prompt(history, user_transcript, correction_mode, None)
}

/// Assembles a tutor prompt anchored to active Roleplay Scenario, CEFR level, and 4-phase scaffolding.
pub fn assemble_scaffolded_tutor_prompt(
    history: &[ConversationTurn],
    user_transcript: &Transcript,
    correction_mode: CorrectionMode,
    scaffold: Option<&PedagogicalScaffold>,
) -> TutorPrompt {
    RoleplayDirector::build_prompt(history, user_transcript, correction_mode, scaffold)
}
