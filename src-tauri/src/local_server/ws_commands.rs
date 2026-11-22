//! WebSocket control command dispatch and audio/turn handlers.

use crate::domain::{AudioFormat, CorrectionMode, SessionId, Transcript};
use crate::local_server::session_handlers::current_unix_ms;
use crate::local_server::ws_audio::{decode_pcm_frame, WsAudioMsg, WsUtteranceWork};
use crate::local_server::ws_protocol::{WsClientCommand, WsServerEvent};
use crate::local_server::ws_turn_events::emit_ws_protocol_err;
pub(super) use crate::local_server::ws_turn_events::send_ws_event;
use crate::local_server::ws_turns::{run_ws_spoken_turn, run_ws_text_turn, WsTurnCtx};
use crate::modules::conversation::SpeakingSessionHost;
use axum::extract::ws::{Message, WebSocket};
use futures_util::{stream::SplitSink, SinkExt};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc::{self, error::TrySendError};
use tokio::task::JoinHandle;

pub(super) struct WsConnCtx {
    pub host: Arc<SpeakingSessionHost>,
    pub sid: SessionId,
    pub format: AudioFormat,
    pub turn_seq: u64,
    pub active_turn: Option<JoinHandle<()>>,
    pub active_turn_gen: Arc<AtomicU64>,
    pub ev_tx: mpsc::Sender<WsServerEvent>,
    pub audio_tx: mpsc::Sender<WsAudioMsg>,
    pub stopped: bool,
}

pub(super) async fn handle_control_text(
    ctx: &mut WsConnCtx,
    ws_tx: &mut SplitSink<WebSocket, Message>,
    raw: &str,
) -> bool {
    let Ok(cmd) = serde_json::from_str::<WsClientCommand>(raw) else {
        emit_ws_err(ctx, "Invalid WebSocket control payload.").await;
        return true;
    };
    dispatch_command(ctx, ws_tx, cmd).await
}

async fn dispatch_command(
    ctx: &mut WsConnCtx,
    ws_tx: &mut SplitSink<WebSocket, Message>,
    cmd: WsClientCommand,
) -> bool {
    match cmd {
        WsClientCommand::StartTurn {
            user_text,
            correction_mode,
            turn_id,
        } => handle_start_turn(ctx, user_text, correction_mode, turn_id).await,
        WsClientCommand::AudioFormat(p) => handle_audio_format(ctx, p.into_audio_format()).await,
        WsClientCommand::PttHold { held } => handle_ptt_hold(ctx, held).await,
        WsClientCommand::BargeIn {
            heard_sentences,
            played_ms,
        } => handle_barge_in(ctx, heard_sentences, played_ms).await,
        WsClientCommand::Stop => return handle_stop(ctx, ws_tx).await,
    }
    true
}

async fn handle_audio_format(ctx: &mut WsConnCtx, res: Result<AudioFormat, String>) {
    match res {
        Ok(fmt) => ctx.format = fmt,
        Err(msg) => emit_ws_err(ctx, msg).await,
    }
}

async fn handle_start_turn(
    ctx: &mut WsConnCtx,
    user_text: Option<String>,
    mode: Option<CorrectionMode>,
    client_turn_id: Option<String>,
) {
    let Some(raw) = user_text else {
        emit_status_snapshot(ctx).await;
        return;
    };
    let Some(tx) = parse_ws_transcript(&raw) else {
        emit_ws_err(ctx, "userText must not be empty.").await;
        return;
    };
    abort_active_turn(ctx);
    let tid = resolve_turn_id(ctx, client_turn_id);
    ctx.active_turn = Some(tokio::spawn(run_ws_text_turn(
        ws_turn_ctx(ctx),
        tx,
        mode,
        tid,
    )));
}

fn resolve_turn_id(ctx: &mut WsConnCtx, client_turn_id: Option<String>) -> String {
    let clean = client_turn_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty() && s.len() <= 64);
    match clean {
        Some(id) => id.to_string(),
        None => next_turn_id(ctx),
    }
}

