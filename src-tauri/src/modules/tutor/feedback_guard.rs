//! Guardrails normalizing LLM structured feedback and suppressing contradictions/no-ops.

use crate::domain::{sanitize_untrusted_text, CorrectionMode};

/// Normalized coaching feedback derived from raw LLM output.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct StructuredFeedback {
    pub proper_way: Option<String>,
    pub why: Option<String>,
    pub legacy_summary: Option<String>,
}

/// Cleans placeholder values (`""`, `"NONE"`, `"null"`, `"N/A"`) after sanitization.
pub(crate) fn clean_non_placeholder(value: Option<String>) -> Option<String> {
    value.map(|s| sanitize_untrusted_text(&s)).filter(|s| {
        !s.is_empty()
            && !s.eq_ignore_ascii_case("null")
            && !s.eq_ignore_ascii_case("none")
            && !s.eq_ignore_ascii_case("n/a")
    })
}

/// Applies mode policy, `needsImprovement` authority, and identical-rewrite suppression.
pub(crate) fn resolve_structured_feedback(
    needs_improvement: Option<bool>,
    proper_way: Option<String>,
    why: Option<String>,
    legacy_correction: Option<String>,
    mode: CorrectionMode,
    learner_utterance: Option<&str>,
) -> StructuredFeedback {
    if mode == CorrectionMode::Off || needs_improvement == Some(false) {
        return StructuredFeedback::default();
    }
    let clean_pw = clean_non_placeholder(proper_way);
    let clean_why = clean_non_placeholder(why);
    let clean_legacy = clean_non_placeholder(legacy_correction);
    if let Some(pw) = clean_pw {
        return build_if_not_noop(pw, clean_why, learner_utterance);
    }
    if needs_improvement == Some(true) && clean_legacy.is_none() {
        return StructuredFeedback::default();
    }
    StructuredFeedback {
        proper_way: None,
        why: None,
        legacy_summary: clean_legacy,
    }
}

fn build_if_not_noop(
    proper_way: String,
    why: Option<String>,
    learner_utterance: Option<&str>,
) -> StructuredFeedback {
    if is_identical_rewrite(&proper_way, learner_utterance) {
        return StructuredFeedback::default();
    }
    let summary = format_feedback_summary(&proper_way, why.as_deref());
    StructuredFeedback {
        proper_way: Some(proper_way),
        why,
        legacy_summary: Some(summary),
    }
}

fn format_feedback_summary(proper_way: &str, why: Option<&str>) -> String {
    match why {
        Some(reason) => format!("{proper_way} — {reason}"),
        None => proper_way.to_string(),
    }
}

fn is_identical_rewrite(proper_way: &str, learner_utterance: Option<&str>) -> bool {
    let Some(raw) = learner_utterance else {
        return false;
    };
    let lhs = normalize_for_compare(proper_way);
    let rhs = normalize_for_compare(raw);
    !lhs.is_empty() && lhs == rhs
}

fn normalize_for_compare(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut prev_space = true;
    for ch in text.chars() {
        if ch.is_alphanumeric() {
            for lower in ch.to_lowercase() {
                out.push(lower);
            }
            prev_space = false;
        } else if (ch.is_whitespace() || matches!(ch, '-' | '/' | '\'' | '’')) && !prev_space {
            out.push(' ');
            prev_space = true;
        }
    }
    out.trim().to_string()
}
