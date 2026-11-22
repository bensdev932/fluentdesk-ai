//! Unit tests for Ollama streaming stall timeouts, background warmup preload, and thinking-model detection.

use crate::ai::{
    ollama::OllamaConfig,
    ollama_stream::{stream_ollama_completion_with_timeouts, StreamTimeouts},
    ollama_warmup::warm_local_model,
};
use crate::domain::{CorrectionMode, SessionErrorKind, Transcript};
use crate::modules::tutor::prompts::assemble_voice_tutor_prompt;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[tokio::test]
async fn ollama_stream_times_out_when_server_stalls_after_200_headers() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        let (mut sock, _) = listener.accept().await.unwrap();
        let mut buf = [0u8; 4096];
        let _ = sock.read(&mut buf).await;
        sock.write_all(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n")
            .await
            .unwrap();
        sock.flush().await.unwrap();
        tokio::time::sleep(Duration::from_secs(5)).await;
    });

    let cfg = OllamaConfig::try_new("127.0.0.1", port, "qwen3.5:4b").unwrap();
    let tx = Transcript::new("Hello there.", "en", 800, Some(0.95)).unwrap();
    let prompt = assemble_voice_tutor_prompt(&[], &tx, CorrectionMode::Natural);
    let started = Instant::now();
    let mut rx = stream_ollama_completion_with_timeouts(
        &cfg,
        &prompt,
        CorrectionMode::Natural,
        StreamTimeouts {
            first: Duration::from_millis(120),
            idle: Duration::from_millis(120),
        },
    )
    .await
    .unwrap();
    let err = rx.recv().await.unwrap().unwrap_err();
    assert_eq!(err.kind, SessionErrorKind::AiProviderUnavailable);
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "stalled Ollama stream must time out promptly"
    );
}

#[tokio::test]
async fn ollama_warmup_discovers_model_and_sends_empty_prompt_preload() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let seen_bg = Arc::clone(&seen);
    tokio::spawn(async move {
        let replies = [
            r#"{"models":[{"name":"qwen3.5:4b","capabilities":["completion","thinking"]}]}"#,
            r#"{"model":"qwen3.5:4b","done":true}"#,
        ];
        for body in replies {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 4096];
            let n = sock.read(&mut buf).await.unwrap_or(0);
            seen_bg
                .lock()
                .unwrap()
                .push(String::from_utf8_lossy(&buf[..n]).to_string());
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            sock.write_all(resp.as_bytes()).await.unwrap();
            let _ = sock.shutdown().await;
        }
    });

    let cfg = OllamaConfig::try_new("127.0.0.1", port, "llama3.2:3b").unwrap();
    let loaded = warm_local_model(&cfg, None).await.unwrap();
    assert_eq!(loaded, "qwen3.5:4b");
    let reqs = seen.lock().unwrap();
    assert_eq!(reqs.len(), 2);
    assert!(reqs[0].starts_with("GET /api/tags"));
    assert!(reqs[1].starts_with("POST /api/generate"));
    assert!(reqs[1].contains("\"model\":\"qwen3.5:4b\""));
    assert!(reqs[1].contains("\"prompt\":\"\""));
    assert!(reqs[1].contains("\"keep_alive\":\"10m\""));
    assert!(reqs[1].contains("\"num_ctx\":2048"));
}

#[test]
fn ollama_config_disables_thinking_for_known_thinking_families() {
    for model in [
        "qwen3.5:4b",
        "deepseek-r1:8b",
        "qwq:32b",
        "gpt-oss:20b",
        "magistral:latest",
    ] {
        let cfg = OllamaConfig::try_new("127.0.0.1", 11434, model).unwrap();
        assert!(cfg.disable_thinking(), "expected think:false for {model}");
    }
    let plain = OllamaConfig::try_new("127.0.0.1", 11434, "llama3.2:3b").unwrap();
    assert!(!plain.disable_thinking());
}

#[test]
fn sse_error_extracts_status_from_status_field_even_when_code_is_non_numeric() {
    use crate::ai::cloud_stream_extract::extract_sse_error;
    let err = extract_sse_error(
        r#"{"error":{"code":"service_unavailable","status":503,"message":"overloaded"}}"#,
    )
    .expect("parsed");
    assert_eq!(err.status, Some(503));
    assert_eq!(err.code.as_deref(), Some("service_unavailable"));
}

#[tokio::test]
async fn cloud_sse_stream_times_out_promptly_when_stalled_between_chunks() {
    use crate::ai::{cloud_http::CloudHttpRequest, cloud_http_stream::post_sse_stream_with_idle};
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        let (mut sock, _) = listener.accept().await.unwrap();
        let mut buf = [0u8; 2048];
        let _ = sock.read(&mut buf).await;
        let head = b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n\r\n";
        sock.write_all(head).await.unwrap();
        sock.flush().await.unwrap();
        tokio::time::sleep(Duration::from_secs(5)).await;
    });
    let started = Instant::now();
    let mut rx = post_sse_stream_with_idle(
        CloudHttpRequest {
            url: format!("http://127.0.0.1:{port}/sse"),
            secret_headers: vec![],
            extra_headers: vec![],
            body: "{}".into(),
            timeout_secs: 5,
        },
        Duration::from_millis(120),
    )
    .await
    .unwrap();
    let err = rx.recv().await.unwrap().unwrap_err();
    assert!(err.is_network);
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[tokio::test]
async fn ollama_warmup_tracker_recovers_after_failed_launch_attempt_when_forced() {
    use crate::ai::router::TutorRouter;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let hits = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let hits_bg = Arc::clone(&hits);
    tokio::spawn(async move {
        let replies = [
            ("503 Service Unavailable", r#"{"error":"starting"}"#),
            (
                "200 OK",
                r#"{"models":[{"name":"qwen3.5:4b","capabilities":["thinking"]}]}"#,
            ),
            ("200 OK", r#"{"model":"qwen3.5:4b","done":true}"#),
        ];
        for (status, body) in replies {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 4096];
            let _ = sock.read(&mut buf).await;
            hits_bg.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let resp = format!(
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            sock.write_all(resp.as_bytes()).await.unwrap();
            let _ = sock.shutdown().await;
        }
    });
    let cfg = OllamaConfig::try_new("127.0.0.1", port, "llama3.2:3b").unwrap();
    let router = TutorRouter::new(cfg);
    router.spawn_local_warmup_if_needed();
    tokio::time::sleep(Duration::from_millis(80)).await;
    assert_eq!(hits.load(std::sync::atomic::Ordering::SeqCst), 1);
    router.spawn_local_warmup_forced();
    tokio::time::sleep(Duration::from_millis(120)).await;
    assert_eq!(hits.load(std::sync::atomic::Ordering::SeqCst), 3);
}
