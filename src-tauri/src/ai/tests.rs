//! Loopback HTTP and integration tests for `OllamaAiProvider` and session turns (`T029`, `T031`).

use crate::ai::{
    mock::MockAiProvider,
    ollama::{OllamaAiProvider, OllamaConfig},
    provider::{generate_with_cancellation, AiProvider},
};
use crate::database::{connection::init_sqlite_pool, repository::SessionRepository};
use crate::domain::*;
use crate::modules::conversation::StartSpeakingSessionConfig;
use crate::modules::{
    conversation::{
        cancellation::SessionCancellationToken,
        context::MaxContextTurns,
        manager::{SessionManager, SessionPipelineAdapters},
    },
    speech::{audio_capture::MockAudioInput, playback::MockAudioOutput},
};
use crate::stt::mock::MockSttProvider;
use crate::tts::{mock::MockTtsProvider, provider::SpeakingSpeed};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

async fn spawn_ollama_stub(responses: Vec<String>) -> (u16, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let seen_clone = Arc::clone(&seen);
    tokio::spawn(async move {
        for raw_reply in responses {
            let (mut sock, _) = listener.accept().await.expect("accept");
            let mut buf = [0u8; 4096];
            let n = sock.read(&mut buf).await.unwrap_or(0);
            seen_clone
                .lock()
                .unwrap()
                .push(String::from_utf8_lossy(&buf[..n]).to_string());
            sock.write_all(raw_reply.as_bytes()).await.expect("write");
            sock.shutdown().await.expect("shutdown");
        }
    });
    (port, seen)
}

fn http_json(status: &str, body: &str) -> String {
    let len = body.len();
    format!("HTTP/1.1 {status}\r\nContent-Length: {len}\r\nConnection: close\r\n\r\n{body}")
}

