//! Loopback WebSocket integration tests: strict `Origin` allowlisting, `StartTurn` streaming, `BargeIn`, and `Stop`.

use crate::local_server::tests::ollama_http;
use crate::local_server::tests_cycling::{http_json, setup_bridge};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{client::IntoClientRequest, http::HeaderValue, Error as WsError, Message},
    MaybeTlsStream, WebSocketStream,
};

type WsClient = WebSocketStream<MaybeTlsStream<TcpStream>>;

pub(super) async fn ws_connect_with_origin(
    port: u16,
    sid: &str,
    origin: Option<&str>,
) -> Result<WsClient, u16> {
    let url = format!("ws://127.0.0.1:{port}/api/sessions/{sid}/ws");
    let mut req = url.into_client_request().unwrap();
    if let Some(org) = origin {
        req.headers_mut()
            .insert("Origin", HeaderValue::from_str(org).unwrap());
    }
    match connect_async(req).await {
        Ok((ws, _)) => Ok(ws),
        Err(WsError::Http(resp)) => Err(resp.status().as_u16()),
        Err(e) => panic!("unexpected ws error: {e}"),
    }
}

pub(super) async fn recv_ws_json(ws: &mut WsClient) -> Value {
    let msg = tokio::time::timeout(Duration::from_secs(2), ws.next())
        .await
        .expect("ws frame timeout")
        .expect("ws open")
        .expect("ws frame ok");
    serde_json::from_str(&msg.into_text().expect("text frame")).expect("valid json")
}

async fn recv_until(ws: &mut WsClient, pred: impl Fn(&Value) -> bool) -> Value {
    loop {
        let ev = recv_ws_json(ws).await;
        if pred(&ev) {
            return ev;
        }
    }
}

async fn spawn_barge_in_ollama() -> (u16, watch::Sender<bool>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let (gate_tx, gate_rx) = watch::channel(false);
    tokio::spawn(async move {
        while let Ok((mut sock, _)) = listener.accept().await {
            let mut rx = gate_rx.clone();
            tokio::spawn(async move {
                let req = crate::local_server::tests_cycling::read_req(&mut sock).await;
                let quick = if req.contains("/api/tags") {
                    Some(r#"{"models":[{"name":"qwen2.5:7b"}]}"#)
                } else if req.contains("After barge in") || !req.contains("Hold gate") {
                    Some(r#"{"response":"{\"spokenReply\":\"Fresh turn ok!\"}"}"#)
                } else {
                    None
                };
                if let Some(body) = quick {
                    let _ = sock.write_all(ollama_http("200 OK", body).as_bytes()).await;
                    let _ = sock.shutdown().await;
                    return;
                }
                let _ = sock
                    .write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/x-ndjson\r\n\r\n")
                    .await;
                let _ = sock
                    .write_all(
                        b"{\"response\":\"{\\\"spokenReply\\\":\\\"Part one! \",\"done\":false}\n",
                    )
                    .await;
                let _ = sock.flush().await;
                let _ = rx.wait_for(|v| *v).await;
                let _ = sock
                    .write_all(b"{\"response\":\"Part two.\\\"}\",\"done\":true}\n")
                    .await;
                let _ = sock.shutdown().await;
            });
        }
    });
    (port, gate_tx)
}

#[tokio::test]
async fn ws_upgrade_enforces_strict_loopback_origin_allowlist() {
    let (ol_port, _) = spawn_barge_in_ollama().await;
    let bridge = setup_bridge(ol_port, vec![]).await;
    let sid = http_json(bridge, "POST", "/api/sessions", "{}").await.2["sessionId"]
        .as_str()
        .unwrap()
        .to_string();

    for bad in [
        None,
        Some("https://evil.example"),
        Some("http://localhost.evil.com"),
        Some("null"),
    ] {
        assert_eq!(
            ws_connect_with_origin(bridge, &sid, bad).await.unwrap_err(),
            403
        );
    }
    for good in [
        "http://127.0.0.1:3420",
        "http://localhost:5173",
        "http://tauri.localhost",
        "tauri://localhost",
    ] {
        let mut ws = ws_connect_with_origin(bridge, &sid, Some(good))
            .await
            .unwrap();
        let ev = recv_ws_json(&mut ws).await;
        assert_eq!(
            (ev["type"].as_str(), ev["state"].as_str()),
            (Some("state_changed"), Some("Listening"))
        );
        let _ = ws.close(None).await;
    }
}

#[tokio::test]
async fn ws_start_turn_streams_sentences_barge_in_cancels_and_stop_ends_session() {
    let (ol_port, _gate) = spawn_barge_in_ollama().await;
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
        json!({"type": "StartTurn", "userText": "Hold gate"})
            .to_string()
            .into(),
    ))
    .await
    .unwrap();
    let s1 = recv_until(&mut ws, |ev| ev["type"] == "sentence").await;
    assert_eq!(s1["text"], "Part one!");

    ws.send(Message::Text(
        json!({"type": "barge_in"}).to_string().into(),
    ))
    .await
    .unwrap();
    recv_until(&mut ws, |ev| {
        ev["type"] == "state_changed" && ev["state"] == "Listening"
    })
    .await;

    ws.send(Message::Text(
        json!({"type": "start_turn", "userText": "After barge in"})
            .to_string()
            .into(),
    ))
    .await
    .unwrap();
    let done = recv_until(&mut ws, |ev| ev["type"] == "turn_complete").await;
    assert_eq!(done["reply"]["spokenReply"], "Fresh turn ok!");

    ws.send(Message::Text(json!({"type": "Stop"}).to_string().into()))
        .await
        .unwrap();
    recv_until(&mut ws, |ev| {
        ev["type"] == "state_changed" && ev["state"] == "Ended"
    })
    .await;
    assert_eq!(
        http_json(bridge, "GET", &format!("/api/sessions/{sid}/status"), "")
            .await
            .2["state"],
        "Ended"
    );
}
