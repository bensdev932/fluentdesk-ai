//! Incremental JSON/plain-text sentence extractor for streaming LLM tutor replies.

use crate::modules::speech::spoken_text::{
    ends_with_sentence_boundary, normalize_for_speech, split_spoken_sentences,
};
use crate::modules::tutor::stream_parser_json::{
    decode_spoken_char, step_json_key_str, step_json_unquoted, DecodedChar, JsonScanState,
    UnescapeState,
};

/// Incremental parser extracting complete spoken sentences from streaming LLM tokens.
#[derive(Debug)]
pub struct SpokenJsonStreamParser {
    raw: String,
    mode: ParseMode,
    sentence_buf: String,
    fence_buf: String,
}

#[derive(Debug)]
enum ParseMode {
    Detect,
    SkipThink(String),
    Json(JsonScanState),
    InSpokenString(UnescapeState),
    PlainText,
    Done,
}

impl Default for SpokenJsonStreamParser {
    fn default() -> Self {
        Self::new()
    }
}

impl SpokenJsonStreamParser {
    pub fn new() -> Self {
        Self {
            raw: String::new(),
            mode: ParseMode::Detect,
            sentence_buf: String::new(),
            fence_buf: String::new(),
        }
    }

    pub fn push_token(&mut self, token: &str) -> Vec<String> {
        self.raw.push_str(token);
        let mut emitted = Vec::new();
        for ch in token.chars() {
            self.step_char(ch, &mut emitted);
        }
        emitted
    }

    pub fn finish(mut self) -> (Vec<String>, String) {
        let mut emitted = Vec::new();
        if !matches!(self.mode, ParseMode::Done | ParseMode::Json(_)) {
            self.flush_remaining(&mut emitted);
        }
        (emitted, self.raw)
    }

    fn step_char(&mut self, ch: char, out: &mut Vec<String>) {
        match &mut self.mode {
            ParseMode::Detect => self.step_detect(ch, out),
            ParseMode::SkipThink(_) => self.step_skip_think(ch),
            ParseMode::Json(_) => self.step_json(ch),
            ParseMode::InSpokenString(_) => self.step_spoken(ch, out),
            ParseMode::PlainText => self.push_spoken_char(ch, out),
            ParseMode::Done => {}
        }
    }

    fn step_detect(&mut self, ch: char, out: &mut Vec<String>) {
        if ch.is_whitespace() && self.fence_buf.is_empty() {
            return;
        }
        self.fence_buf.push(ch);
        if self.fence_buf.eq_ignore_ascii_case("<think>") {
            self.fence_buf.clear();
            self.mode = ParseMode::SkipThink(String::new());
            return;
        }
        if self.fence_buf.starts_with('<')
            && "<think>".starts_with(&self.fence_buf.to_ascii_lowercase())
        {
            return;
        }
        if self.fence_buf.starts_with('`') {
            return self.handle_fence_prefix(ch);
        }
        self.resolve_detected_prefix(ch, out);
    }

    fn handle_fence_prefix(&mut self, ch: char) {
        if ch == '\n' {
            self.fence_buf.clear();
        }
    }

    fn resolve_detected_prefix(&mut self, ch: char, out: &mut Vec<String>) {
        if ch == '{' {
            self.fence_buf.clear();
            self.mode = ParseMode::Json(JsonScanState {
                depth: 1,
                ..Default::default()
            });
            return;
        }
        let replay = std::mem::take(&mut self.fence_buf);
        self.mode = ParseMode::PlainText;
        for c in replay.chars() {
            self.push_spoken_char(c, out);
        }
    }

    fn step_skip_think(&mut self, ch: char) {
        if let ParseMode::SkipThink(buf) = &mut self.mode {
            buf.push(ch);
            if buf.to_ascii_lowercase().ends_with("</think>") {
                self.mode = ParseMode::Detect;
            }
        }
    }

    fn step_json(&mut self, ch: char) {
        let ParseMode::Json(st) = &mut self.mode else {
            return;
        };
        if st.in_str {
            return step_json_key_str(st, ch);
        }
        if ch == '"' && st.awaiting_value_quote {
            self.mode = ParseMode::InSpokenString(UnescapeState::default());
            return;
        }
        step_json_unquoted(st, ch);
    }

    fn step_spoken(&mut self, ch: char, out: &mut Vec<String>) {
        let finished = match &mut self.mode {
            ParseMode::InSpokenString(st) => decode_spoken_char(st, ch),
            _ => return,
        };
        match finished {
            DecodedChar::Char(c) => self.push_spoken_char(c, out),
            DecodedChar::EndOfString => {
                self.flush_remaining(out);
                self.mode = ParseMode::Done;
            }
            DecodedChar::Pending => {}
        }
    }

    fn push_spoken_char(&mut self, ch: char, out: &mut Vec<String>) {
        if ch.is_whitespace() && ends_with_sentence_punct(&self.sentence_buf) {
            self.flush_remaining(out);
            return;
        }
        self.sentence_buf.push(ch);
    }

    fn flush_remaining(&mut self, out: &mut Vec<String>) {
        let cleaned = normalize_for_speech(&self.sentence_buf);
        if !cleaned.is_empty() {
            out.push(cleaned);
        }
        self.sentence_buf.clear();
    }
}

fn ends_with_sentence_punct(buf: &str) -> bool {
    ends_with_sentence_boundary(buf)
}

/// Splits non-streaming reply text into complete spoken sentences using the same boundary rules.
pub fn split_sentences(text: &str) -> Vec<String> {
    split_spoken_sentences(text)
}
