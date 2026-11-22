//! Edge-case tests for inbound localhost WebSockets: duplicate Origin rejection,
//! immediate barge-in cancellation races, and post-utterance binary PCM bursts.

use crate::ai::{mock::MockAiProvider, ollama::OllamaConfig, router::TutorRouter};
use crate::database::{connection::init_sqlite_pool, repository::SessionRepository};
use crate::domain::{ProviderHealth, SessionError, Transcript};
use crate::local_server::tests::ollama_http;
use crate::local_server::tests_cycling::{http_json, read_req, setup_bridge};
use crate::local_server::tests_ws::{recv_ws_json, ws_connect_with_origin};
use crate::local_server::LocalBridgeServer;
use crate::modules::conversation::{manager::SessionPipelineAdapters, SpeakingSessionHost};
use crate::modules::speech::{
    audio_capture::MockAudioInput, playback::MockAudioOutput, utterance::CompletedUtterance,
};
use crate::stt::provider::SpeechToText;
use crate::tts::mock::MockTtsProvider;
use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{client::IntoClientRequest, http::HeaderValue, Error as WsError, Message},
};

struct SlowStt(Arc<AtomicUsize>);

#[async_trait]
impl SpeechToText for SlowStt {
    async fn health_check(&self) -> ProviderHealth {
        ProviderHealth::Available
    }
    async fn transcribe(&self, _: &CompletedUtterance) -> Result<Transcript, SessionError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(25)).await;
        Ok(Transcript::new("Only first burst.", "en", 600, Some(0.94)).unwrap())
    }
}

fn pcm16(sample: i16, count: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(count * 2);
    for _ in 0..count {
        out.extend_from_slice(&sample.to_le_bytes());
    }
    out
}

async fn spawn_slow_ollama() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        while let Ok((mut sock, _)) = listener.accept().await {
            tokio::spawn(async move {
                let req = read_req(&mut sock).await;
                let body = if req.contains("/api/tags") {
                    r#"{"models":[{"name":"qwen2.5:7b"}]}"#
                } else if req.contains("Slow cancelled turn") {
                    tokio::time::sleep(Duration::from_millis(40)).await;
                    r#"{"response":"{\"spokenReply\":\"Should never commit.\"}"}"#
                } else {
                    r#"{"response":"{\"spokenReply\":\"Fresh turn committed.\"}"}"#
                };
                let _ = sock.write_all(ollama_http("200 OK", body).as_bytes()).await;
                let _ = sock.shutdown().await;
            });
        }
    });
    port
}

#[tokio::test]
async fn ws_rejects_duplicate_origin_headers_and_inactive_sessions() {
    let ol = spawn_slow_ollama().await;
    let bridge = setup_bridge(ol, vec![]).await;
    let sid = http_json(bridge, "POST", "/api/sessions", "{}").await.2["sessionId"]
        .as_str()
        .unwrap()
        .to_string();

    let url = format!("ws://127.0.0.1:{bridge}/api/sessions/{sid}/ws");
    let mut req = url.into_client_request().unwrap();
    req.headers_mut()
        .append("Origin", HeaderValue::from_static("http://localhost:5173"));
    req.headers_mut()
        .append("Origin", HeaderValue::from_static("https://evil.example"));
    match connect_async(req).await {
        Err(WsError::Http(resp)) => assert_eq!(resp.status().as_u16(), 403),
        other => panic!("expected 403 for duplicate Origin headers, got {other:?}"),
    }

    http_json(bridge, "POST", &format!("/api/sessions/{sid}/stop"), "").await;
    assert_eq!(
        ws_connect_with_origin(bridge, &sid, Some("http://localhost:5173"))
            .await
            .unwrap_err(),
        409
    );
}

#[tokio::test]
async fn ws_immediate_barge_in_prevents_stale_turn_execution_and_persistence() {
    let ol = spawn_slow_ollama().await;
    let bridge = setup_bridge(ol, vec![]).await;
    let sid = http_json(bridge, "POST", "/api/sessions", "{}").await.2["sessionId"]
        .as_str()
        .unwrap()
        .to_string();
    let mut ws = ws_connect_with_origin(bridge, &sid, Some("http://localhost:5173"))
        .await
        .unwrap();
    let _ = recv_ws_json(&mut ws).await;

    for msg in [
        json!({"type": "StartTurn", "userText": "Slow cancelled turn"}),
        json!({"type": "BargeIn"}),
        json!({"type": "StartTurn", "userText": "Fresh after immediate barge"}),
    ] {
        ws.send(Message::Text(msg.to_string().into()))
            .await
            .unwrap();
    }
    loop {
        let ev: Value = recv_ws_json(&mut ws).await;
        if ev["type"] == "turn_complete" {
            assert_eq!(ev["userTurn"]["text"], "Fresh after immediate barge");
            break;
        }
    }
    tokio::time::sleep(Duration::from_millis(80)).await;
    let hist = http_json(bridge, "GET", &format!("/api/sessions/{sid}"), "")
        .await
        .2;
    let turns = hist["turns"].as_array().unwrap();
    assert_eq!(
        turns.len(),
        2,
        "stale cancelled turn leaked into DB: {turns:?}"
    );
}

#[tokio::test]
async fn ws_audio_burst_after_vad_utterance_never_spawns_duplicate_turn() {
    let stt_calls = Arc::new(AtomicUsize::new(0));
    let pool = init_sqlite_pool("sqlite::memory:").await.unwrap();
    let adapters = SessionPipelineAdapters {
        audio_input: Arc::new(MockAudioInput::default()),
        stt: Arc::new(SlowStt(stt_calls.clone())),
        ai: Arc::new(MockAiProvider::default()),
        tts: Arc::new(MockTtsProvider::default()),
        audio_output: Arc::new(MockAudioOutput::new()),
    };
    let host = Arc::new(SpeakingSessionHost::new(
        adapters,
        Some(SessionRepository::new(pool)),
    ));
    let ol = OllamaConfig::try_new("127.0.0.1", 1, "llama3").unwrap();
    let srv = LocalBridgeServer::bind_with_host(0, Arc::new(TutorRouter::new(ol)), host)
        .await
        .unwrap();
    let port = srv.local_addr().unwrap().port();
    tokio::spawn(async move {
        let _ = srv.serve().await;
    });

    let sid = http_json(port, "POST", "/api/sessions", "{}").await.2["sessionId"]
        .as_str()
        .unwrap()
        .to_string();
    let mut ws = ws_connect_with_origin(port, &sid, Some("http://localhost:5173"))
        .await
        .unwrap();
    let _ = recv_ws_json(&mut ws).await;

    for (amp, n) in [(8_000, 4_800), (0, 17_600), (8_000, 4_800), (0, 17_600)] {
        ws.send(Message::Binary(pcm16(amp, n).into()))
            .await
            .unwrap();
    }
    while recv_ws_json(&mut ws).await["type"] != "turn_complete" {}
    tokio::time::sleep(Duration::from_millis(60)).await;
    assert_eq!(stt_calls.load(Ordering::SeqCst), 1);
    while let Ok(Some(_)) = tokio::time::timeout(Duration::from_millis(20), ws.next()).await {}
    let hist = http_json(port, "GET", &format!("/api/sessions/{sid}"), "")
        .await
        .2;
    assert_eq!(hist["turns"].as_array().unwrap().len(), 2);
}
