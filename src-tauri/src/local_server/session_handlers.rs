//! HTTP route handlers for `/api/sessions` delegating to `SpeakingSessionHost`.

use crate::domain::{CorrectionMode, SessionId, Transcript};
use crate::local_server::dto::{error_response, json_response};
use crate::local_server::http::HttpResponse;
pub(super) use crate::local_server::session_dto::map_session_http_error;
use crate::local_server::session_dto::{
    SessionTurnHttpRequest, SessionTurnHttpResponse, StartSessionHttpRequest,
};
use crate::modules::conversation::{SpeakingSessionHost, StartSpeakingSessionConfig};
use std::sync::Arc;

pub(super) enum LifecycleOp {
    Pause,
    Resume,
    Stop,
    Status,
}

pub(super) async fn handle_start_session(body: &[u8], host: &SpeakingSessionHost) -> HttpResponse {
    let cfg = match parse_start_config(body, host).await {
        Ok(cfg) => cfg,
        Err(resp) => return resp,
    };
    match host.start_session(cfg).await {
        Ok(snap) => json_response(200, "OK", &snap),
        Err(err) => map_session_http_error(&err),
    }
}

async fn parse_start_config(
    body: &[u8],
    host: &SpeakingSessionHost,
) -> Result<StartSpeakingSessionConfig, HttpResponse> {
    let req = if body.iter().all(|b| b.is_ascii_whitespace()) {
        StartSessionHttpRequest::default()
    } else {
        serde_json::from_slice::<StartSessionHttpRequest>(body).map_err(|_| {
            error_response(400, "Bad Request", "Invalid session configuration payload.")
        })?
    };
    let prefs = match host.preferences_store() {
        Some(store) => store
            .get_preferences()
            .await
            .map_err(|e| map_session_http_error(&e))?,
        None => Default::default(),
    };
    Ok(req.into_config(prefs))
}

pub(super) async fn handle_list_sessions(host: &SpeakingSessionHost) -> HttpResponse {
    match host.list_sessions().await {
        Ok(list) => json_response(200, "OK", &list),
        Err(err) => map_session_http_error(&err),
    }
}

pub(super) async fn handle_get_session(raw_id: &str, host: &SpeakingSessionHost) -> HttpResponse {
    let Ok(sid) = SessionId::new(raw_id) else {
        return error_response(400, "Bad Request", "Invalid session ID.");
    };
    match host.find_session_history(&sid).await {
        Ok(Some(view)) => json_response(200, "OK", &view),
        Ok(None) => error_response(404, "Not Found", "Speaking session not found."),
        Err(err) => map_session_http_error(&err),
    }
}

pub(super) async fn handle_session_turn(
    raw_id: &str,
    body: &[u8],
    host: Arc<SpeakingSessionHost>,
) -> HttpResponse {
    let (sid, tx, mode) = match parse_turn_input(raw_id, body) {
        Ok(parsed) => parsed,
        Err(resp) => return resp,
    };
    let now_ms = current_unix_ms();
    let prep = match host
        .prepare_replaceable_text_turn(&sid, &tx, mode, now_ms)
        .await
    {
        Ok(prep) => prep,
        Err(err) => return map_session_http_error(&err),
    };
    let mut guard = RestTurnGuard {
        host: Arc::clone(&host),
        sid: sid.clone(),
        turn_gen: prep.turn_gen,
        completed: false,
    };
    let (sent_tx, sent_rx) = tokio::sync::mpsc::channel(1);
    drop(sent_rx);
    let res = host
        .run_prepared_text_turn_stream(&sid, prep, &tx, now_ms, sent_tx)
        .await;
    guard.completed = true;
    match res {
        Ok(outcome) => json_response(200, "OK", &SessionTurnHttpResponse::from_outcome(outcome)),
        Err(err) => map_session_http_error(&err),
    }
}

struct RestTurnGuard {
    host: Arc<SpeakingSessionHost>,
    sid: SessionId,
    turn_gen: u64,
    completed: bool,
}

impl Drop for RestTurnGuard {
    fn drop(&mut self) {
        if !self.completed {
            let (host, sid, turn_gen) = (Arc::clone(&self.host), self.sid.clone(), self.turn_gen);
            tokio::spawn(async move {
                host.cancel_turn_if_gen(&sid, turn_gen).await;
            });
        }
    }
}

pub(super) fn parse_turn_input(
    raw_id: &str,
    body: &[u8],
) -> Result<(SessionId, Transcript, Option<CorrectionMode>), HttpResponse> {
    let sid = SessionId::new(raw_id)
        .map_err(|_| error_response(400, "Bad Request", "Invalid session ID."))?;
    let req = serde_json::from_slice::<SessionTurnHttpRequest>(body)
        .map_err(|_| error_response(400, "Bad Request", "Invalid JSON request body."))?;
    let tx = parse_non_empty_transcript(&req.user_text)
        .map_err(|_| error_response(400, "Bad Request", "userText must not be empty."))?;
    Ok((sid, tx, req.correction_mode))
}

pub(super) async fn handle_lifecycle(
    raw_id: &str,
    host: &SpeakingSessionHost,
    op: LifecycleOp,
) -> HttpResponse {
    let Ok(sid) = SessionId::new(raw_id) else {
        return error_response(400, "Bad Request", "Invalid session ID.");
    };
    let res = match op {
        LifecycleOp::Pause => host.pause_session(&sid).await,
        LifecycleOp::Resume => host.resume_session(&sid).await,
        LifecycleOp::Stop => host.stop_session(&sid).await,
        LifecycleOp::Status => host.get_status(&sid).await,
    };
    match res {
        Ok(snap) => json_response(200, "OK", &snap),
        Err(err) => map_session_http_error(&err),
    }
}

fn parse_non_empty_transcript(raw: &str) -> Result<Transcript, ()> {
    let tx = Transcript::new(raw, "en", 0, None).map_err(|_| ())?;
    (!tx.is_empty_or_whitespace()).then_some(tx).ok_or(())
}

pub(super) fn current_unix_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