fn tags_ok() -> String {
    http_json("200 OK", r#"{"models":[{"name":"llama3.2:3b"}]}"#)
}

fn provider_at(port: u16) -> OllamaAiProvider {
    OllamaAiProvider::with_local_http("127.0.0.1", port, "llama3.2:3b").expect("cfg")
}

async fn setup_mgr(sid: &SessionId, port: u16) -> (SessionManager, SessionRepository) {
    let repo = SessionRepository::new(init_sqlite_pool("sqlite::memory:").await.expect("sqlite"));
    let adapters = SessionPipelineAdapters {
        audio_input: Arc::new(MockAudioInput::default()),
        stt: Arc::new(MockSttProvider::default()),
        ai: Arc::new(provider_at(port)),
        tts: Arc::new(MockTtsProvider::default()),
        audio_output: Arc::new(MockAudioOutput::new()),
    };
    let max = MaxContextTurns::try_new(6).unwrap();
    let speed = SpeakingSpeed::try_new(1.0).unwrap();
    let mut mgr = SessionManager::with_persistence(sid.clone(), max, speed, adapters, repo.clone());
    let cfg = StartSpeakingSessionConfig {
        correction_mode: CorrectionMode::Gentle,
        ..Default::default()
    };
    mgr.start_session(&cfg, "u1", 1_000).await.expect("start");
    (mgr, repo)
}

#[test]
fn ollama_config_enforces_localhost_and_non_empty_model() {
    assert!(OllamaConfig::try_new("api.openai.com", 11434, "llama3.2:3b").is_err());
    assert!(OllamaConfig::try_new("127.0.0.1", 0, "llama3.2:3b").is_err());
    assert!(OllamaConfig::try_new("127.0.0.1", 11434, "   ").is_err());
    let cfg = OllamaConfig::try_new("localhost", 11434, "llama3.2:3b").unwrap();
    assert_eq!(cfg.model_name(), "llama3.2:3b");
}

#[tokio::test]
async fn ollama_health_check_reports_available_misconfigured_or_unavailable() {
    let (ok, _) = spawn_ollama_stub(vec![tags_ok()]).await;
    let bad_tags = http_json("200 OK", r#"{"models":[{"name":"phi3"}]}"#);
    let (bad, _) = spawn_ollama_stub(vec![bad_tags]).await;
    let (err, _) = spawn_ollama_stub(vec![http_json("500 Internal Server Error", "{}")]).await;
    assert_eq!(
        provider_at(ok).health_check().await,
        ProviderHealth::Available
    );
    assert_eq!(
        provider_at(bad).health_check().await,
        ProviderHealth::Misconfigured
    );
    assert_eq!(
        provider_at(err).health_check().await,
        ProviderHealth::Unavailable
    );
}

#[tokio::test]
async fn ollama_decodes_chunked_responses_and_rejects_non_2xx_http_status() {
    let body = r#"{"response":"{\"spokenReply\":\"Great chunked reply!\"}"}"#;
    let hex = format!("{:x}", body.len());
    let chunked = format!(
        "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n{hex}\r\n{body}\r\n0\r\n\r\n"
    );
    let err_500 = http_json("500 Internal Server Error", "x");
    let (port, _) = spawn_ollama_stub(vec![chunked, err_500]).await;
    let provider = provider_at(port);
    let tx = Transcript::new("Testing chunked HTTP.", "en", 900, Some(0.95)).expect("tx");
    let reply = provider
        .generate_tutor_reply(&[], &tx, CorrectionMode::Gentle)
        .await
        .unwrap();
    assert_eq!(reply.spoken_reply, "Great chunked reply!");
    let err = provider
        .generate_tutor_reply(&[], &tx, CorrectionMode::Gentle)
        .await
        .unwrap_err();
    assert_eq!(err.kind, SessionErrorKind::AiInferenceFailed);
}

#[tokio::test]
async fn generate_with_cancellation_returns_cancelled_when_token_fires() {
    let mut tok = SessionCancellationToken::new();
    tok.cancel();
    let tx = Transcript::new("Hello tutor", "en", 1000, Some(0.9)).expect("tx");
    let ai = MockAiProvider::default();
    let res = generate_with_cancellation(&ai, &[], &tx, CorrectionMode::Gentle, &mut tok).await;
    assert_eq!(res.expect_err("cancel").kind, SessionErrorKind::Cancelled);
}

#[tokio::test]
async fn session_manager_retains_context_and_persists_turns_chronologically() {
    let r1 = http_json(
        "200 OK",
        r#"{"response":"{\"spokenReply\":\"I love sushi too!\"}"}"#,
    );
    let r2 = http_json(
        "200 OK",
        r#"{"response":"{\"spokenReply\":\"Twice a week!\"}"}"#,
    );
    let (port, seen) = spawn_ollama_stub(vec![tags_ok(), r1, r2]).await;
    let sid = SessionId::new("sess-turn-1").expect("sid");
    let (mut mgr, repo) = setup_mgr(&sid, port).await;
    let t1 = Transcript::new("I really love eating sushi.", "en", 1200, Some(0.95)).unwrap();
    let t2 = Transcript::new("Usually twice a week.", "en", 1100, Some(0.94)).unwrap();
    mgr.process_user_transcript(&t1, 2_000).await.unwrap();
    mgr.process_user_transcript(&t2, 3_000).await.unwrap();
    let saved = repo.list_turns_chronological(&sid).await.unwrap();
    assert_eq!(saved.len(), 4);
    assert_eq!(
        (saved[0].role, saved[3].role),
        (ConversationRole::User, ConversationRole::Tutor)
    );
    assert!(seen.lock().unwrap()[2].contains("eating sushi"));
}

#[tokio::test]
async fn session_manager_preserves_user_transcript_when_ai_provider_fails() {
    let err_503 = http_json("503 Service Unavailable", "busy");
    let (port, _) = spawn_ollama_stub(vec![tags_ok(), err_503]).await;
    let sid = SessionId::new("sess-fail-safe").expect("sid");
    let (mut mgr, repo) = setup_mgr(&sid, port).await;
    let tx = Transcript::new("My important spoken thought.", "en", 1500, Some(0.92)).unwrap();
    assert!(mgr.process_user_transcript(&tx, 5_000).await.is_err());
    let saved = repo.list_turns_chronological(&sid).await.unwrap();
    assert_eq!(
        (saved.len(), saved[0].text.as_str()),
        (1, "My important spoken thought.")
    );
}

#[tokio::test]
#[ignore = "Requires local Ollama daemon running with FLUENTDESK_OLLAMA_MODEL installed"]
async fn real_local_ollama_generates_tutor_reply_when_available() {
    let model = std::env::var("FLUENTDESK_OLLAMA_MODEL").unwrap_or_else(|_| "llama3.2:3b".into());
    let provider = OllamaAiProvider::with_local_http("127.0.0.1", 11434, model).expect("provider");
    assert_eq!(provider.health_check().await, ProviderHealth::Available);
    let tx = Transcript::new("Yesterday I go to the park.", "en", 1500, Some(0.95)).unwrap();
    let reply = provider
        .generate_tutor_reply(&[], &tx, CorrectionMode::Gentle)
        .await
        .unwrap();
    assert!(!reply.spoken_reply.is_empty());
}
