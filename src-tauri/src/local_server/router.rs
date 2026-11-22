//! Axum `Router` wiring REST, SSE streaming, and inbound WebSocket endpoints.

use crate::ai::router::TutorRouter;
use crate::local_server::dto::error_response;
use crate::local_server::handlers::{
    handle_get_preferences, handle_health, handle_list_providers, handle_list_scenarios,
    handle_patch_preferences, handle_tutor_reply, handle_update_providers,
};
use crate::local_server::http::{apply_bridge_middleware, MAX_BRIDGE_BODY_BYTES};
use crate::local_server::review_handlers::{
    handle_clear_learner_facts, handle_delete_learner_fact, handle_get_learner_progress,
    handle_get_session_review, handle_list_learner_facts,
};
use crate::local_server::session_handlers::{
    handle_get_session, handle_lifecycle, handle_list_sessions, handle_session_turn,
    handle_start_session, LifecycleOp,
};
use crate::local_server::session_stream::handle_turn_sse_stream;
use crate::local_server::ws_handler::handle_ws_upgrade;
use crate::modules::conversation::SpeakingSessionHost;
use crate::preferences::LearnerPreferencesStore;
use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Path, State};
use axum::http::{header::ACCEPT, HeaderMap};
use axum::middleware::from_fn;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::Router;
use std::sync::Arc;

#[derive(Clone)]
pub(super) struct BridgeState {
    pub router: Arc<TutorRouter>,
    pub host: Arc<SpeakingSessionHost>,
    pub prefs: Arc<LearnerPreferencesStore>,
}

pub(super) fn build_bridge_router(state: BridgeState) -> Router {
    Router::new()
        .route("/api/health", get(ep_health))
        .route("/api/preferences", get(ep_get_prefs).patch(ep_patch_prefs))
        .route("/api/providers", get(ep_list_providers))
        .route("/api/providers/config", post(ep_update_providers))
        .route("/api/scenarios", get(ep_list_scenarios))
        .route("/api/tutor/reply", post(ep_tutor_reply))
        .route(
            "/api/sessions",
            post(ep_start_session).get(ep_list_sessions),
        )
        .route("/api/sessions/{id}", get(ep_get_session))
        .route("/api/sessions/{id}/review", get(ep_session_review))
        .route("/api/learner/progress", get(ep_learner_progress))
        .route(
            "/api/learner/facts",
            get(ep_list_facts).delete(ep_clear_facts),
        )
        .route("/api/learner/facts/{id}", delete(ep_delete_fact))
        .route("/api/sessions/{id}/turn", post(ep_session_turn))
        .route(
            "/api/sessions/{id}/turn/stream",
            post(ep_session_turn_stream),
        )
        .route("/api/sessions/{id}/pause", post(ep_pause_session))
        .route("/api/sessions/{id}/resume", post(ep_resume_session))
        .route("/api/sessions/{id}/stop", post(ep_stop_session))
        .route("/api/sessions/{id}/status", get(ep_status_session))
        .route("/api/sessions/{id}/ws", get(handle_ws_upgrade))
        .fallback(ep_not_found)
        .method_not_allowed_fallback(ep_not_found)
        .layer(DefaultBodyLimit::max(MAX_BRIDGE_BODY_BYTES))
        .layer(from_fn(apply_bridge_middleware))
        .with_state(state)
}

async fn ep_health(State(s): State<BridgeState>) -> Response {
    handle_health(&s.router).await.into_response()
}
async fn ep_get_prefs(State(s): State<BridgeState>) -> Response {
    handle_get_preferences(&s.prefs).await.into_response()
}
async fn ep_patch_prefs(State(s): State<BridgeState>, body: Bytes) -> Response {
    handle_patch_preferences(&body, &s.router, &s.host, &s.prefs)
        .await
        .into_response()
}
async fn ep_list_providers(State(s): State<BridgeState>) -> Response {
    handle_list_providers(&s.router).await.into_response()
}
async fn ep_list_scenarios() -> Response {
    handle_list_scenarios().await.into_response()
}
async fn ep_update_providers(State(s): State<BridgeState>, body: Bytes) -> Response {
    handle_update_providers(&body, &s.router, &s.prefs)
        .await
        .into_response()
}
async fn ep_tutor_reply(State(s): State<BridgeState>, body: Bytes) -> Response {
    handle_tutor_reply(&body, &s.router).await.into_response()
}
async fn ep_start_session(State(s): State<BridgeState>, body: Bytes) -> Response {
    handle_start_session(&body, &s.host).await.into_response()
}
async fn ep_list_sessions(State(s): State<BridgeState>) -> Response {
    handle_list_sessions(&s.host).await.into_response()
}
async fn ep_get_session(State(s): State<BridgeState>, Path(id): Path<String>) -> Response {
    handle_get_session(&id, &s.host).await.into_response()
}
async fn ep_session_review(State(s): State<BridgeState>, Path(id): Path<String>) -> Response {
    handle_get_session_review(&id, &s.host)
        .await
        .into_response()
}
async fn ep_list_facts(State(s): State<BridgeState>) -> Response {
    handle_list_learner_facts(&s.host).await.into_response()
}
async fn ep_delete_fact(State(s): State<BridgeState>, Path(id): Path<String>) -> Response {
    handle_delete_learner_fact(&id, &s.host)
        .await
        .into_response()
}
async fn ep_clear_facts(State(s): State<BridgeState>) -> Response {
    handle_clear_learner_facts(&s.host).await.into_response()
}
async fn ep_learner_progress(State(s): State<BridgeState>) -> Response {
    handle_get_learner_progress(&s.host).await.into_response()
}
async fn ep_session_turn(
    State(s): State<BridgeState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if accepts_sse(&headers) {
        return handle_turn_sse_stream(&id, &body, s.host).await;
    }
    handle_session_turn(&id, &body, &s.host)
        .await
        .into_response()
}
async fn ep_session_turn_stream(
    State(s): State<BridgeState>,
    Path(id): Path<String>,
    body: Bytes,
) -> Response {
    handle_turn_sse_stream(&id, &body, s.host).await
}
async fn ep_pause_session(State(s): State<BridgeState>, Path(id): Path<String>) -> Response {
    handle_lifecycle(&id, &s.host, LifecycleOp::Pause)
        .await
        .into_response()
}
async fn ep_resume_session(State(s): State<BridgeState>, Path(id): Path<String>) -> Response {
    handle_lifecycle(&id, &s.host, LifecycleOp::Resume)
        .await
        .into_response()
}
async fn ep_stop_session(State(s): State<BridgeState>, Path(id): Path<String>) -> Response {
    handle_lifecycle(&id, &s.host, LifecycleOp::Stop)
        .await
        .into_response()
}
async fn ep_status_session(State(s): State<BridgeState>, Path(id): Path<String>) -> Response {
    handle_lifecycle(&id, &s.host, LifecycleOp::Status)
        .await
        .into_response()
}
async fn ep_not_found() -> Response {
    error_response(404, "Not Found", "Endpoint not found.").into_response()
}

fn accepts_sse(headers: &HeaderMap) -> bool {
    headers
        .get(ACCEPT)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|s| s.to_ascii_lowercase().contains("text/event-stream"))
}
