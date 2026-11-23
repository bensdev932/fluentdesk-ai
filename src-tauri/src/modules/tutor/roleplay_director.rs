//! Deep Roleplay Director building phase-aware, CEFR-scoped prompts without contradictions.

use crate::domain::{
    sanitize_untrusted_text, CefrLevel, ConversationRole, ConversationTurn, CorrectionMode,
    PedagogicalScaffold, SessionPhase, Transcript,
};
use crate::modules::tutor::director_context::format_director_block;
use crate::modules::tutor::director_rules::{
    correction_rule, ANTI_FILLER_RULE, ENGAGEMENT_RULE, EVALUATION_RULES, IDENTITY_RULE,
    JSON_CONTRACT_RULE, NATIVE_STYLE_RULE,
};
use crate::modules::tutor::prompts::{ChatMessage, TutorPrompt};

/// Single cohesive director assembling system rules, scenario persona, goals, and dialogue turns.
pub struct RoleplayDirector;

impl RoleplayDirector {
    pub fn build_prompt(
        history: &[ConversationTurn],
        transcript: &Transcript,
        mode: CorrectionMode,
        scaffold: Option<&PedagogicalScaffold>,
    ) -> TutorPrompt {
        let system = build_system_instruction(mode, scaffold);
        let messages = build_structured_messages(history, transcript);
        let user_prompt = build_user_prompt(history, transcript);
        TutorPrompt::with_messages(system, messages, user_prompt)
    }
}

pub(crate) fn word_budget_for_cefr(level: CefrLevel) -> u16 {
    match level {
        CefrLevel::A1 => 25,
        CefrLevel::A2 => 35,
        CefrLevel::B1 => 50,
        CefrLevel::B2 => 65,
        CefrLevel::C1 | CefrLevel::C2 => 80,
    }
}

fn build_system_instruction(
    mode: CorrectionMode,
    scaffold: Option<&PedagogicalScaffold>,
) -> String {
    let phase = scaffold
        .map(|s| s.current_phase())
        .unwrap_or(SessionPhase::Opener);
    let budget = word_budget_for_cefr(scaffold.map(|s| s.cefr_level).unwrap_or_default());
    let mut parts = vec![
        IDENTITY_RULE.to_string(),
        NATIVE_STYLE_RULE.to_string(),
        ENGAGEMENT_RULE.to_string(),
        ANTI_FILLER_RULE.to_string(),
        format!("Keep \"spokenReply\" conversational, voice-friendly, and under ~{budget} words."),
        EVALUATION_RULES.to_string(),
        correction_rule(mode).to_string(),
        phase_closing_rule(phase).to_string(),
    ];
    if let Some(sc) = scaffold {
        parts.push(format_director_block(sc));
    }
    parts.push(JSON_CONTRACT_RULE.to_string());
    parts.join("\n")
}

fn phase_closing_rule(phase: SessionPhase) -> &'static str {
    if matches!(phase, SessionPhase::Closer | SessionPhase::Completed) {
        "Closing turn rule: Set followUpPrompt to null and ask no further question."
    } else {
        "End with one specific follow-up question tied to their topic and put only that question in \"followUpPrompt\"."
    }
}

fn build_structured_messages(history: &[ConversationTurn], tx: &Transcript) -> Vec<ChatMessage> {
    let mut msgs: Vec<ChatMessage> = history
        .iter()
        .filter_map(|t| {
            let content = sanitize_untrusted_text(&t.text);
            (!content.is_empty()).then_some(ChatMessage {
                role: t.role,
                content,
            })
        })
        .collect();
    msgs.push(ChatMessage {
        role: ConversationRole::User,
        content: sanitize_untrusted_text(&tx.text),
    });
    msgs
}

fn build_user_prompt(history: &[ConversationTurn], tx: &Transcript) -> String {
    let lines: Vec<String> = history
        .iter()
        .filter_map(|t| {
            let text = sanitize_untrusted_text(&t.text);
            let label = if matches!(t.role, ConversationRole::User) {
                "User"
            } else {
                "Tutor"
            };
            (!text.is_empty()).then(|| format!("{label}: {text}"))
        })
        .collect();
    let history_block = if lines.is_empty() {
        "(start of conversation)".into()
    } else {
        lines.join("\n")
    };
    format!(
        "Conversation history (oldest to newest):\n{history_block}\n\nLearner just said ({}):\nUser: {}",
        tx.language,
        sanitize_untrusted_text(&tx.text)
    )
}
