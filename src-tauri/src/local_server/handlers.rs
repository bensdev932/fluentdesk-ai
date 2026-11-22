//! Route handlers for health, learner preferences, provider configuration, and tutor turns.
//! Clean protocol adapter delegating routing, consent, and persistence to backend modules.

use crate::ai::{router::TutorRouter, router_types::RouterConfigPatch};
use crate::domain::{
    built_in_scenarios, LearnerPreferencesPatch, SessionError, SessionErrorKind, Transcript,
};
use crate::local_server::dto::{
    convert_bounded_history, error_response, json_response, TutorReplyHttpRequest,
    TutorReplyHttpResponse,
};
use crate::local_server::http::HttpResponse;
use crate::modules::conversation::SpeakingSessionHost;
use crate::preferences::LearnerPreferencesStore;
use crate::telemetry::redact_sensitive_log_text;

pub(super) async fn handle_health(router: &TutorRouter) -> HttpResponse {
    let mut health = router.overall_health().await;
    health.error = health.error.as_deref().map(redact_sensitive_log_text);
    if health.status == "available" {
        json_response(200, "OK", &health)
    } else {
        json_response(503, "Service Unavailable", &health)
    }
}

pub(super) async fn handle_get_preferences(prefs: &LearnerPreferencesStore) -> HttpResponse {
    match prefs.get_preferences().await {
        Ok(current) => json_response(200, "OK", &current),
        Err(err) => map_preferences_error(&err),
    }
}

pub(super) async fn handle_patch_preferences(
    body: &[u8],
    router: &TutorRouter,
    host: &SpeakingSessionHost,
    prefs: &LearnerPreferencesStore,
) -> HttpResponse {
    let Ok(patch) = serde_json::from_slice::<LearnerPreferencesPatch>(body) else {
        return error_response(400, "Bad Request", "Invalid learner preferences payload.");
    };
    let updated = match prefs.patch_preferences(patch).await {
        Ok(p) => p,
        Err(err) => return map_preferences_error(&err),
    };
    router.sync_preferences(&updated);
    host.apply_preferences_to_active(&updated).await;
    json_response(200, "OK", &updated)
}

pub(super) async fn handle_list_providers(router: &TutorRouter) -> HttpResponse {
    let mut snapshot = router.providers_snapshot().await;
    for provider in &mut snapshot.providers {
        provider.status_detail = provider
            .status_detail
            .as_deref()
            .map(redact_sensitive_log_text);
    }
    json_response(200, "OK", &snapshot)
}

pub(super) async fn handle_update_providers(
    body: &[u8],
    router: &TutorRouter,
    prefs: &LearnerPreferencesStore,
) -> HttpResponse {
    let Ok(patch) = serde_json::from_slice::<RouterConfigPatch>(body) else {
        return error_response(
            400,
            "Bad Request",
            "Invalid provider configuration payload.",
        );
    };
    let prefs_patch = patch.to_preferences_patch();
    if let Err(err) = router.apply_config_patch(patch).await {
        return map_preferences_error(&err);
    }
    if let Err(err) = prefs.patch_preferences(prefs_patch).await {
        return map_preferences_error(&err);
    }
    handle_list_providers(router).await
}

pub(super) async fn handle_list_scenarios() -> HttpResponse {
    json_response(200, "OK", &built_in_scenarios())
}

pub(super) async fn handle_tutor_reply(body: &[u8], router: &TutorRouter) -> HttpResponse {
    let Ok(req) = serde_json::from_slice::<TutorReplyHttpRequest>(body) else {
        return error_response(400, "Bad Request", "Invalid JSON request body.");
    };
    let Ok(transcript) = parse_user_transcript(&req.user_text) else {
        return error_response(400, "Bad Request", "userText must not be empty.");
    };
    dispatch_tutor_turn(req, &transcript, router).await
}

async fn dispatch_tutor_turn(
    req: TutorReplyHttpRequest,
    tx: &Transcript,
    router: &TutorRouter,
) -> HttpResponse {
    let hist = convert_bounded_history(req.history);
    let model = req.model.as_deref();
    match router
        .route_turn(&hist, tx, req.correction_mode, req.provider, model)
        .await
    {
        Ok(routed) => json_response(200, "OK", &TutorReplyHttpResponse::from_routed(routed)),
        Err(err) => map_session_error(&err),
    }
}

fn parse_user_transcript(raw: &str) -> Result<Transcript, ()> {
    let tx = Transcript::new(raw, "en", 0, None).map_err(|_| ())?;
    (!tx.is_empty_or_whitespace()).then_some(tx).ok_or(())
}

fn map_preferences_error(err: &SessionError) -> HttpResponse {
    let safe_msg = redact_sensitive_log_text(&err.user_message);
    match err.kind {
        SessionErrorKind::TtsFailed | SessionErrorKind::InvalidStateTransition => {
            error_response(400, "Bad Request", safe_msg)
        }
        _ => error_response(500, "Internal Server Error", safe_msg),
    }
}

fn map_session_error(err: &SessionError) -> HttpResponse {
    let safe_msg = redact_sensitive_log_text(&err.user_message);
    if err.kind == SessionErrorKind::AiProviderUnavailable {
        error_response(503, "Service Unavailable", safe_msg)
    } else {
        error_response(502, "Bad Gateway", safe_msg)
    }
}
