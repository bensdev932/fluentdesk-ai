//! Deep Roleplay Director building phase-aware, CEFR-scoped prompts without contradictions.

use crate::domain::{
    sanitize_untrusted_text, CefrLevel, ConversationRole, ConversationTurn, CorrectionMode,
    PedagogicalScaffold, SessionPhase, Transcript,
};
use crate::modules::tutor::director_context::format_director_block;
use crate::modules::tutor::prompts::{ChatMessage, TutorPrompt};

const ENGAGEMENT_RULE: &str =
    "React directly to the specific details, names, events, or feelings the learner just shared first, and continue naturally from prior turns.";
const ANTI_FILLER_RULE: &str =
    "Never use canned filler like 'Thanks for sharing that naturally — your meaning came through clearly!', 'That is interesting!', or 'Could you tell me more about that?'.";

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
        "You are FluentDesk AI, a voice-first spoken English tutor in character.".to_string(),
        ENGAGEMENT_RULE.to_string(),
        ANTI_FILLER_RULE.to_string(),
        format!("Keep \"spokenReply\" conversational, voice-friendly, and under ~{budget} words."),
        correction_rule(mode).to_string(),
        phase_closing_rule(phase).to_string(),
    ];
    if let Some(sc) = scaffold {
        parts.push(format_director_block(sc));
    }
    parts.push("Return ONLY valid JSON with \"spokenReply\" first and metadata last: {\"spokenReply\":\"...\",\"correction\":null,\"followUpPrompt\":null,\"completedGoalIds\":[]}".to_string());
    parts.join("\n")
}

fn correction_rule(mode: CorrectionMode) -> &'static str {
    match mode {
        CorrectionMode::Natural => "Recast real grammar errors smoothly in \"spokenReply\" and add a brief correction note only for major clarity blockers.",
        CorrectionMode::Gentle => "Keep \"spokenReply\" warm and provide a brief gentle correction in \"correction\" only for real errors.",
        CorrectionMode::Detailed => "Respond warmly in \"spokenReply\" and explain key grammar or vocabulary improvements in \"correction\".",
        CorrectionMode::Off => "Do NOT provide grammar corrections; always set \"correction\" to null.",
    }
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
