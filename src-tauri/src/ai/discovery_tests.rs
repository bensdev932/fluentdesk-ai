//! Unit and loopback tests for `OLLAMA_HOST` resolution and `/api/tags` model auto-discovery.

use crate::ai::{
    ollama::OllamaAiProvider,
    ollama_discovery::{discover_local_provider, parse_ollama_host_env, select_installed_model},
    provider::AiProvider,
};
use crate::domain::{CorrectionMode, SessionErrorKind, Transcript};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

async fn spawn_http_stub(
    responses: Vec<String>,
) -> (u16, std::sync::Arc<std::sync::Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen_ref = std::sync::Arc::clone(&seen);
    tokio::spawn(async move {
        for raw in responses {
            let (mut sock, _) = listener.accept().await.expect("accept");
            let mut buf = [0u8; 4096];
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

fn http_json(status: &str, body: &str) -> String {
    let len = body.len();
    format!("HTTP/1.1 {status}\r\nContent-Length: {len}\r\nConnection: close\r\n\r\n{body}")
}

#[test]
fn parses_ollama_host_env_and_normalizes_wildcard_to_loopback() {
    assert_eq!(
        parse_ollama_host_env(Some("0.0.0.0:11434")).unwrap(),
        ("127.0.0.1".into(), 11434)
    );
    assert_eq!(
        parse_ollama_host_env(Some("http://localhost:11435/")).unwrap(),
        ("localhost".into(), 11435)
    );
    assert_eq!(
        parse_ollama_host_env(Some("[::]:11434")).unwrap(),
        ("::1".into(), 11434)
    );
    assert_eq!(
        parse_ollama_host_env(None).unwrap(),
        ("127.0.0.1".into(), 11434)
    );
    assert!(parse_ollama_host_env(Some("127.0.0.1:0")).is_err());
}

#[test]
fn selects_preferred_model_or_skips_embeddings_for_chat_model() {
    let models = vec![
        "nomic-embed-text:latest".to_string(),
        "qwen2.5:7b".to_string(),
        "gemma3:4b".to_string(),
    ];
    assert_eq!(
        select_installed_model(&models, "qwen2.5"),
        Some("qwen2.5:7b".to_string())
    );
    assert_eq!(
        select_installed_model(&models, "llama3.2:3b"),
        Some("qwen2.5:7b".to_string())
    );
    assert_eq!(select_installed_model(&[], "llama3.2:3b"), None);
}

#[tokio::test]
async fn retries_with_discovered_model_when_default_model_returns_404() {
    let not_found = http_json("404 Not Found", r#"{"error":"model not found"}"#);
    let tags = http_json("200 OK", r#"{"models":[{"name":"qwen2.5:7b"}]}"#);
    let ok_reply = http_json(
        "200 OK",
        r#"{"response":"{\"spokenReply\":\"Rainy commutes are tough! Did you take the subway instead?\"}"}"#,
    );
    let (port, _) = spawn_http_stub(vec![not_found, tags, ok_reply]).await;
    let provider = OllamaAiProvider::with_local_http("127.0.0.1", port, "llama3.2:3b").unwrap();
    let tx = Transcript::new("It poured on my bike ride to work.", "en", 1000, Some(0.95)).unwrap();
    let reply = provider
        .generate_tutor_reply(&[], &tx, CorrectionMode::Natural)
        .await
        .expect("discovered model retry should succeed");
    assert!(reply.spoken_reply.contains("subway"));
    assert_eq!(
        reply.follow_up_prompt.as_deref(),
        Some("Did you take the subway instead?")
    );
}

#[tokio::test]
async fn discover_local_provider_errors_explicitly_when_no_models_installed() {
    let empty_tags = http_json("200 OK", r#"{"models":[]}"#);
    let (port, _) = spawn_http_stub(vec![empty_tags]).await;
    let cfg = crate::ai::ollama::OllamaConfig::try_new("127.0.0.1", port, "llama3.2:3b").unwrap();
    let err = discover_local_provider(&cfg)
        .await
        .err()
        .expect("empty models error");
    assert_eq!(err.kind, SessionErrorKind::AiProviderUnavailable);
}

#[tokio::test]
async fn sets_think_false_only_when_model_has_thinking_capability() {
    let tags = http_json(
        "200 OK",
        r#"{"models":[{"name":"qwen3.5:4b","capabilities":["completion","thinking"]}]}"#,
    );
    let ok = http_json(
        "200 OK",
        r#"{"response":"{\"spokenReply\":\"Nice! Did you enjoy the walk?\"}"}"#,
    );
    let (port, seen) = spawn_http_stub(vec![tags, ok]).await;
    let cfg = crate::ai::ollama::OllamaConfig::try_new("127.0.0.1", port, "llama3.2:3b").unwrap();
    let provider = discover_local_provider(&cfg).await.expect("discover");
    let tx = Transcript::new("I walked by the harbor.", "en", 900, Some(0.95)).unwrap();
    provider
        .generate_tutor_reply(&[], &tx, CorrectionMode::Natural)
        .await
        .unwrap();
    assert!(seen.lock().unwrap()[1].contains("\"think\":false"));
}
