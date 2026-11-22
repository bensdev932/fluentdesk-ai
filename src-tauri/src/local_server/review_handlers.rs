//! HTTP route handlers for post-session AI-estimate reviews and Learner Facts.

use crate::domain::SessionId;
use crate::local_server::dto::{error_response, json_response};
use crate::local_server::http::HttpResponse;
use crate::local_server::session_handlers::map_session_http_error;
use crate::modules::conversation::SpeakingSessionHost;
use serde_json::json;

pub(super) async fn handle_get_session_review(
    raw_id: &str,
    host: &SpeakingSessionHost,
) -> HttpResponse {
    let Ok(sid) = SessionId::new(raw_id) else {
        return error_response(400, "Bad Request", "Invalid session ID.");
    };
    match host.get_or_generate_session_review(&sid).await {
        Ok(Some(review)) => json_response(200, "OK", &review),
        Ok(None) => error_response(404, "Not Found", "Speaking session not found."),
        Err(err) => map_session_http_error(&err),
    }
}

pub(super) async fn handle_list_learner_facts(host: &SpeakingSessionHost) -> HttpResponse {
    match host.list_learner_facts().await {
        Ok(facts) => json_response(200, "OK", &facts),
        Err(err) => map_session_http_error(&err),
    }
}

pub(super) async fn handle_delete_learner_fact(
    fact_id: &str,
    host: &SpeakingSessionHost,
) -> HttpResponse {
    let clean = fact_id.trim();
    if clean.is_empty() {
        return error_response(400, "Bad Request", "Invalid learner fact ID.");
    }
    match host.delete_learner_fact(clean).await {
        Ok(true) => json_response(200, "OK", &json!({ "deleted": true, "id": clean })),
        Ok(false) => error_response(404, "Not Found", "Learner fact not found."),
        Err(err) => map_session_http_error(&err),
    }
}

pub(super) async fn handle_clear_learner_facts(host: &SpeakingSessionHost) -> HttpResponse {
    match host.clear_learner_facts().await {
        Ok(deleted) => json_response(200, "OK", &json!({ "deleted": deleted })),
        Err(err) => map_session_http_error(&err),
    }
}

pub(super) async fn handle_get_learner_progress(host: &SpeakingSessionHost) -> HttpResponse {
    match host.get_learner_progress().await {
        Ok(progress) => json_response(200, "OK", &progress),
        Err(err) => map_session_http_error(&err),
    }
}