fn parse_ws_transcript(raw: &str) -> Option<Transcript> {
    Transcript::new(raw, "en", 0, None)
        .ok()
        .filter(|t| !t.is_empty_or_whitespace())
}

async fn emit_status_snapshot(ctx: &WsConnCtx) {
    if let Ok(snap) = ctx.host.get_status(&ctx.sid).await {
        let _ = ctx.ev_tx.send(WsServerEvent::from_snapshot(&snap)).await;
    }
}

async fn handle_ptt_hold(ctx: &WsConnCtx, held: bool) {
    if ctx.audio_tx.send(WsAudioMsg::PttHold(held)).await.is_err() {
        emit_ws_err(ctx, "Audio input worker closed.").await;
    }
}

async fn handle_barge_in(
    ctx: &mut WsConnCtx,
    heard_sentences: Option<Vec<String>>,
    played_ms: Option<u32>,
) {
    abort_active_turn(ctx);
    ctx.active_turn_gen.store(0, Ordering::Release);
    let snap = ctx
        .host
        .barge_in_with_progress(&ctx.sid, heard_sentences.as_deref(), played_ms)
        .await;
    if let Ok(s) = snap {
        let _ = ctx.ev_tx.send(WsServerEvent::from_snapshot(&s)).await;
    }
}

async fn handle_stop(ctx: &mut WsConnCtx, ws_tx: &mut SplitSink<WebSocket, Message>) -> bool {
    ctx.stopped = true;
    abort_active_turn(ctx);
    if let Ok(s) = ctx.host.stop_session(&ctx.sid).await {
        let _ = send_ws_event(ws_tx, &WsServerEvent::from_snapshot(&s)).await;
    }
    let _ = ws_tx.send(Message::Close(None)).await;
    false
}

pub(super) async fn handle_binary_audio(ctx: &WsConnCtx, bytes: &[u8]) {
    let ts = u64::try_from(current_unix_ms()).unwrap_or(0);
    match decode_pcm_frame(bytes, ctx.format, ts) {
        Ok(Some(chunk)) => send_audio_chunk(ctx, chunk).await,
        Ok(None) => {}
        Err(msg) => emit_ws_err(ctx, msg).await,
    }
}

async fn send_audio_chunk(ctx: &WsConnCtx, chunk: crate::domain::AudioChunk) {
    if let Err(TrySendError::Closed(_)) = ctx.audio_tx.try_send(WsAudioMsg::Chunk(chunk)) {
        emit_ws_err(ctx, "Audio input worker closed.").await;
    }
}

pub(super) fn start_spoken_turn(ctx: &mut WsConnCtx, work: WsUtteranceWork) {
    abort_active_turn(ctx);
    let tid = next_turn_id(ctx);
    ctx.active_turn = Some(tokio::spawn(run_ws_spoken_turn(
        ws_turn_ctx(ctx),
        work,
        tid,
    )));
}

fn ws_turn_ctx(ctx: &WsConnCtx) -> WsTurnCtx {
    WsTurnCtx {
        host: Arc::clone(&ctx.host),
        sid: ctx.sid.clone(),
        ev_tx: ctx.ev_tx.clone(),
        turn_gen: Arc::clone(&ctx.active_turn_gen),
    }
}

pub(super) fn abort_active_turn(ctx: &mut WsConnCtx) {
    if let Some(handle) = ctx.active_turn.take() {
        handle.abort();
    }
}

fn next_turn_id(ctx: &mut WsConnCtx) -> String {
    ctx.turn_seq += 1;
    format!("ws-{}-{}", ctx.sid.as_str(), ctx.turn_seq)
}

async fn emit_ws_err(ctx: &WsConnCtx, msg: impl Into<String>) {
    emit_ws_protocol_err(&ctx.sid, &ctx.ev_tx, msg).await;
}
