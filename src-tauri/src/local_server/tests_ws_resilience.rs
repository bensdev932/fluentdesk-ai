//! WebSocket turn overlap and turnId correlation regression tests.

use crate::local_server::tests::ollama_http;
use crate::local_server::tests_cycling::{http_json, read_req, setup_bridge};
use crate::local_server::tests_ws::{recv_ws_json, ws_connect_with_origin};
use futures_util::SinkExt;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{watch, Notify};
use tokio_tungstenite::tungstenite::Message;

async fn spawn_overlap_ollama() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        while let Ok((mut sock, _)) = listener.accept().await {
            tokio::spawn(async move {
                let req = read_req(&mut sock).await;
                let body = if req.contains("/api/tags") {
                    r#"{"models":[{"name":"qwen2.5:7b"}]}"#
                } else if req.contains("Second turn") {
                    r#"{"response":"{\"spokenReply\":\"Second turn won!\"}"}"#
                } else {
                    tokio::time::sleep(Duration::from_millis(250)).await;
                    r#"{"response":"{\"spokenReply\":\"Stale first reply.\"}"}"#
                };
                let _ = sock.write_all(ollama_http("200 OK", body).as_bytes()).await;
                let _ = sock.shutdown().await;
            });
        }
    });
    port
}

#[tokio::test]
async fn ws_overlapping_start_turn_cancels_prior_host_state_and_echoes_turn_id() {
    let ol_port = spawn_overlap_ollama().await;
    let bridge = setup_bridge(ol_port, vec![]).await;
    let sid = http_json(bridge, "POST", "/api/sessions", "{}").await.2["sessionId"]
        .as_str()
        .unwrap()
        .to_string();
    let mut ws = ws_connect_with_origin(bridge, &sid, Some("http://localhost:5173"))
        .await
        .unwrap();
    let _ = recv_ws_json(&mut ws).await;

    ws.send(Message::Text(
        json!({"type": "start_turn", "turnId": "c-1", "userText": "First slow turn"})
            .to_string()
            .into(),
    ))
    .await
    .unwrap();
    // Wait until turn 1 enters Thinking on the host.
    loop {
        let ev: Value = recv_ws_json(&mut ws).await;
        if ev["type"] == "state_changed" && ev["state"] == "Thinking" {
            break;
        }
    }
    tokio::time::sleep(Duration::from_millis(30)).await;

    // Immediately issue a second start_turn without an explicit barge_in.
    ws.send(Message::Text(
        json!({"type": "start_turn", "turnId": "c-2", "userText": "Second turn"})
            .to_string()
            .into(),
    ))
    .await
    .unwrap();

    loop {
        let ev: Value = recv_ws_json(&mut ws).await;
        assert_ne!(
            ev["type"], "error",
            "overlapping start_turn must not fail with InvalidStateTransition: {ev}"
        );
        if ev["type"] == "turn_complete" {
            assert_eq!(ev["turnId"], "c-2");
            assert_eq!(ev["reply"]["spokenReply"], "Second turn won!");
            break;
        }
    }
}

async fn send_sse_req(port: u16, sid: &str, text: &str) -> TcpStream {
    let mut sock = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    let body = format!(r#"{{"userText":"{text}"}}"#);
    let req = format!(
        "POST /api/sessions/{sid}/turn HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAccept: text/event-stream\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    sock.write_all(req.as_bytes()).await.unwrap();
    sock
}

async fn spawn_gated_mixed_ollama() -> (u16, Arc<Notify>, watch::Sender<bool>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let entered = Arc::new(Notify::new());
    let (release_tx, release_rx) = watch::channel(false);
    let n = Arc::clone(&entered);
    tokio::spawn(async move {
        while let Ok((mut sock, _)) = listener.accept().await {
            let (n, mut rx) = (Arc::clone(&n), release_rx.clone());
            tokio::spawn(async move {
                let req = read_req(&mut sock).await;
                let body = if req.contains("/api/tags") {
                    r#"{"models":[{"name":"qwen2.5:7b"}]}"#
                } else if req.contains("Gated fallback turn") {
                    n.notify_one();
                    let _ = rx.wait_for(|v| *v).await;
                    r#"{"response":"{\"spokenReply\":\"Gated turn survived!\"}"}"#
                } else if req.contains("Quick WS") {
                    r#"{"response":"{\"spokenReply\":\"WS superseded SSE!\"}"}"#
                } else if req.contains("Gated turn") {
                    n.notify_one();
                    let _ = rx.wait_for(|v| *v).await;
                    r#"{"response":"{\"spokenReply\":\"Gated turn survived!\"}"}"#
                } else {
                    r#"{"response":"{\"spokenReply\":\"WS superseded SSE!\"}"}"#
                };
                let _ = sock.write_all(ollama_http("200 OK", body).as_bytes()).await;
                let _ = sock.shutdown().await;
            });
        }
    });
    (port, entered, release_tx)
}

#[tokio::test]
async fn ws_start_turn_supersedes_in_flight_http_sse_and_ws_close_preserves_sse_fallback() {
    let (ol_port, entered, release) = spawn_gated_mixed_ollama().await;
    let bridge = setup_bridge(ol_port, vec![]).await;
    let sid = http_json(bridge, "POST", "/api/sessions", "{}").await.2["sessionId"]
        .as_str()
        .unwrap()
        .to_string();

    // 1. Start slow HTTP SSE turn, then immediately issue WS start_turn while SSE is in flight.
    let sse1 = send_sse_req(bridge, &sid, "Gated turn").await;
    tokio::time::timeout(Duration::from_secs(2), entered.notified())
        .await
        .unwrap();
    let mut ws = ws_connect_with_origin(bridge, &sid, Some("http://localhost:5173"))
        .await
        .unwrap();
    let _ = recv_ws_json(&mut ws).await;
    ws.send(Message::Text(
        json!({"type": "start_turn", "turnId": "ws-over-sse", "userText": "Quick WS"})
            .to_string()
            .into(),
    ))
    .await
    .unwrap();
    loop {
        let ev = recv_ws_json(&mut ws).await;
        assert_ne!(ev["type"], "error", "WS must supersede SSE: {ev}");
        if ev["type"] == "turn_complete" {
            assert_eq!(ev["turnId"], "ws-over-sse");
            assert_eq!(ev["reply"]["spokenReply"], "WS superseded SSE!");
            break;
        }
    }
    drop(sse1);

    // 2. Start HTTP SSE fallback turn while old WS is open, then close WS; SSE must NOT be cancelled.
    let mut sse2 = send_sse_req(bridge, &sid, "Gated fallback turn").await;
    tokio::time::timeout(Duration::from_secs(2), entered.notified())
        .await
        .unwrap();
    drop(ws);
    tokio::time::sleep(Duration::from_millis(60)).await;
    let _ = release.send(true);
    let mut raw = Vec::new();
    tokio::time::timeout(Duration::from_secs(3), sse2.read_to_end(&mut raw))
        .await
        .unwrap()
        .unwrap();
    let text = String::from_utf8_lossy(&raw);
    assert!(
        text.contains("event: done") && text.contains("Gated turn survived!"),
        "closing idle/older WS must not cancel active HTTP SSE turn: {text}"
    );
}
