//! Background turn-execution tasks for WebSocket text and binary-audio speaking turns.

use crate::domain::{
    ConversationRole, ConversationTurn, CorrectionMode, SessionError, SessionErrorKind, SessionId,
    SessionState, Transcript,
};
use crate::local_server::session_handlers::current_unix_ms;
use crate::local_server::ws_audio::WsUtteranceWork;
use crate::local_server::ws_protocol::WsServerEvent;
use crate::modules::conversation::{
    turn::EndToEndTurnResult, SpeakingSessionHost, SpokenTurnEvent, TextTurnOutcome,
};
use std::sync::Arc;
use tokio::sync::mpsc;

pub(super) async fn run_ws_text_turn(
    host: Arc<SpeakingSessionHost>,
    sid: SessionId,
    tx: Transcript,
    mode: Option<CorrectionMode>,
    turn_id: String,
    ev_tx: mpsc::Sender<WsServerEvent>,
) {
    let (sent_tx, sent_rx) = mpsc::channel(16);
    let _ = ev_tx
        .send(WsServerEvent::state_only(&sid, SessionState::Thinking))
        .await;
    let (res, _) = tokio::join!(
        host.submit_text_turn_stream(&sid, &tx, mode, current_unix_ms(), sent_tx),
        forward_text_sentences(sent_rx, &sid, &turn_id, &ev_tx)
    );
    finish_text_turn(&host, &sid, res, &ev_tx).await;
}

pub(super) async fn run_ws_spoken_turn(
    host: Arc<SpeakingSessionHost>,
    sid: SessionId,
    work: WsUtteranceWork,
    turn_id: String,
    ev_tx: mpsc::Sender<WsServerEvent>,
) {
    let (stage_tx, stage_rx) = mpsc::channel(24);
    let (res, _) = tokio::join!(
        host.process_spoken_utterance_with_ack(
            &sid,
            &work.utt,
            current_unix_ms(),
            Some(stage_tx),
            work.started_tx,
        ),
        forward_spoken_events(stage_rx, &sid, &turn_id, &ev_tx)
    );
    finish_spoken_turn(&host, &sid, res, &ev_tx).await;
}

async fn finish_text_turn(
    host: &SpeakingSessionHost,
    sid: &SessionId,
    res: Result<TextTurnOutcome, SessionError>,
    ev_tx: &mpsc::Sender<WsServerEvent>,
) {
    match res {
        Ok(outcome) => {
            let _ = ev_tx.send(WsServerEvent::from_text_outcome(outcome)).await;
            emit_host_status(host, sid, ev_tx).await;
        }
        Err(err) => emit_non_cancel_error(host, sid, &err, ev_tx).await,
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
        Err(err) => emit_non_cancel_error(host, sid, &err, ev_tx).await,
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

async fn forward_text_sentences(
    mut sent_rx: mpsc::Receiver<String>,
    sid: &SessionId,
    turn_id: &str,
    ev_tx: &mpsc::Sender<WsServerEvent>,
) {
    let mut speaking = false;
    while let Some(text) = sent_rx.recv().await {
        if !speaking {
            speaking = true;
            let _ = ev_tx
                .send(WsServerEvent::state_only(sid, SessionState::Speaking))
                .await;
        }
        if ev_tx.send(sentence_ev(sid, turn_id, text)).await.is_err() {
            break;
        }
    }
}

async fn forward_spoken_events(
    mut rx: mpsc::Receiver<SpokenTurnEvent>,
    sid: &SessionId,
    turn_id: &str,
    ev_tx: &mpsc::Sender<WsServerEvent>,
) {
    while let Some(ev) = rx.recv().await {
        if ev_tx.send(map_spoken_ev(ev, sid, turn_id)).await.is_err() {
            break;
        }
    }
}

fn map_spoken_ev(ev: SpokenTurnEvent, sid: &SessionId, turn_id: &str) -> WsServerEvent {
    match ev {
        SpokenTurnEvent::StateChanged(st) => WsServerEvent::state_only(sid, st),
        SpokenTurnEvent::UserTranscript(utx) => WsServerEvent::UserTranscript {
            session_id: sid.clone(),
            confidence: utx.transcript.confidence,
            turn: build_user_turn(sid, utx.transcript.text),
        },
        SpokenTurnEvent::Sentence(text) => sentence_ev(sid, turn_id, text),
    }
}

fn sentence_ev(sid: &SessionId, turn_id: &str, text: String) -> WsServerEvent {
    WsServerEvent::Sentence {
        session_id: sid.clone(),
        turn_id: turn_id.to_string(),
        text,
    }
}

fn build_user_turn(sid: &SessionId, text: String) -> ConversationTurn {
    ConversationTurn {
        id: format!("ws-u-{}", current_unix_ms()),
        session_id: sid.clone(),
        role: ConversationRole::User,
        text,
        correction: None,
        follow_up_prompt: None,
        timestamp_unix_ms: current_unix_ms(),
        stage_metadata: None,
    }
}

async fn emit_host_status(
    host: &SpeakingSessionHost,
    sid: &SessionId,
    ev_tx: &mpsc::Sender<WsServerEvent>,
) {
    if let Ok(snap) = host.get_status(sid).await {
        let _ = ev_tx.send(WsServerEvent::from_snapshot(&snap)).await;
    }
}

async fn emit_non_cancel_error(
    host: &SpeakingSessionHost,
    sid: &SessionId,
    err: &SessionError,
    ev_tx: &mpsc::Sender<WsServerEvent>,
) {
    if err.kind != SessionErrorKind::Cancelled {
        let _ = ev_tx.send(WsServerEvent::from_error(sid, err)).await;
        emit_host_status(host, sid, ev_tx).await;
    }
}
