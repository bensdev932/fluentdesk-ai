//! HTTP JSON DTOs and bounded history conversion for the local FluentDesk AI bridge.

use crate::ai::{
    catalog::{ProviderId, ProviderPreference},
    router_types::RoutedTutorReply,
};
use crate::domain::{
    sanitize_untrusted_text, ConversationRole, ConversationTurn, CorrectionMode, SessionId,
    TurnStageMetadata,
};
use crate::local_server::http::HttpResponse;
use crate::modules::conversation::context::{BoundedConversationContext, MaxContextTurns};
use crate::telemetry::redact_sensitive_log_text;
use serde::{Deserialize, Serialize};

const MAX_HISTORY_TURNS: usize = 50;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TutorReplyHttpRequest {
    #[serde(default)]
    pub history: Vec<HttpHistoryTurn>,
    #[serde(default)]
    pub user_text: String,
    #[serde(default)]
    pub correction_mode: CorrectionMode,
    #[serde(default)]
    pub provider: Option<ProviderPreference>,
    #[serde(default)]
    pub model: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpHistoryTurn {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub session_id: Option<String>,
    pub role: ConversationRole,
    pub text: String,
    #[serde(default)]
    pub correction: Option<String>,
    #[serde(default)]
    pub follow_up_prompt: Option<String>,
    #[serde(default)]
    pub timestamp_unix_ms: i64,
    #[serde(default)]
    pub stage_metadata: Option<TurnStageMetadata>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TutorReplyHttpResponse {
    pub spoken_reply: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub correction: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub follow_up_prompt: Option<String>,
    pub provider: ProviderId,
    pub model: String,
    pub is_cloud: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failover_note: Option<String>,
}

impl TutorReplyHttpResponse {
    /// Builds a sanitized HTTP reply DTO from a `TutorRouter` result.
    pub fn from_routed(r: RoutedTutorReply) -> Self {
        Self {
            spoken_reply: r.reply.spoken_reply,
            correction: r.reply.correction,
            follow_up_prompt: r.reply.follow_up_prompt,
            provider: r.provider,
            model: r.model,
            is_cloud: r.is_cloud,
            failover_note: r.failover_note.as_deref().map(redact_sensitive_log_text),
        }
    }
}

#[derive(Serialize)]
pub struct ErrorPayload {
    pub error: String,
}

/// Converts untrusted JSON history items into bounded chronological `ConversationTurn` records.
pub fn convert_bounded_history(items: Vec<HttpHistoryTurn>) -> Vec<ConversationTurn> {
    let limit = MaxContextTurns::try_new(MAX_HISTORY_TURNS).expect("valid context bound");
    let mut ctx = BoundedConversationContext::new(limit);
    items
        .into_iter()
        .filter_map(map_history_turn)
        .for_each(|t| ctx.push_turn(t));
    ctx.recent_turns()
}

/// Serializes any JSON payload into an `HttpResponse` or returns an explicit `500` error.
pub fn json_response<T: Serialize>(status: u16, reason: &'static str, payload: &T) -> HttpResponse {
    match serde_json::to_string(payload) {
        Ok(body) => HttpResponse::json(status, reason, body),
        Err(_) => error_response(
            500,
            "Internal Server Error",
            "Failed to serialize response.",
        ),
    }
}

/// Builds an explicit JSON error response without emitting fake replies.
pub fn error_response(
    status: u16,
    reason: &'static str,
    message: impl Into<String>,
) -> HttpResponse {
    let payload = ErrorPayload {
        error: message.into(),
    };
    let json = serde_json::to_string(&payload)
        .unwrap_or_else(|_| "{\"error\":\"Internal error\"}".to_string());
    HttpResponse::json(status, reason, json)
}

fn map_history_turn(raw: HttpHistoryTurn) -> Option<ConversationTurn> {
    let text = sanitize_untrusted_text(&raw.text);
    if text.is_empty() {
        return None;
    }
    let sid_str = raw
        .session_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("http-session");
    let session_id = SessionId::new(sid_str).ok()?;
    Some(build_turn_record(raw, session_id, text))
}

fn build_turn_record(
    raw: HttpHistoryTurn,
    session_id: SessionId,
    text: String,
) -> ConversationTurn {
    let correction = raw
        .correction
        .map(|c| sanitize_untrusted_text(&c))
        .filter(|s| !s.is_empty());
    let follow_up_prompt = raw
        .follow_up_prompt
        .map(|f| sanitize_untrusted_text(&f))
        .filter(|s| !s.is_empty());
    ConversationTurn {
        id: raw.id,
        session_id,
        role: raw.role,
        text,
        correction,
        follow_up_prompt,
        timestamp_unix_ms: raw.timestamp_unix_ms,
        stage_metadata: raw.stage_metadata,
    }
}
