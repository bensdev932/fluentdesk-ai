//! Axum WebSocket upgrade handler and bounded-channel session coordinator (`GET /api/sessions/:id/ws`).

use crate::domain::SessionId;
use crate::local_server::dto::error_response;
use crate::local_server::router::BridgeState;
use crate::local_server::session_dto::map_session_http_error;
use crate::local_server::ws_audio::{
    default_ws_audio_format, spawn_audio_worker, WsAudioMsg, WsUtteranceWork,
};
use crate::local_server::ws_commands::{
    abort_active_turn, handle_binary_audio, handle_control_text, send_ws_event, start_spoken_turn,
    WsConnCtx,
};
use crate::local_server::ws_origin::validate_loopback_origin;
use crate::local_server::ws_protocol::WsServerEvent;
use crate::modules::conversation::{SessionStatusSnapshot, SpeakingSessionHost};
use crate::modules::speech::audio_capture::DEFAULT_AUDIO_CHANNEL_CAPACITY;
use axum::extract::{
    ws::{Message, WebSocket, WebSocketUpgrade},
    Path, State,
};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use futures_util::{stream::SplitSink, StreamExt};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;

const MAX_WS_FRAME_BYTES: usize = 256 * 1024;

pub(super) async fn handle_ws_upgrade(
    Path(raw_id): Path<String>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
    State(state): State<BridgeState>,
) -> Response {
    if let Err(resp) = validate_loopback_origin(&headers) {
        return resp.into_response();
    }
    let Ok(sid) = SessionId::new(&raw_id) else {
        return error_response(400, "Bad Request", "Invalid session ID.").into_response();
    };
    let snap = match active_session_snapshot(&state.host, &sid).await {
        Ok(s) => s,
        Err(resp) => return *resp,
    };
    ws.max_message_size(MAX_WS_FRAME_BYTES)
        .max_frame_size(MAX_WS_FRAME_BYTES)
        .on_upgrade(move |sock| serve_session_ws(sock, state.host, sid, snap))
}

async fn active_session_snapshot(
    host: &SpeakingSessionHost,
    sid: &SessionId,
) -> Result<SessionStatusSnapshot, Box<Response>> {
    match host.get_status(sid).await {
        Ok(s) if s.state.is_active() => Ok(s),
        Ok(_) => Err(Box::new(
            error_response(409, "Conflict", "Speaking session is not active.").into_response(),
        )),
        Err(e) => Err(Box::new(map_session_http_error(&e).into_response())),
    }
}

async fn serve_session_ws(
    socket: WebSocket,
    host: Arc<SpeakingSessionHost>,
    sid: SessionId,
    initial: SessionStatusSnapshot,
) {
    let (mut ws_tx, mut ws_rx) = socket.split();
    let (ev_tx, mut ev_rx) = mpsc::channel::<WsServerEvent>(32);
    let (audio_tx, audio_rx) = mpsc::channel::<WsAudioMsg>(DEFAULT_AUDIO_CHANNEL_CAPACITY + 8);
    let (utt_tx, mut utt_rx) = mpsc::channel::<WsUtteranceWork>(16);
    spawn_audio_worker(
        Arc::clone(&host),
        sid.clone(),
        audio_rx,
        utt_tx,
        ev_tx.clone(),
    );
    let _ = ev_tx.send(WsServerEvent::from_snapshot(&initial)).await;
    let mut ctx = build_conn_ctx(host, sid, ev_tx, audio_tx);
    run_ws_loop(&mut ctx, &mut ws_tx, &mut ws_rx, &mut ev_rx, &mut utt_rx).await;
    cleanup_ws_session(&mut ctx).await;
}

fn build_conn_ctx(
    host: Arc<SpeakingSessionHost>,
    sid: SessionId,
    ev_tx: mpsc::Sender<WsServerEvent>,
    audio_tx: mpsc::Sender<WsAudioMsg>,
) -> WsConnCtx {
    WsConnCtx {
        host,
        sid,
        format: default_ws_audio_format(),
        turn_seq: 0,
        active_turn: None,
        active_turn_gen: Arc::new(AtomicU64::new(0)),
        ev_tx,
        audio_tx,
        stopped: false,
    }
}

async fn run_ws_loop(
    ctx: &mut WsConnCtx,
    ws_tx: &mut SplitSink<WebSocket, Message>,
    ws_rx: &mut futures_util::stream::SplitStream<WebSocket>,
    ev_rx: &mut mpsc::Receiver<WsServerEvent>,
    utt_rx: &mut mpsc::Receiver<WsUtteranceWork>,
) {
    loop {
        tokio::select! {
            biased;
            maybe_ev = ev_rx.recv() => match maybe_ev {
                Some(ev) => if send_ws_event(ws_tx, &ev).await.is_err() { break; },
                None => break,
            },
            maybe_utt = utt_rx.recv() => {
                if let Some(work) = maybe_utt { start_spoken_turn(ctx, work); }
            },
            maybe_msg = ws_rx.next() => match maybe_msg {
                Some(Ok(msg)) => if !handle_inbound_msg(ctx, ws_tx, msg).await { break; },
                _ => break,
            },
        }
    }
}

async fn handle_inbound_msg(
    ctx: &mut WsConnCtx,
    ws_tx: &mut SplitSink<WebSocket, Message>,
    msg: Message,
) -> bool {
    match msg {
        Message::Text(txt) => handle_control_text(ctx, ws_tx, &txt).await,
        Message::Binary(bin) => {
            handle_binary_audio(ctx, &bin).await;
            true
        }
        Message::Close(_) => false,
        Message::Ping(_) | Message::Pong(_) => true,
    }
}

async fn cleanup_ws_session(ctx: &mut WsConnCtx) {
    abort_active_turn(ctx);
    if !ctx.stopped {
        let turn_gen = ctx.active_turn_gen.load(Ordering::Acquire);
        if turn_gen != 0 {
            let _ = ctx.host.cancel_turn_if_gen(&ctx.sid, turn_gen).await;
        }
    }
}
