//! Integration tests for the `127.0.0.1` HTTP bridge (`GET /api/health`, `POST /api/tutor/reply`).

use crate::ai::ollama::OllamaConfig;
use crate::local_server::LocalBridgeServer;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

async fn spawn_ollama_stub(responses: Vec<String>) -> (u16, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind stub");
    let port = listener.local_addr().expect("addr").port();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let seen_ref = Arc::clone(&seen);
    tokio::spawn(async move {
        for raw in responses {
            let (mut sock, _) = listener.accept().await.expect("accept");
            let mut buf = [0u8; 8192];
            let n = sock.read(&mut buf).await.unwrap_or(0);
            seen_ref
                .lock()
                .unwrap()
                .push(String::from_utf8_lossy(&buf[..n]).to_string());
            sock.write_all(raw.as_bytes()).await.expect("write");
            sock.shutdown().await.expect("shutdown");
        }
    });
    (port, seen)
}

async fn start_bridge(ollama_port: u16) -> u16 {
    let cfg = OllamaConfig::try_new("127.0.0.1", ollama_port, "llama3.2:3b").unwrap();
    let server = LocalBridgeServer::bind(0, cfg).await.expect("bind bridge");
    let port = server.local_addr().expect("local addr").port();
    tokio::spawn(async move {
        let _ = server.serve().await;
    });
    port
}

pub(super) async fn send_raw(port: u16, req: &str) -> (u16, String) {
    let mut stream = TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("connect");
    stream.write_all(req.as_bytes()).await.expect("write req");
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await.expect("read resp");
    let text = String::from_utf8_lossy(&buf).to_string();
    let (head, body) = text.split_once("\r\n\r\n").expect("http split");
    let status = head
        .lines()
        .next()
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .parse()
        .unwrap();
    (status, body.to_string())
}

pub(super) fn ollama_http(status: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

pub(super) async fn closed_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("ephemeral");
    listener.local_addr().expect("addr").port()
}

#[tokio::test]
async fn health_endpoint_auto_discovers_installed_model_and_reports_503_when_down() {
    let tags = ollama_http(
        "200 OK",
        r#"{"models":[{"name":"nomic-embed-text"},{"name":"gemma3:4b"}]}"#,
    );
    let (ollama_port, _) = spawn_ollama_stub(vec![tags]).await;
    let bridge_port = start_bridge(ollama_port).await;
    let (status, body) = send_raw(
        bridge_port,
        "GET /api/health HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n",
    )
    .await;
    assert_eq!(status, 200);
    assert!(body.contains("\"status\":\"available\""));
    assert!(body.contains("\"model\":\"gemma3:4b\""));

    let dead_port = closed_port().await;
    let down_bridge = start_bridge(dead_port).await;
    let (err_status, err_body) = send_raw(
        down_bridge,
        "GET /api/health HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n",
    )
    .await;
    assert_eq!(err_status, 503);
    assert!(err_body.contains("\"status\":\"unavailable\""));
}

#[tokio::test]
async fn tutor_reply_forwards_multi_turn_history_and_extracts_follow_up() {
    let tags = ollama_http("200 OK", r#"{"models":[{"name":"qwen2.5:7b"}]}"#);
    let gen = ollama_http(
        "200 OK",
        r#"{"response":"{\"spokenReply\":\"I drank iced espresso too! Was the roast dark or fruity?\",\"correction\":\"Say 'I drank' instead of 'I drinked'.\"}"}"#,
    );
    let (ollama_port, seen) = spawn_ollama_stub(vec![tags, gen]).await;
    let bridge_port = start_bridge(ollama_port).await;
    let payload = r#"{"history":[{"role":"User","text":"I love coffee."},{"role":"Tutor","text":"How do you take it?"}],"userText":"Yesterday I drinked an iced espresso.","correctionMode":"Gentle"}"#;
    let req = format!("POST /api/tutor/reply HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{payload}", payload.len());
    let (status, body) = send_raw(bridge_port, &req).await;
    assert_eq!(status, 200);
    assert!(body.contains("\"model\":\"qwen2.5:7b\""));
    assert!(body.contains("\"followUpPrompt\":\"Was the roast dark or fruity?\""));
    assert!(seen.lock().unwrap()[1].contains("User: I love coffee."));
    assert!(seen.lock().unwrap()[1].contains("Tutor: How do you take it?"));
}

#[tokio::test]
async fn tutor_reply_returns_explicit_errors_when_ollama_down_or_input_empty() {
    let dead_port = closed_port().await;
    let bridge_port = start_bridge(dead_port).await;
    let payload = r#"{"history":[],"userText":"Hello there","correctionMode":"Natural"}"#;
    let req = format!(
        "POST /api/tutor/reply HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\n\r\n{payload}",
        payload.len()
    );
    let (status, body) = send_raw(bridge_port, &req).await;
    assert_eq!(status, 503);
    assert!(body.contains("\"error\""));
    assert!(!body.contains("spokenReply"));

    let bad = r#"{"userText":"   "}"#;
    let bad_req = format!(
        "POST /api/tutor/reply HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\n\r\n{bad}",
        bad.len()
    );
    let (bad_status, _) = send_raw(bridge_port, &bad_req).await;
    assert_eq!(bad_status, 400);

    let (opt_status, _) = send_raw(
        bridge_port,
        "OPTIONS /api/tutor/reply HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n",
    )
    .await;
    assert_eq!(opt_status, 204);
}

#[tokio::test]
async fn providers_endpoints_configure_cloud_opt_in_and_mask_runtime_keys() {
    let dead_port = closed_port().await;
    let bridge_port = start_bridge(dead_port).await;
    let patch = r#"{"cloudOptIn":true,"preferredProvider":"groq","apiKeys":{"groq":"gsk_secret_token_99"}}"#;
    let req = format!("POST /api/providers/config HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\n\r\n{patch}", patch.len());
    let (status, body) = send_raw(bridge_port, &req).await;
    assert_eq!(status, 200);
    assert!(body.contains("\"cloudOptIn\":true"));
    assert!(body.contains("\"preferredProvider\":\"groq\""));
    assert!(body.contains("\"keySource\":\"runtime\""));
    assert!(!body.contains("gsk_secret_token_99"));

    let (g_status, g_body) = send_raw(
        bridge_port,
        "GET /api/providers HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n",
    )
    .await;
    assert_eq!(g_status, 200);
    assert!(!g_body.contains("gsk_secret_token_99"));
}
