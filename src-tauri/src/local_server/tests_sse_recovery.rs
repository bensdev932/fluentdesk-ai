//! Integration tests for HTTP SSE/REST immediate retry recovery and stale `SseTurnStream::drop` isolation.

use crate::local_server::tests::ollama_http;
use crate::local_server::tests_cycling::{http_json, read_req, setup_bridge};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{watch, Notify};

async fn spawn_gated_retry_ollama() -> (u16, Arc<Notify>, Arc<Notify>, watch::Sender<bool>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let t1_seen = Arc::new(Notify::new());
    let t2_seen = Arc::new(Notify::new());
    let (t2_release_tx, t2_release_rx) = watch::channel(false);
    let (n1, n2) = (Arc::clone(&t1_seen), Arc::clone(&t2_seen));
    tokio::spawn(async move {
        while let Ok((mut sock, _)) = listener.accept().await {
            let (n1, n2, mut rx) = (Arc::clone(&n1), Arc::clone(&n2), t2_release_rx.clone());
            tokio::spawn(async move {
                let req = read_req(&mut sock).await;
                if req.contains("/api/tags") {
                    let tags = ollama_http("200 OK", r#"{"models":[{"name":"qwen2.5:7b"}]}"#);
                    let _ = sock.write_all(tags.as_bytes()).await;
                    let _ = sock.shutdown().await;
                    return;
                }
                if req.contains("Retry turn 2") {
                    n2.notify_one();
                    let _ = rx.wait_for(|v| *v).await;
                    let ok = ollama_http(
                        "200 OK",
                        r#"{"response":"{\"spokenReply\":\"Retry turn 2 succeeded!\"}"}"#,
                    );
                    let _ = sock.write_all(ok.as_bytes()).await;
                    let _ = sock.shutdown().await;
                    return;
                }
                if req.contains("Plain rest retry") {
                    let ok =
                        ollama_http("200 OK", r#"{"response":"{\"spokenReply\":\"Rest ok!\"}"}"#);
                    let _ = sock.write_all(ok.as_bytes()).await;
                    let _ = sock.shutdown().await;
                    return;
                }
                if req.contains("Slow turn 1") {
                    n1.notify_one();
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    return;
                }
                let ok = ollama_http("200 OK", r#"{"response":"{\"spokenReply\":\"Rest ok!\"}"}"#);
                let _ = sock.write_all(ok.as_bytes()).await;
                let _ = sock.shutdown().await;
            });
        }
    });
    (port, t1_seen, t2_seen, t2_release_tx)
}

async fn send_sse_turn_request(port: u16, sid: &str, text: &str) -> TcpStream {
    let mut sock = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    let body = format!(r#"{{"userText":"{text}"}}"#);
    let req = format!(
        "POST /api/sessions/{sid}/turn HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAccept: text/event-stream, application/json\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    sock.write_all(req.as_bytes()).await.unwrap();
    sock
}

#[tokio::test]
async fn sse_immediate_retry_before_prior_drop_succeeds_and_late_drop_cannot_cancel_retry() {
    let (ol_port, t1_seen, t2_seen, t2_release) = spawn_gated_retry_ollama().await;
    let bridge = setup_bridge(ol_port, vec![]).await;
    let sid = http_json(bridge, "POST", "/api/sessions", "{}").await.2["sessionId"]
        .as_str()
        .unwrap()
        .to_string();

    let sock1 = send_sse_turn_request(bridge, &sid, "Slow turn 1").await;
    tokio::time::timeout(Duration::from_secs(2), t1_seen.notified())
        .await
        .expect("turn 1 must enter Ollama request");

    // Client retries IMMEDIATELY while sock1 is still open (SseTurnStream #1 has NOT dropped yet).
    let mut sock2 = send_sse_turn_request(bridge, &sid, "Retry turn 2").await;
    tokio::time::timeout(Duration::from_secs(2), t2_seen.notified())
        .await
        .expect("retry turn 2 must supersede slow turn 1 without InvalidStateTransition");

    // Now drop sock1 so SseTurnStream #1::drop runs WHILE turn 2 is actively Thinking!
    drop(sock1);
    tokio::time::sleep(Duration::from_millis(60)).await;
    let _ = t2_release.send(true);

    let mut raw2 = Vec::new();
    tokio::time::timeout(Duration::from_secs(3), sock2.read_to_end(&mut raw2))
        .await
        .expect("turn 2 response must finish")
        .unwrap();
    let text2 = String::from_utf8_lossy(&raw2);
    assert!(
        text2.starts_with("HTTP/1.1 200"),
        "unexpected head: {text2}"
    );
    assert!(
        !text2.contains("event: error"),
        "turn 2 must not be cancelled or error: {text2}"
    );
    assert!(
        text2.contains("event: done") && text2.contains("Retry turn 2 succeeded!"),
        "missing turn 2 done payload: {text2}"
    );
    let st = http_json(bridge, "GET", &format!("/api/sessions/{sid}/status"), "").await;
    assert_eq!(st.2["state"].as_str(), Some("Listening"));
}

#[tokio::test]
async fn plain_rest_retry_supersedes_stalled_sse_while_paused_still_returns_409() {
    let (ol_port, t1_seen, _, _) = spawn_gated_retry_ollama().await;
    let bridge = setup_bridge(ol_port, vec![]).await;
    let sid = http_json(bridge, "POST", "/api/sessions", "{}").await.2["sessionId"]
        .as_str()
        .unwrap()
        .to_string();

    let sock1 = send_sse_turn_request(bridge, &sid, "Slow turn 1").await;
    tokio::time::timeout(Duration::from_secs(2), t1_seen.notified())
        .await
        .unwrap();

    let (st, _, body) = http_json(
        bridge,
        "POST",
        &format!("/api/sessions/{sid}/turn"),
        r#"{"userText":"Plain rest retry"}"#,
    )
    .await;
    drop(sock1);
    assert_eq!(st, 200, "plain REST retry must succeed: {body}");
    assert_eq!(body["reply"]["spokenReply"].as_str(), Some("Rest ok!"));

    assert_eq!(
        http_json(bridge, "POST", &format!("/api/sessions/{sid}/pause"), "")
            .await
            .0,
        200
    );
    let mut paused_sse = send_sse_turn_request(bridge, &sid, "While paused").await;
    let mut raw = Vec::new();
    paused_sse.read_to_end(&mut raw).await.unwrap();
    let paused_text = String::from_utf8_lossy(&raw);
    assert!(
        paused_text.starts_with("HTTP/1.1 409"),
        "paused SSE turn must return 409 Conflict: {paused_text}"
    );
}

#[tokio::test]
async fn sse_error_event_includes_http_status_503_for_unavailable_provider() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let ol_port = listener.local_addr().unwrap().port();
    let (stop_tx, mut stop_rx) = tokio::sync::oneshot::channel::<()>();
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = &mut stop_rx => break,
                Ok((mut sock, _)) = listener.accept() => {
                    let _ = read_req(&mut sock).await;
                    let tags = ollama_http("200 OK", r#"{"models":[{"name":"qwen2.5:7b"}]}"#);
                    let _ = sock.write_all(tags.as_bytes()).await;
                    let _ = sock.shutdown().await;
                }
            }
        }
    });
    let bridge = setup_bridge(ol_port, vec![]).await;
    let sid = http_json(bridge, "POST", "/api/sessions", "{}").await.2["sessionId"]
        .as_str()
        .unwrap()
        .to_string();
    let _ = stop_tx.send(());
    tokio::time::sleep(Duration::from_millis(30)).await;

    let mut sse = send_sse_turn_request(bridge, &sid, "Is Ollama up?").await;
    let mut raw = Vec::new();
    tokio::time::timeout(Duration::from_secs(3), sse.read_to_end(&mut raw))
        .await
        .unwrap()
        .unwrap();
    let text = String::from_utf8_lossy(&raw);
    assert!(text.contains("event: error"), "expected SSE error: {text}");
    assert!(
        text.contains(r#""status":503"#),
        "expected status 503 in SSE error payload: {text}"
    );
}
