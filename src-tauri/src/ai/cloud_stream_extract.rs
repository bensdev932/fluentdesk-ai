//! SSE line framing, delta text extraction, and redacted SSE error parsing.

use crate::ai::gemini::extract_gemini_text;
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
    extract_gemini_text(payload).ok()
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

pub(crate) fn extract_sse_error(payload: &str) -> Option<(Option<String>, String)> {
    let v: Value = serde_json::from_str(payload).ok()?;
    let err = v.get("error")?;
    let code = err.get("code").and_then(|x| x.as_str()).map(str::to_string);
    Some((code, format_sse_error_msg(err)))
}

fn format_sse_error_msg(err: &Value) -> String {
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
