//! Scenario persona, goal checklist, CEFR scaffolding, and memory formatting helpers.

use crate::domain::{
    find_scenario, sanitize_untrusted_text, CefrLevel, PedagogicalScaffold, SessionPhase,
};

const MAX_VOCAB_TERMS: usize = 12;
const MAX_LEARNER_FACTS: usize = 5;

pub(super) fn format_director_block(sc: &PedagogicalScaffold) -> String {
    let mut lines = vec![
        format!(
            "Level: CEFR {}. Topic: {}.",
            sc.cefr_level.as_str(),
            sc.topic.as_str()
        ),
        persona_line(&sc.scenario_id),
        cefr_scaffolding_line(sc.cefr_level).to_string(),
        phase_directive(sc),
    ];
    if let Some(v) = format_vocab_line(&sc.target_vocabulary) {
        lines.push(v);
    }
    if let Some(g) = format_goals_line(sc) {
        lines.push(g);
    }
    if let Some(f) = format_facts_line(&sc.learner_facts) {
        lines.push(f);
    }
    lines.join("\n")
}

fn persona_line(scenario_id: &str) -> String {
    let sc = find_scenario(scenario_id).unwrap_or_else(crate::domain::default_scenario);
    format!(
        "Roleplay persona: {} (learner role: {}).",
        sc.tutor_persona, sc.learner_role
    )
}

fn cefr_scaffolding_line(level: CefrLevel) -> &'static str {
    match level {
        CefrLevel::A1 | CefrLevel::A2 => "Use short, high-frequency everyday phrasing; if helpful, include one simple starter phrase.",
        CefrLevel::B1 | CefrLevel::B2 => "Use natural everyday phrasing with light idiomatic expressions.",
        CefrLevel::C1 | CefrLevel::C2 => "Engage fluently with nuanced vocabulary and natural conversational pacing.",
    }
}

fn phase_directive(sc: &PedagogicalScaffold) -> String {
    let left = sc.remaining_turns();
    match sc.current_phase() {
        SessionPhase::Opener => "Phase 1 Opener: Greet warmly; only recast gently if confused or silent.".into(),
        SessionPhase::FirstQuestion => "Phase 2 First Question: Respond briefly and ask one easy opening question.".into(),
        SessionPhase::TopicExchange => format!("Phase 3 Topic Exchange ({left} turns left): Deepen the roleplay naturally."),
        SessionPhase::Closer | SessionPhase::Completed => "Phase 4 Closer: Celebrate progress warmly. Set followUpPrompt to null and ask no further question.".into(),
    }
}

fn format_vocab_line(words: &[String]) -> Option<String> {
    let clean: Vec<String> = words
        .iter()
        .map(|w| sanitize_untrusted_text(w))
        .filter(|w| !w.is_empty())
        .take(MAX_VOCAB_TERMS)
        .collect();
    (!clean.is_empty()).then(|| {
        format!(
            "Weave in target vocabulary where natural: {}",
            clean.join(", ")
        )
    })
}

fn format_goals_line(sc: &PedagogicalScaffold) -> Option<String> {
    let pending: Vec<String> = sc
        .goals
        .iter()
        .filter(|g| !sc.completed_goal_ids.iter().any(|d| d == &g.id))
        .map(|g| format!("{} ({})", g.id, g.description))
        .collect();
    (!pending.is_empty()).then(|| {
        format!(
            "Remaining Communicative Goals (append newly met IDs to \"completedGoalIds\"): {}",
            pending.join("; ")
        )
    })
}

fn format_facts_line(facts: &[String]) -> Option<String> {
    let clean: Vec<String> = facts
        .iter()
        .map(|f| sanitize_untrusted_text(f))
        .filter(|f| !f.is_empty())
        .take(MAX_LEARNER_FACTS)
        .collect();
    (!clean.is_empty()).then(|| format!("Known Learner Facts: {}", clean.join(" | ")))
}
