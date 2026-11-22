//! Domain vocabulary and topic context for Whisper `--prompt` biasing (`voice-research-upgrades` #03).

use crate::domain::{sanitize_untrusted_text, ConversationRole, ConversationTurn};
use std::collections::HashSet;

const MAX_WHISPER_PROMPT_CHARS: usize = 224;
const MAX_KEYTERMS: usize = 16;
const MAX_TERM_CHARS: usize = 48;
const STOP_WORDS: &[&str] = &[
    "about", "after", "again", "been", "could", "doing", "every", "from", "good", "great", "have",
    "hello", "here", "into", "just", "like", "more", "much", "nice", "only", "please", "really",
    "should", "some", "thank", "thanks", "that", "their", "them", "then", "there", "they", "this",
    "today", "very", "want", "well", "were", "what", "when", "where", "which", "while", "will",
    "with", "would", "your", "yourself",
];

/// Bounded, sanitized topic and keyterm hints passed to Whisper's `--prompt` decoder seed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SttPromptContext {
    topic: Option<String>,
    keyterms: Vec<String>,
}

impl SttPromptContext {
    pub fn new(topic: Option<&str>, keyterms: impl IntoIterator<Item = String>) -> Self {
        Self {
            topic: topic.and_then(clean_topic),
            keyterms: dedup_keyterms(keyterms),
        }
    }

    pub fn from_session(
        topic: &str,
        target_vocabulary: &[String],
        recent_turns: &[ConversationTurn],
    ) -> Self {
        let mut terms: Vec<String> = target_vocabulary.to_vec();
        terms.extend(extract_recent_tutor_keywords(recent_turns));
        Self::new(Some(topic), terms)
    }

    pub fn topic(&self) -> Option<&str> {
        self.topic.as_deref()
    }

    pub fn keyterms(&self) -> &[String] {
        &self.keyterms
    }

    pub fn is_empty(&self) -> bool {
        self.topic.is_none() && self.keyterms.is_empty()
    }

    pub fn format_whisper_prompt(&self) -> Option<String> {
        let mut out = String::new();
        if let Some(topic) = &self.topic {
            out.push_str(&format!("Topic: {topic}."));
        }
        append_bounded_terms(&mut out, &self.keyterms);
        append_verbatim_hint(&mut out);
        let bounded: String = out.trim().chars().take(MAX_WHISPER_PROMPT_CHARS).collect();
        (!bounded.is_empty()).then_some(bounded)
    }
}

fn clean_single_line(raw: &str, max_chars: usize) -> Option<String> {
    let cleaned = sanitize_untrusted_text(raw);
    let collapsed = cleaned
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .filter(|c| !c.is_control())
        .take(max_chars)
        .collect::<String>();
    let trimmed = collapsed.trim().to_string();
    (!trimmed.is_empty()).then_some(trimmed)
}

fn clean_topic(raw: &str) -> Option<String> {
    clean_single_line(raw, 80)
}

fn dedup_keyterms(raw_terms: impl IntoIterator<Item = String>) -> Vec<String> {
    let (mut out, mut seen) = (Vec::new(), HashSet::new());
    for raw in raw_terms {
        if out.len() >= MAX_KEYTERMS {
            break;
        }
        if let Some(term) = clean_single_line(&raw, MAX_TERM_CHARS) {
            if seen.insert(term.to_ascii_lowercase()) {
                out.push(term);
            }
        }
    }
    out
}

fn extract_recent_tutor_keywords(recent_turns: &[ConversationTurn]) -> Vec<String> {
    recent_turns
        .iter()
        .rev()
        .filter(|t| t.role == ConversationRole::Tutor)
        .take(2)
        .flat_map(collect_turn_keywords)
        .collect()
}

fn collect_turn_keywords(turn: &ConversationTurn) -> Vec<String> {
    let combined = match &turn.follow_up_prompt {
        Some(f) => format!("{f} {}", turn.text),
        None => turn.text.clone(),
    };
    combined
        .split(|c: char| !c.is_alphanumeric() && c != '\'' && c != '-')
        .map(|w| w.trim_matches(|c: char| !c.is_alphabetic()))
        .filter(|w| {
            w.chars().count() >= 4 && !STOP_WORDS.contains(&w.to_ascii_lowercase().as_str())
        })
        .map(str::to_string)
        .collect()
}

fn append_bounded_terms(out: &mut String, terms: &[String]) {
    let Some(first) = terms.first() else {
        return;
    };
    let header = if out.is_empty() {
        "Vocabulary: "
    } else {
        " Vocabulary: "
    };
    if !fits_prompt(out, header.len() + first.chars().count()) {
        return;
    }
    out.push_str(header);
    for (idx, term) in terms.iter().enumerate() {
        let sep = if idx == 0 { "" } else { ", " };
        if !fits_prompt(out, sep.len() + term.chars().count()) {
            break;
        }
        out.push_str(sep);
        out.push_str(term);
    }
}

fn fits_prompt(current: &str, added_chars: usize) -> bool {
    current.chars().count() + added_chars <= MAX_WHISPER_PROMPT_CHARS
}

fn append_verbatim_hint(out: &mut String) {
    const HINT: &str = " Verbatim speech, including um, uh.";
    if !out.is_empty() && fits_prompt(out, HINT.len()) {
        out.push_str(HINT);
    }
}
