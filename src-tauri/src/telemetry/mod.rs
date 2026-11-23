//! Structured tracing initialization and redaction utilities (`T004`).
//!
//! Enforces Constitution Principle 3 & Plan Section 2:
//! Secrets and raw audio MUST never be written to logs.

use std::fmt;

/// Wrapper for sensitive values (API keys, bearer tokens, credentials) that
/// guarantees `Debug` and `Display` output `[REDACTED_SECRET]`.
#[derive(Clone, PartialEq, Eq)]
pub struct RedactedSecret(String);

impl RedactedSecret {
    pub fn new(secret: impl Into<String>) -> Self {
        Self(secret.into())
    }

    /// Explicitly exposes the inner secret only for authenticated adapter calls.
    pub fn expose_secret(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for RedactedSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[REDACTED_SECRET]")
    }
}

impl fmt::Display for RedactedSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[REDACTED_SECRET]")
    }
}

/// Redacts known secret patterns (`Bearer <token>`, `sk-`, `gsk_`, `AIza`, headers, and `?key=`).
pub fn redact_sensitive_log_text(input: &str) -> String {
    let words: Vec<&str> = input.split_whitespace().collect();
    let mut out = Vec::with_capacity(words.len());
    let mut i = 0;
    while i < words.len() {
        if is_header_secret_prefix(words[i]) && i + 1 < words.len() {
            out.push(format!("{} [REDACTED_SECRET]", words[i]));
            i += 2;
            continue;
        }
        out.push(scrub_token(words[i]));
        i += 1;
    }
    out.join(" ")
}

fn is_header_secret_prefix(word: &str) -> bool {
    let trimmed = word.trim_end_matches(':');
    trimmed.eq_ignore_ascii_case("bearer") || trimmed.eq_ignore_ascii_case("x-goog-api-key")
}

fn scrub_token(word: &str) -> String {
    let clean = word.trim_matches(|c: char| matches!(c, '"' | '\'' | ',' | ';'));
    if looks_like_raw_secret(clean) {
        return "[REDACTED_SECRET]".to_string();
    }
    if word.contains('=') {
        return scrub_kv_segments(word);
    }
    if let Some((k, _)) = word.split_once(':') {
        if is_secret_key(k) {
            return format!("{k}:[REDACTED_SECRET]");
        }
    }
    word.to_string()
}

fn scrub_kv_segments(word: &str) -> String {
    word.split('&')
        .map(scrub_single_kv)
        .collect::<Vec<_>>()
        .join("&")
}

fn scrub_single_kv(part: &str) -> String {
    if let Some((raw_k, _)) = part.split_once('=') {
        let key_tail = raw_k.rsplit('?').next().unwrap_or(raw_k);
        if is_secret_key(key_tail) {
            return format!("{raw_k}=[REDACTED_SECRET]");
        }
    }
    if looks_like_raw_secret(part) {
        "[REDACTED_SECRET]".to_string()
    } else {
        part.to_string()
    }
}

fn looks_like_raw_secret(val: &str) -> bool {
    (val.starts_with("sk-") && val.len() > 5)
        || (val.starts_with("gsk_") && val.len() > 6)
        || (val.starts_with("AIza") && val.len() > 8)
        || (val.starts_with("freellmapi-") && val.len() > 14)
}

fn is_secret_key(key: &str) -> bool {
    let k = key
        .trim_matches(|c: char| matches!(c, '"' | '\'' | ':' | '?'))
        .to_ascii_lowercase();
    k == "key"
        || k.contains("api_key")
        || k.contains("apikey")
        || k.contains("x-goog-api-key")
        || k.contains("secret")
        || k.contains("token")
        || k.contains("password")
}

/// Initializes default structured tracing subscriber if not already set.
pub fn init_default_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .try_init();
}

#[cfg(test)]
mod tests;
