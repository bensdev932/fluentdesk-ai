//! Background turn-execution tasks for WebSocket text and binary-audio speaking turns.

use crate::domain::{CorrectionMode, SessionError, SessionId, SessionState, Transcript};
use crate::local_server::session_handlers::current_unix_ms;
use crate::local_server::ws_audio::WsUtteranceWork;
use crate::local_server::ws_protocol::WsServerEvent;
use crate::local_server::ws_turn_events::{
    emit_host_status, emit_non_cancel_error, forward_spoken_events, forward_text_sentences,
};
use crate::modules::conversation::{
    turn::EndToEndTurnResult, SpeakingSessionHost, SpokenTurnAck, TextTurnOutcome,
};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;

pub(super) struct WsTurnCtx {
    pub host: Arc<SpeakingSessionHost>,
    pub sid: SessionId,
    pub ev_tx: mpsc::Sender<WsServerEvent>,
    pub turn_gen: Arc<AtomicU64>,
}

pub(super) async fn run_ws_text_turn(
    ctx: WsTurnCtx,
    tx: Transcript,
    mode: Option<CorrectionMode>,
    turn_id: String,
) {
    let now_ms = current_unix_ms();
    let prep = match ctx
        .host
        .prepare_replaceable_text_turn(&ctx.sid, &tx, mode, now_ms)
        .await
    {
        Ok(prep) => prep,
        Err(err) => {
            emit_non_cancel_error(&ctx.host, &ctx.sid, Some(turn_id), &err, &ctx.ev_tx).await;
            return;
        }
    };
    ctx.turn_gen.store(prep.turn_gen, Ordering::Release);
    let (sent_tx, sent_rx) = mpsc::channel(16);
    let _ = ctx
        .ev_tx
        .send(WsServerEvent::state_only(&ctx.sid, SessionState::Thinking))
        .await;
    let (res, _) = tokio::join!(
        ctx.host
            .run_prepared_text_turn_stream(&ctx.sid, prep, &tx, now_ms, sent_tx),
        forward_text_sentences(sent_rx, &ctx.sid, &turn_id, &ctx.ev_tx)
    );
    finish_text_turn(&ctx.host, &ctx.sid, turn_id, res, &ctx.ev_tx).await;
}

pub(super) async fn run_ws_spoken_turn(ctx: WsTurnCtx, work: WsUtteranceWork, turn_id: String) {
    let (stage_tx, stage_rx) = mpsc::channel(24);
    let ack = SpokenTurnAck {
        started_tx: work.started_tx,
        turn_gen: &ctx.turn_gen,
    };
    let (res, _) = tokio::join!(
        ctx.host.process_spoken_utterance_with_ack(
            &ctx.sid,
            &work.utt,
            current_unix_ms(),
            Some(stage_tx),
            ack,
        ),
        forward_spoken_events(stage_rx, &ctx.sid, &turn_id, &ctx.ev_tx)
    );
    finish_spoken_turn(&ctx.host, &ctx.sid, res, &ctx.ev_tx).await;
}

async fn finish_text_turn(
    host: &SpeakingSessionHost,
    sid: &SessionId,
    turn_id: String,
    res: Result<TextTurnOutcome, SessionError>,
    ev_tx: &mpsc::Sender<WsServerEvent>,
) {
    match res {
        Ok(outcome) => {
            let _ = ev_tx
                .send(WsServerEvent::from_text_outcome(outcome, Some(turn_id)))
                .await;
            emit_host_status(host, sid, ev_tx).await;
        }
        Err(err) => emit_non_cancel_error(host, sid, Some(turn_id), &err, ev_tx).await,
    }
}

async fn finish_spoken_turn(
    host: &SpeakingSessionHost,
    sid: &SessionId,
    res: Result<EndToEndTurnResult, SessionError>,
    ev_tx: &mpsc::Sender<WsServerEvent>,
) {
    match res {
        Ok(done) => emit_spoken_done(host, sid, done, ev_tx).await,
        Err(err) => emit_non_cancel_error(host, sid, None, &err, ev_tx).await,
    }
}

async fn emit_spoken_done(
    host: &SpeakingSessionHost,
    sid: &SessionId,
    done: EndToEndTurnResult,
    ev_tx: &mpsc::Sender<WsServerEvent>,
) {
    if let Some(tutor_ev) = done.tutor_text_event {
        if let Ok(snap) = host.get_status(sid).await {
            let _ = ev_tx
                .send(WsServerEvent::from_spoken(&snap, tutor_ev))
                .await;
        }
    }
    emit_host_status(host, sid, ev_tx).await;
}
