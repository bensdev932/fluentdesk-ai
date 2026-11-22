//! Speech-safe text normalization and abbreviation-aware sentence boundary segmentation.

const ABBREVIATIONS: &[&str] = &[
    "mr.", "mrs.", "ms.", "dr.", "prof.", "sr.", "jr.", "st.", "no.", "vs.", "etc.", "e.g.",
    "i.e.", "u.s.", "u.k.", "a.m.", "p.m.",
];

/// Strips markdown tokens, bracketed/asterisk stage directions, and emojis for natural TTS.
pub fn normalize_for_speech(raw: &str) -> String {
    let without_stages = strip_bracketed_stages(raw);
    let mut out = String::with_capacity(without_stages.len());
    for line in without_stages.lines() {
        append_clean_line(line, &mut out);
    }
    collapse_whitespace(&out)
}

/// Returns `true` when `buf` ends at a real spoken sentence boundary (not an abbreviation).
pub fn ends_with_sentence_boundary(buf: &str) -> bool {
    let trimmed = buf.trim_end();
    if trimmed.ends_with('!') || trimmed.ends_with('?') {
        return true;
    }
    if !trimmed.ends_with('.') || trimmed.ends_with("..") {
        return false;
    }
    !ends_with_abbreviation(trimmed)
}

/// Normalizes text for speech and splits it into complete spoken sentences.
pub fn split_spoken_sentences(text: &str) -> Vec<String> {
    let clean = normalize_for_speech(text);
    let (mut out, mut buf) = (Vec::new(), String::new());
    for ch in clean.chars() {
        if ch.is_whitespace() && ends_with_sentence_boundary(&buf) {
            flush_sentence(&mut buf, &mut out);
        } else {
            buf.push(ch);
        }
    }
    flush_sentence(&mut buf, &mut out);
    out
}

fn ends_with_abbreviation(trimmed: &str) -> bool {
    let tail = trimmed
        .split_whitespace()
        .next_back()
        .unwrap_or("")
        .trim_matches(|c: char| matches!(c, '"' | '\'' | '(' | ')'));
    let lower = tail.to_ascii_lowercase();
    ABBREVIATIONS.iter().any(|abbr| *abbr == lower)
}

fn flush_sentence(buf: &mut String, out: &mut Vec<String>) {
    let trimmed = buf.trim();
    if !trimmed.is_empty() {
        out.push(trimmed.to_string());
    }
    buf.clear();
}

fn strip_bracketed_stages(raw: &str) -> String {
    let (mut out, mut close_ch) = (String::with_capacity(raw.len()), None::<char>);
    for ch in raw.chars() {
        if let Some(end) = close_ch {
            if ch == end {
                close_ch = None;
            }
        } else if ch == '[' {
            close_ch = Some(']');
        } else {
            out.push(ch);
        }
    }
    out
}

fn append_clean_line(line: &str, out: &mut String) {
    let trimmed = line.trim_start();
    let line = trimmed
        .trim_start_matches('#')
        .trim_start_matches([' ', '-', '*']);
    if !out.is_empty() && !line.is_empty() {
        out.push(' ');
    }
    for ch in line.chars().filter(|c| !is_speech_noise_char(*c)) {
        out.push(ch);
    }
}

fn is_speech_noise_char(ch: char) -> bool {
    matches!(ch, '*' | '`' | '_')
        || matches!(ch as u32, 0x1F300..=0x1FAFF | 0x2600..=0x27BF | 0xFE0F)
}

fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::{normalize_for_speech, split_spoken_sentences};

    #[test]
    fn normalizes_markdown_stages_and_emojis() {
        let raw = "### **Great job!** [smiles] *nods* Try saying `can I` today! 😊👍";
        assert_eq!(
            normalize_for_speech(raw),
            "Great job! nods Try saying can I today!"
        );
    }

    #[test]
    fn preserves_abbreviations_when_splitting_sentences() {
        let raw = "Dr. Smith arrived at 9 a.m. in St. Louis. Did you meet him?";
        assert_eq!(
            split_spoken_sentences(raw),
            vec![
                "Dr. Smith arrived at 9 a.m. in St. Louis.",
                "Did you meet him?",
            ]
        );
    }
}
