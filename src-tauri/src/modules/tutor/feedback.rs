//! Structured tutor response parsing and dynamic follow-up extraction (`T032`).

use crate::domain::{
    sanitize_untrusted_text, CorrectionMode, SessionError, SessionErrorKind, TutorReply,
};
use crate::modules::tutor::feedback_guard::{clean_non_placeholder, resolve_structured_feedback};
use crate::modules::tutor::stream_parser::SpokenJsonStreamParser;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawTutorJson {
    #[serde(alias = "spoken_reply")]
    spoken_reply: String,
    #[serde(default, alias = "needs_improvement")]
    needs_improvement: Option<bool>,
    #[serde(default, alias = "proper_way")]
    proper_way: Option<String>,
    #[serde(default, alias = "feedback_why")]
    why: Option<String>,
    #[serde(default)]
    correction: Option<String>,
    #[serde(default, alias = "follow_up_prompt")]
    follow_up_prompt: Option<String>,
    #[serde(default, alias = "completed_goal_ids")]
    completed_goal_ids: Vec<String>,
}

/// Parses untrusted LLM output as structured JSON and falls back cleanly on plain text (`T032`).
pub fn parse_tutor_response_or_fallback(
    raw_model_output: &str,
    correction_mode: CorrectionMode,
) -> Result<TutorReply, SessionError> {
    parse_tutor_response_for_utterance(raw_model_output, correction_mode, None)
}

/// Parses untrusted LLM output while guarding against no-op rewrites matching `learner_utterance`.
pub fn parse_tutor_response_for_utterance(
    raw_model_output: &str,
    correction_mode: CorrectionMode,
    learner_utterance: Option<&str>,
) -> Result<TutorReply, SessionError> {
    let without_think = strip_think_blocks(raw_model_output.trim());
    let stripped = strip_markdown_code_fence(&without_think);
    if let Some(parsed) = try_parse_raw_json(stripped) {
        return build_validated_reply(parsed, correction_mode, learner_utterance);
    }
    let recovered = extract_truncated_spoken_reply(stripped);
    let text = recovered.as_deref().unwrap_or(stripped);
    let follow_up = extract_follow_up_prompt(text);
    TutorReply::new_validated(text, None, follow_up).map_err(map_empty_reply_error)
}

fn extract_truncated_spoken_reply(stripped: &str) -> Option<String> {
    let start = stripped.find('{')?;
    let mut parser = SpokenJsonStreamParser::new();
    let mut parts = parser.push_token(&stripped[start..]);
    parts.extend(parser.finish().0);
    let joined = parts.join(" ");
    (!joined.trim().is_empty()).then_some(joined)
}

/// Dynamically extracts a distinct closing question sentence from a tutor reply.
pub fn extract_follow_up_prompt(spoken_reply: &str) -> Option<String> {
    let cleaned = sanitize_untrusted_text(spoken_reply);
    let q_pos = cleaned.rfind('?')?;
    let start = find_question_start(&cleaned[..q_pos]);
    let candidate = cleaned[start..=q_pos].trim();
    if candidate.is_empty() || candidate.eq_ignore_ascii_case(&cleaned) {
        return None;
    }
    Some(candidate.to_string())
}

fn build_validated_reply(
    parsed: RawTutorJson,
    correction_mode: CorrectionMode,
    learner_utterance: Option<&str>,
) -> Result<TutorReply, SessionError> {
    let fb = resolve_structured_feedback(
        parsed.needs_improvement,
        parsed.proper_way,
        parsed.why,
        parsed.correction,
        correction_mode,
        learner_utterance,
    );
    let follow_up = resolve_follow_up(&parsed.spoken_reply, parsed.follow_up_prompt);
    let goals = sanitize_goal_ids(parsed.completed_goal_ids);
    TutorReply::new_validated(parsed.spoken_reply, fb.legacy_summary, follow_up)
        .map(|reply| {
            reply
                .with_structured_feedback(fb.proper_way, fb.why)
                .with_completed_goals(goals)
        })
        .map_err(map_empty_reply_error)
}

fn sanitize_goal_ids(raw: Vec<String>) -> Vec<String> {
    let mut out = Vec::new();
    for id in raw
        .into_iter()
        .filter_map(|s| clean_non_placeholder(Some(s)))
    {
        if !out.contains(&id) && out.len() < 8 {
            out.push(id);
        }
    }
    out
}

fn resolve_follow_up(spoken_reply: &str, explicit: Option<String>) -> Option<String> {
    let clean_spoken = sanitize_untrusted_text(spoken_reply);
    clean_non_placeholder(explicit)
        .filter(|f| !f.eq_ignore_ascii_case(&clean_spoken))
        .or_else(|| extract_follow_up_prompt(&clean_spoken))
}

fn try_parse_raw_json(text: &str) -> Option<RawTutorJson> {
    serde_json::from_str::<RawTutorJson>(text)
        .ok()
        .or_else(|| extract_json_object(text).and_then(|s| serde_json::from_str(s).ok()))
}

fn extract_json_object(text: &str) -> Option<&str> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    (start < end).then(|| &text[start..=end])
}

fn strip_think_blocks(raw: &str) -> String {
    let mut out = raw.to_string();
    while let Some((start, end)) = find_think_span(&out) {
        out.replace_range(start..end, "");
    }
    out.trim().to_string()
}

fn find_think_span(text: &str) -> Option<(usize, usize)> {
    let lower = text.to_ascii_lowercase();
    let start = lower.find("<think>")?;
    let close_rel = lower[start..].find("</think>")?;
    Some((start, start + close_rel + "</think>".len()))
}

fn strip_markdown_code_fence(trimmed: &str) -> &str {
    let without_prefix = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .unwrap_or(trimmed);
    without_prefix
        .strip_suffix("```")
        .unwrap_or(without_prefix)
        .trim()
}

fn find_question_start(prefix: &str) -> usize {
    prefix
        .char_indices()
        .rev()
        .find_map(|(idx, ch)| sentence_boundary_end(prefix, idx, ch))
        .unwrap_or(0)
}

fn sentence_boundary_end(prefix: &str, idx: usize, ch: char) -> Option<usize> {
    let next_idx = idx + ch.len_utf8();
    if ch == '\n' {
        return Some(next_idx);
    }
    if !matches!(ch, '.' | '!' | '?') {
        return None;
    }
    boundary_after_punct(&prefix[next_idx..], next_idx)
}

fn boundary_after_punct(after: &str, next_idx: usize) -> Option<usize> {
    if after.starts_with(char::is_whitespace) {
        return Some(next_idx);
    }
    let mut chars = after.chars();
    let quote = chars.next()?;
    let valid_quote = matches!(quote, '"' | '\'' | ')');
    (valid_quote && chars.next().is_some_and(char::is_whitespace))
        .then_some(next_idx + quote.len_utf8())
}

fn map_empty_reply_error(err: crate::domain::DomainValidationError) -> SessionError {
    SessionError::new(
        SessionErrorKind::AiInferenceFailed,
        "The AI tutor returned an empty response. Please try speaking again.",
        true,
        Some(err.to_string()),
    )
}
