//! Loopback integration test for SSE `/api/sessions/:id/turn/stream` early sentence emission.

use crate::local_server::tests::ollama_http;
use crate::local_server::tests_cycling::{http_json, setup_bridge};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;

async fn spawn_gated_ndjson_ollama() -> (u16, watch::Sender<bool>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let (gate_tx, gate_rx) = watch::channel(false);
    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else {
                break;
            };
            let mut rx = gate_rx.clone();
            tokio::spawn(async move {
                let mut buf = [0u8; 4096];
                let n = sock.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]);
                if req.contains("/api/tags") {
                    let tags = ollama_http("200 OK", r#"{"models":[{"name":"qwen2.5:7b"}]}"#);
                    let _ = sock.write_all(tags.as_bytes()).await;
                    let _ = sock.shutdown().await;
                    return;
                }
                let _ = sock
                    .write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/x-ndjson\r\n\r\n")
                    .await;
                let _ = sock
                    .write_all(
                        b"{\"response\":\"{\\\"spokenReply\\\":\\\"First part! \",\"done\":false}\n",
                    )
                    .await;
                let _ = sock.flush().await;
                let _ = rx.wait_for(|v| *v).await;
                let _ = sock
                    .write_all(b"{\"response\":\"Second part?\\\",\\\"correction\\\":\\\"Say went.\\\"}\",\"done\":true}\n")
                    .await;
                let _ = sock.shutdown().await;
            });
        }
    });
    (port, gate_tx)
}

#[tokio::test]
async fn sessions_turn_stream_emits_sse_sentence_before_ollama_finishes() {
    let (ol_port, gate_tx) = spawn_gated_ndjson_ollama().await;
    let bridge = setup_bridge(ol_port, vec![]).await;
    let (st, _, started) = http_json(bridge, "POST", "/api/sessions", "{}").await;
    assert_eq!(st, 200);
    let sid = started["sessionId"].as_str().unwrap().to_string();

    let mut sock = TcpStream::connect(("127.0.0.1", bridge)).await.unwrap();
    let body = r#"{"userText":"Yesterday I go to Rome."}"#;
    let req = format!(
        "POST /api/sessions/{sid}/turn/stream HTTP/1.1\r\nHost: 127.0.0.1:{bridge}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    sock.write_all(req.as_bytes()).await.unwrap();
    let mut reader = BufReader::new(sock);

    let mut saw_first_sentence = false;
    let mut line = String::new();
    while tokio::time::timeout(Duration::from_secs(2), reader.read_line(&mut line))
        .await
        .expect("SSE sentence must arrive before Ollama gate opens")
        .unwrap()
        > 0
    {
        if line.starts_with("data: {\"text\":\"First part!\"}") {
            saw_first_sentence = true;
            break;
        }
        line.clear();
    }
    assert!(saw_first_sentence);
    let _ = gate_tx.send(true);

    let mut rest = String::new();
    while reader.read_line(&mut line).await.unwrap() > 0 {
        rest.push_str(&line);
        line.clear();
    }
    assert!(rest.contains("event: done"));
    assert!(rest.contains("Second part?"));
    assert!(rest.contains("Say went."));
}

async fn serve_abort_conn(mut sock: TcpStream) {
    let mut buf = [0u8; 4096];
    let n = sock.read(&mut buf).await.unwrap_or(0);
    let req = String::from_utf8_lossy(&buf[..n]);
    if !req.contains("Abort me") {
        let b = if req.contains("/api/tags") {
            r#"{"models":[{"name":"qwen2.5:7b"}]}"#
        } else {
            r#"{"response":"{\"spokenReply\":\"Ok!\"}"}"#
        };
        let _ = sock.write_all(ollama_http("200 OK", b).as_bytes()).await;
        return;
    }
    let _ = sock
        .write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/x-ndjson\r\n\r\n")
        .await;
    for c in ["One! ", "Two! ", "Three! ", "Four! "] {
        let l = format!("{{\"response\":\"{{\\\"spokenReply\\\":\\\"{c}\",\"done\":false}}\n");
        if sock.write_all(l.as_bytes()).await.is_err() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(15)).await;
    }
    tokio::time::sleep(Duration::from_secs(5)).await;
}

#[tokio::test]
async fn sse_disconnect_cancels_in_flight_turn_and_keeps_session_listening() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let ol_port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        while let Ok((s, _)) = listener.accept().await {
            tokio::spawn(serve_abort_conn(s));
        }
    });
    let bridge = setup_bridge(ol_port, vec![]).await;
    let s0 = http_json(bridge, "POST", "/api/sessions", "{}").await.2;
    let sid = s0["sessionId"].as_str().unwrap();
    let mut sock = TcpStream::connect(("127.0.0.1", bridge)).await.unwrap();
    let req = format!("POST /api/sessions/{sid}/turn/stream HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: 22\r\n\r\n{{\"userText\":\"Abort me\"}}");
    sock.write_all(req.as_bytes()).await.unwrap();
    let (mut r, mut line) = (BufReader::new(sock), String::new());
    while r.read_line(&mut line).await.unwrap() > 0 && !line.contains("One!") {
        line.clear();
    }
    drop(r);
    tokio::time::sleep(Duration::from_millis(100)).await;
    let st = http_json(bridge, "GET", &format!("/api/sessions/{sid}/status"), "").await;
    let t2 = http_json(
        bridge,
        "POST",
        &format!("/api/sessions/{sid}/turn"),
        r#"{"userText":"Next"}"#,
    )
    .await;
    assert_eq!(
        (st.2["state"].as_str(), t2.0, t2.2["state"].as_str()),
        (Some("Listening"), 200, Some("Listening"))
    );
}
