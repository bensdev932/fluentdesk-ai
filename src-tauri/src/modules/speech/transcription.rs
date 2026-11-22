//! Utterance transcription and Whisper hallucination filtering (`T023`, `T026`).

use crate::domain::{SessionError, Transcript};
use crate::modules::speech::utterance::CompletedUtterance;
use crate::stt::provider::{SpeechToText, SttPromptContext};
use serde::{Deserialize, Serialize};

const WHISPER_NOISE_MARKERS: &[&str] = &[
    "[silence]",
    "[music]",
    "[applause]",
    "[laughter]",
    "[cough]",
    "[throat clearing]",
    "[blank_audio]",
    "thank you.",
    "thanks for watching!",
    "you",
    ".",
    "...",
];

/// Event emitted when a learner's speech transcript is ready (`T026`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserTranscriptReadyEvent {
    pub transcript: Transcript,
    pub is_empty_or_hallucinated: bool,
}

/// Transcribes a normalized 16 kHz mono utterance directly via `SpeechToText` and flags noise (`T023`, `T026`).
pub async fn transcribe_completed_utterance(
    stt: &dyn SpeechToText,
    utterance: &CompletedUtterance,
) -> Result<UserTranscriptReadyEvent, SessionError> {
    transcribe_completed_utterance_with_context(stt, utterance, None).await
}

/// Transcribes with optional vocabulary/topic biasing context (`--prompt`).
pub async fn transcribe_completed_utterance_with_context(
    stt: &dyn SpeechToText,
    utterance: &CompletedUtterance,
    context: Option<&SttPromptContext>,
) -> Result<UserTranscriptReadyEvent, SessionError> {
    let transcript = stt.transcribe_with_context(utterance, context).await?;
    let is_empty_or_hallucinated = is_noise_or_hallucination(&transcript.text)
        || context.is_some_and(|ctx| is_prompt_echo(&transcript.text, ctx));
    Ok(UserTranscriptReadyEvent {
        transcript,
        is_empty_or_hallucinated,
    })
}

/// Checks if transcript consists only of punctuation, stage noise, Whisper hallucinations, or 3+ repetition loops.
pub fn is_noise_or_hallucination(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() || !has_alphabetic_char(trimmed) || is_stage_marker(trimmed) {
        return true;
    }
    let lower = trimmed.to_ascii_lowercase();
    is_known_whisper_artifact(&lower) || has_repeated_ngram_loop(&tokenize_words(&lower))
}

/// Detects when Whisper echoes its topic title or prompt scaffolding instructions.
pub fn is_prompt_echo(transcript: &str, ctx: &SttPromptContext) -> bool {
    let tx_words = tokenize_words(&transcript.to_ascii_lowercase());
    if tx_words.len() < 2 {
        return false;
    }
    let exact_topic = ctx
        .topic()
        .is_some_and(|t| tokenize_words(&t.to_ascii_lowercase()) == tx_words);
    exact_topic
        || ctx
            .format_whisper_prompt()
            .is_some_and(|p| is_scaffold_echo(&tx_words, &p))
}

fn is_scaffold_echo(tx_words: &[String], prompt: &str) -> bool {
    let pw = tokenize_words(&prompt.to_ascii_lowercase());
    let has_header = matches!(
        tx_words.first().map(String::as_str),
        Some("topic" | "vocabulary" | "verbatim")
    );
    let has_verbatim = tx_words.windows(2).any(|w| w == ["verbatim", "speech"]);
    contains_subslice(&pw, tx_words) && (has_header || has_verbatim)
}

fn has_alphabetic_char(text: &str) -> bool {
    text.chars().any(char::is_alphabetic)
}

fn is_stage_marker(trimmed: &str) -> bool {
    trimmed.len() <= 40
        && ((trimmed.starts_with('[') && trimmed.ends_with(']'))
            || (trimmed.starts_with('(') && trimmed.ends_with(')'))
            || (trimmed.starts_with('*') && trimmed.ends_with('*')))
}

fn is_known_whisper_artifact(lower_text: &str) -> bool {
    WHISPER_NOISE_MARKERS.contains(&lower_text)
}

fn tokenize_words(lower: &str) -> Vec<String> {
    lower
        .split(|c: char| !c.is_alphanumeric() && c != '\'')
        .map(|w| w.trim_matches('\''))
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect()
}

fn has_repeated_ngram_loop(tokens: &[String]) -> bool {
    (1..=3).any(|n| tokens.len() >= n * 3 && has_triple_repeat(tokens, n))
}

fn has_triple_repeat(tokens: &[String], n: usize) -> bool {
    tokens.windows(n * 3).any(|w| {
        let (a, b, c) = (&w[..n], &w[n..n * 2], &w[n * 2..n * 3]);
        a == b && b == c
    })
}

fn contains_subslice(haystack: &[String], needle: &[String]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}
