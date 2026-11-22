//! WebSocket turn event-forwarding and error-emission helpers.

use crate::domain::{
    ConversationRole, ConversationTurn, SessionError, SessionErrorKind, SessionId, SessionState,
};
use crate::local_server::session_handlers::current_unix_ms;
use crate::local_server::ws_protocol::WsServerEvent;
use crate::modules::conversation::{SpeakingSessionHost, SpokenTurnEvent};
use axum::extract::ws::{Message, WebSocket};
use futures_util::{stream::SplitSink, SinkExt};
use tokio::sync::mpsc;

pub(super) async fn forward_text_sentences(
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

pub(super) async fn forward_spoken_events(
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

pub(super) async fn emit_host_status(
    host: &SpeakingSessionHost,
    sid: &SessionId,
    ev_tx: &mpsc::Sender<WsServerEvent>,
) {
    if let Ok(snap) = host.get_status(sid).await {
        let _ = ev_tx.send(WsServerEvent::from_snapshot(&snap)).await;
    }
}

pub(super) async fn emit_non_cancel_error(
    host: &SpeakingSessionHost,
    sid: &SessionId,
    turn_id: Option<String>,
    err: &SessionError,
    ev_tx: &mpsc::Sender<WsServerEvent>,
) {
    if err.kind != SessionErrorKind::Cancelled {
        let _ = ev_tx
            .send(WsServerEvent::from_error_for_turn(sid, err, turn_id))
            .await;
        emit_host_status(host, sid, ev_tx).await;
    }
}

pub(super) async fn send_ws_event(
    ws_tx: &mut SplitSink<WebSocket, Message>,
    ev: &WsServerEvent,
) -> Result<(), ()> {
    let json = serde_json::to_string(ev).map_err(|_| ())?;
    ws_tx.send(Message::Text(json.into())).await.map_err(|_| ())
}

pub(super) async fn emit_ws_protocol_err(
    sid: &SessionId,
    ev_tx: &mpsc::Sender<WsServerEvent>,
    msg: impl Into<String>,
) {
    let ev = WsServerEvent::Error {
        session_id: sid.clone(),
        turn_id: None,
        error: msg.into(),
        kind: None,
        recoverable: true,
    };
    let _ = ev_tx.send(ev).await;
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
