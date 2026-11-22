//! SSE line framing, delta text extraction, and redacted SSE error parsing.

use crate::ai::gemini::extract_candidates_text;
use crate::telemetry::redact_sensitive_log_text;
use serde_json::Value;

pub(crate) fn strip_sse_data(line: &str) -> Option<&str> {
    line.trim().strip_prefix("data:").map(str::trim)
}

pub(crate) fn extract_openai_delta(payload: &str) -> Option<String> {
    let parsed: Value = serde_json::from_str(payload).ok()?;
    let choices = parsed.get("choices")?.as_array()?;
    choices.iter().find_map(extract_choice_text)
}

fn extract_choice_text(choice: &Value) -> Option<String> {
    choice
        .pointer("/delta/content")
        .or_else(|| choice.pointer("/message/content"))
        .and_then(|v| v.as_str())
        .map(str::to_string)
}

pub(crate) fn extract_gemini_delta(payload: &str) -> Option<String> {
    let parsed: Value = serde_json::from_str(payload).ok()?;
    extract_candidates_text(&parsed, false)
}

pub(crate) fn extract_openai_ws_delta(payload: &str) -> Option<String> {
    if let Some(text) = extract_openai_delta(payload) {
        return Some(text);
    }
    let val: Value = serde_json::from_str(payload).ok()?;
    let ty = val.get("type").and_then(|v| v.as_str()).unwrap_or("");
    matches!(
        ty,
        "response.text.delta" | "response.audio_transcript.delta"
    )
    .then(|| {
        val.get("delta")
            .and_then(|v| v.as_str())
            .map(str::to_string)
    })
    .flatten()
}

pub(crate) fn extract_gemini_ws_delta(payload: &str) -> Option<String> {
    if let Some(text) = extract_gemini_delta(payload) {
        return Some(text);
    }
    let val: Value = serde_json::from_str(payload).ok()?;
    let parts = val.pointer("/serverContent/modelTurn/parts")?.as_array()?;
    let text: String = parts
        .iter()
        .filter(|p| p.get("thought").and_then(|v| v.as_bool()) != Some(true))
        .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
        .collect();
    (!text.is_empty()).then_some(text)
}

pub(crate) fn is_ws_turn_complete(payload: &str) -> bool {
    let Ok(val) = serde_json::from_str::<Value>(payload) else {
        return false;
    };
    val.pointer("/serverContent/turnComplete")
        .and_then(|v| v.as_bool())
        == Some(true)
        || val.get("type").and_then(|v| v.as_str()) == Some("response.done")
}

#[derive(Debug, Clone)]
pub(crate) struct SsePayloadError {
    pub status: Option<u16>,
    pub code: Option<String>,
    pub message: String,
}

pub(crate) fn extract_sse_error(payload: &str) -> Option<SsePayloadError> {
    let v: Value = serde_json::from_str(payload).ok()?;
    let err = v
        .get("error")
        .filter(|e| !e.is_null() && e.as_bool() != Some(false))?;
    let code = err.get("code").and_then(|x| x.as_str()).map(str::to_string);
    let status = extract_sse_error_status(err, code.as_deref());
    Some(SsePayloadError {
        status,
        code,
        message: format_sse_error_msg(err),
    })
}

fn extract_sse_error_status(err: &Value, code: Option<&str>) -> Option<u16> {
    let numeric = err
        .get("code")
        .and_then(parse_http_status_value)
        .or_else(|| err.get("status").and_then(parse_http_status_value));
    if numeric.is_some() {
        return numeric;
    }
    let fields = [
        code.unwrap_or(""),
        err.get("status").and_then(|x| x.as_str()).unwrap_or(""),
        err.get("type").and_then(|x| x.as_str()).unwrap_or(""),
        err.as_str().unwrap_or(""),
    ];
    fields.into_iter().find_map(map_status_tag)
}

fn map_status_tag(raw: &str) -> Option<u16> {
    let lower = raw.trim().to_ascii_lowercase();
    if lower.is_empty() {
        return None;
    }
    if contains_any(&lower, &["rate_limit", "resource_exhausted", "too_many"]) {
        return Some(429);
    }
    if contains_any(
        &lower,
        &["unauthenticated", "authentication", "invalid_api"],
    ) {
        return Some(401);
    }
    if contains_any(&lower, &["permission_denied", "permission_error"]) {
        return Some(403);
    }
    if contains_any(
        &lower,
        &["unavailable", "overloaded", "server_error", "internal"],
    ) {
        return Some(503);
    }
    contains_any(&lower, &["deadline_exceeded", "timeout"]).then_some(504)
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|n| haystack.contains(n))
}

fn parse_http_status_value(val: &Value) -> Option<u16> {
    let n = val
        .as_u64()
        .or_else(|| val.as_str().and_then(|s| s.trim().parse::<u64>().ok()))?;
    (400..=599).contains(&n).then_some(n as u16)
}

fn format_sse_error_msg(err: &Value) -> String {
    if let Some(s) = err.as_str() {
        return redact_sensitive_log_text(s).chars().take(180).collect();
    }
    let tag = err.get("type").and_then(|x| x.as_str()).unwrap_or("");
    let msg = err
        .get("message")
        .and_then(|x| x.as_str())
        .unwrap_or("Provider error");
    let raw = if tag.is_empty() {
        msg.to_string()
    } else {
        format!("{tag}: {msg}")
    };
    redact_sensitive_log_text(&raw).chars().take(180).collect()
}
