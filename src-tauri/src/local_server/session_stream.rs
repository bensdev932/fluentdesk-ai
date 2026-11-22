//! Axum SSE streaming turn endpoint (`POST /api/sessions/:id/turn/stream`).

use crate::domain::SessionId;
use crate::local_server::{
    session_dto::SessionTurnHttpResponse,
    session_handlers::{current_unix_ms, map_session_http_error, parse_turn_input},
};
use crate::modules::conversation::{SpeakingSessionHost, TextTurnOutcome};
use axum::response::sse::{Event, Sse};
use axum::response::{IntoResponse, Response};
use futures_util::Stream;
use serde_json::json;
use std::convert::Infallible;
use std::pin::Pin;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::task::{Context, Poll};
use tokio::sync::mpsc;

type TurnJoin = tokio::task::JoinHandle<Result<TextTurnOutcome, crate::domain::SessionError>>;

/// Streams spoken sentences over SSE as they generate, ending with `event: done` or `event: error`.
pub(super) async fn handle_turn_sse_stream(
    raw_id: &str,
    body: &[u8],
    host: Arc<SpeakingSessionHost>,
) -> Response {
    let (sid, tx, mode) = match parse_turn_input(raw_id, body) {
        Ok(parsed) => parsed,
        Err(resp) => return resp.into_response(),
    };
    let now_ms = current_unix_ms();
    let prep = match host
        .prepare_replaceable_text_turn(&sid, &tx, mode, now_ms)
        .await
    {
        Ok(prep) => prep,
        Err(err) => return map_session_http_error(&err).into_response(),
    };
    let turn_gen = prep.turn_gen;
    let (sent_tx, sent_rx) = mpsc::channel(16);
    let (sse_tx, sse_rx) = mpsc::channel(16);
    let completed = Arc::new(AtomicBool::new(false));
    let (t_host, t_sid) = (Arc::clone(&host), sid.clone());
    let task = tokio::spawn(async move {
        t_host
            .run_prepared_text_turn_stream(&t_sid, prep, &tx, now_ms, sent_tx)
            .await
    });
    tokio::spawn(pump_sse(
        sse_tx,
        sent_rx,
        Arc::clone(&host),
        sid.clone(),
        turn_gen,
        task,
        Arc::clone(&completed),
    ));
    Sse::new(SseTurnStream {
        rx: sse_rx,
        host,
        sid,
        turn_gen,
        completed,
    })
    .into_response()
}

struct SseTurnStream {
    rx: mpsc::Receiver<Result<Event, Infallible>>,
    host: Arc<SpeakingSessionHost>,
    sid: SessionId,
    turn_gen: u64,
    completed: Arc<AtomicBool>,
}

impl Stream for SseTurnStream {
    type Item = Result<Event, Infallible>;
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.rx).poll_recv(cx)
    }
}

impl Drop for SseTurnStream {
    fn drop(&mut self) {
        if !self.completed.load(Ordering::Acquire) {
            let (host, sid, turn_gen) = (Arc::clone(&self.host), self.sid.clone(), self.turn_gen);
            tokio::spawn(async move {
                host.cancel_turn_if_gen(&sid, turn_gen).await;
            });
        }
    }
}

async fn pump_sse(
    sse_tx: mpsc::Sender<Result<Event, Infallible>>,
    mut sent_rx: mpsc::Receiver<String>,
    host: Arc<SpeakingSessionHost>,
    sid: SessionId,
    turn_gen: u64,
    task: TurnJoin,
    completed: Arc<AtomicBool>,
) {
    while let Some(sentence) = sent_rx.recv().await {
        let payload = json!({ "text": sentence }).to_string();
        if send_ev(&sse_tx, "sentence", payload).await.is_err() {
            drop(sent_rx);
            host.cancel_turn_if_gen(&sid, turn_gen).await;
            let _ = task.await;
            return;
        }
    }
    emit_terminal_sse(&sse_tx, task, &completed).await;
}

async fn emit_terminal_sse(
    sse_tx: &mpsc::Sender<Result<Event, Infallible>>,
    task: TurnJoin,
    completed: &AtomicBool,
) {
    let (ev_name, data) = match task.await {
        Ok(Ok(o)) => (
            "done",
            serde_json::to_string(&SessionTurnHttpResponse::from_outcome(o))
                .unwrap_or_else(|_| "{}".into()),
        ),
        Ok(Err(err)) => ("error", sse_error_payload(&err)),
        Err(_) => (
            "error",
            r#"{"error":"Turn task crashed.","status":502}"#.to_string(),
        ),
    };
    completed.store(true, Ordering::Release);
    let _ = send_ev(sse_tx, ev_name, data).await;
}

fn sse_error_payload(err: &crate::domain::SessionError) -> String {
    let mapped = map_session_http_error(err);
    let msg = crate::telemetry::redact_sensitive_log_text(&err.user_message);
    json!({ "error": msg, "status": mapped.status }).to_string()
}

async fn send_ev(
    tx: &mpsc::Sender<Result<Event, Infallible>>,
    event: &str,
    data: String,
) -> Result<(), ()> {
    tx.send(Ok(Event::default().event(event).data(data)))
        .await
        .map_err(|_| ())
}
