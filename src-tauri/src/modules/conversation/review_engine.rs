//! Asynchronous post-session review builder & text-only Learner Fact extraction engine.

use crate::database::repository::SessionRepository;
use crate::domain::{
    default_scenario, detect_completed_goal_ids, find_scenario, sanitize_untrusted_text,
    ConversationRole, ConversationTurn, LearnerFact, PedagogicalScaffold, SessionError, SessionId,
    SessionReview,
};
use std::collections::HashSet;

const ESTIMATE_LABEL: &str = "AI Estimate";
const MAX_FACTS_PER_SESSION: usize = 3;
const MAX_FACT_CHARS: usize = 90;
const FACT_PREFIXES: &[&str] = &[
    "my name is ",
    "i work as ",
    "i am a ",
    "i'm a ",
    "i live in ",
    "i study ",
    "my favorite ",
    "my hobby is ",
    "i'm preparing for ",
    "learning english for ",
];

/// Computes and persists a `SessionReview` and text-only `LearnerFact` entries from SQLite turns.
pub(crate) async fn generate_and_persist(
    repo: &SessionRepository,
    user_id: &str,
    sid: &SessionId,
    scaffold: &PedagogicalScaffold,
    now_ms: i64,
) -> Result<SessionReview, SessionError> {
    let turns = repo.list_turns_chronological(sid).await?;
    let review = build_session_review(sid, scaffold, &turns, now_ms);
    for fact in extract_learner_facts(sid, &turns, now_ms) {
        repo.upsert_learner_fact(user_id, &fact).await?;
    }
    repo.save_session_review(&review).await?;
    Ok(review)
}

/// Builds an explicitly labeled `AI Estimate` pedagogical review without touching raw audio.
pub fn build_session_review(
    sid: &SessionId,
    scaffold: &PedagogicalScaffold,
    turns: &[ConversationTurn],
    now_ms: i64,
) -> SessionReview {
    let user_turns: Vec<&ConversationTurn> = turns
        .iter()
        .filter(|t| t.role == ConversationRole::User)
        .collect();
    let learner_words: u32 = user_turns
        .iter()
        .map(|t| t.text.split_whitespace().count() as u32)
        .sum();
    let completed = merge_completed_goals(scaffold, &user_turns);
    let focuses = collect_grammar_focuses(turns);
    let vocab_used = collect_vocab_used(scaffold, &user_turns);
    let scenario = find_scenario(&scaffold.scenario_id).unwrap_or_else(default_scenario);
    let summary = format_review_summary(
        &scenario.title,
        &completed,
        scaffold.goals.len(),
        user_turns.len(),
        learner_words,
    );
    SessionReview {
        session_id: sid.clone(),
        scenario_id: scaffold.scenario_id.clone(),
        estimate_label: ESTIMATE_LABEL.to_string(),
        summary,
        cefr_signal: scaffold.cefr_level,
        completed_goal_ids: completed,
        grammar_focuses: focuses,
        vocabulary_used: vocab_used,
        turn_count: user_turns.len() as u32,
        learner_word_count: learner_words,
        created_at_unix_ms: now_ms,
    }
}

/// Extracts concise, sanitized, text-only Learner Facts from user turns.
pub fn extract_learner_facts(
    sid: &SessionId,
    turns: &[ConversationTurn],
    now_ms: i64,
) -> Vec<LearnerFact> {
    let mut facts = Vec::new();
    for turn in turns.iter().filter(|t| t.role == ConversationRole::User) {
        for clause in turn.text.split(['.', '!', '?', '\n']) {
            if facts.len() >= MAX_FACTS_PER_SESSION {
                return facts;
            }
            if let Some(clean) = extract_clause_fact(clause) {
                facts.push(make_fact(sid, facts.len(), clean, now_ms));
            }
        }
    }
    facts
}

fn extract_clause_fact(clause: &str) -> Option<String> {
    let trimmed = sanitize_untrusted_text(clause);
    let lower = trimmed.to_ascii_lowercase();
    let matched = FACT_PREFIXES.iter().any(|p| lower.contains(p));
    let len_ok = (10..=MAX_FACT_CHARS).contains(&trimmed.chars().count());
    (matched && len_ok).then_some(trimmed)
}

fn make_fact(sid: &SessionId, idx: usize, fact_text: String, now_ms: i64) -> LearnerFact {
    LearnerFact {
        id: format!("fact-{}-{idx}", sid.as_str()),
        fact_text,
        source_session_id: Some(sid.as_str().to_string()),
        created_at_unix_ms: now_ms,
    }
}

fn merge_completed_goals(
    scaffold: &PedagogicalScaffold,
    user_turns: &[&ConversationTurn],
) -> Vec<String> {
    let mut done = scaffold.completed_goal_ids.clone();
    for ut in user_turns {
        for id in detect_completed_goal_ids(&scaffold.goals, &ut.text, &done) {
            if !done.contains(&id) {
                done.push(id);
            }
        }
    }
    done
}

fn collect_grammar_focuses(turns: &[ConversationTurn]) -> Vec<String> {
    let mut out = Vec::new();
    for t in turns.iter().filter(|t| t.role == ConversationRole::Tutor) {
        if let Some(note) = t
            .correction
            .as_deref()
            .map(sanitize_untrusted_text)
            .filter(|s| !s.is_empty())
        {
            if !out.contains(&note) && out.len() < 4 {
                out.push(note);
            }
        }
    }
    out
}

fn collect_vocab_used(
    scaffold: &PedagogicalScaffold,
    user_turns: &[&ConversationTurn],
) -> Vec<String> {
    let joined = user_turns
        .iter()
        .map(|t| t.text.to_ascii_lowercase())
        .collect::<Vec<_>>()
        .join(" ");
    let mut seen = HashSet::new();
    scaffold
        .target_vocabulary
        .iter()
        .filter(|w| joined.contains(&w.to_ascii_lowercase()))
        .filter(|w| seen.insert(w.to_ascii_lowercase()))
        .cloned()
        .collect()
}

fn format_review_summary(
    title: &str,
    done: &[String],
    total: usize,
    turns: usize,
    words: u32,
) -> String {
    format!(
        "{title}: {}/{} goals met across {turns} learner turns ({words} words spoken). AI Estimate for practice reflection.",
        done.len(),
        total
    )
}
