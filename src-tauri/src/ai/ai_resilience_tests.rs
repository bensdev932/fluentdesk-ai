//! Regression tests for post-200 SSE failover, numeric SSE error codes, and Ollama stream resilience.

use crate::ai::{
    catalog::{ProviderId, ProviderPreference},
    ollama::{OllamaAiProvider, OllamaConfig},
    provider::{AiProvider, TutorStreamItem},
    router::TutorRouter,
    router_types::RouterConfigPatch,
};
use crate::domain::{CorrectionMode, Transcript};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

fn tx(s: &str) -> Transcript {
    Transcript::new(s, "en", 1000, Some(0.95)).unwrap()
}

async fn spawn_raw_stub(responses: Vec<String>) -> (u16, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let seen_bg = Arc::clone(&seen);
    tokio::spawn(async move {
        for raw in responses {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 4096];
            let n = sock.read(&mut buf).await.unwrap_or(0);
            seen_bg
                .lock()
                .unwrap()
                .push(String::from_utf8_lossy(&buf[..n]).to_string());
            sock.write_all(raw.as_bytes()).await.unwrap();
            let _ = sock.shutdown().await;
        }
    });
    (port, seen)
}

fn sse_200(body: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

fn json_200(body: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

async fn collect_final(
    stream: &mut crate::ai::provider::TutorReplyStream,
) -> Result<crate::ai::router_types::RoutedTutorReply, crate::domain::SessionError> {
    while let Some(item) = stream.recv().await {
        if let TutorStreamItem::Final(routed) = item? {
            return Ok(routed);
        }
    }
    panic!("stream closed without Final");
}

async fn build_failover_router(p_groq: u16, p_gem: u16) -> TutorRouter {
    let router = TutorRouter::new(OllamaConfig::try_new("127.0.0.1", 1, "llama3").unwrap());
    router.set_endpoint_override(ProviderId::Groq, format!("http://127.0.0.1:{p_groq}/chat"));
    router.set_endpoint_override(
        ProviderId::Gemini,
        format!("http://127.0.0.1:{p_gem}/models"),
    );
    router
        .apply_config_patch(RouterConfigPatch {
            cloud_opt_in: Some(true),
            preferred_provider: Some(ProviderPreference::Auto),
            api_keys: HashMap::from([
                (ProviderId::Groq, "gsk_test".into()),
                (ProviderId::Gemini, "AIza_test".into()),
            ]),
            ..Default::default()
        })
        .await
        .unwrap();
    router
}

#[tokio::test]
async fn post_200_sse_numeric_429_fails_over_and_trips_cooldown() {
    let err_sse =
        sse_200("data: {\"error\":{\"code\":429,\"message\":\"Rate limit exceeded\"}}\n\n");
    let ok_sse = sse_200(
        "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"{\\\"spokenReply\\\":\\\"Gemini rescued the turn!\\\"}\"}]}}]}\n\n",
    );
    let (p_groq, _) = spawn_raw_stub(vec![err_sse]).await;
    let (p_gem, _) = spawn_raw_stub(vec![ok_sse]).await;
    let router = build_failover_router(p_groq, p_gem).await;

    let mut stream = router
        .stream_routed_reply(&[], &tx("Hello"), CorrectionMode::Natural)
        .await
        .expect("must fail over after post-200 SSE 429");
    let final_reply = collect_final(&mut stream)
        .await
        .expect("fallback stream must succeed");
    assert_eq!(final_reply.provider, ProviderId::Gemini);
    assert_eq!(final_reply.reply.spoken_reply, "Gemini rescued the turn!");
    assert!(final_reply.failover_note.is_some());

    let snap = router.providers_snapshot().await;
    let groq = snap
        .providers
        .iter()
        .find(|p| p.id == ProviderId::Groq)
        .unwrap();
    assert!(
        groq.cooldown_secs > 0,
        "in-band numeric 429 must trip Groq cooldown"
    );
}

#[tokio::test]
async fn post_200_empty_sse_stream_fails_over_to_next_provider() {
    let empty_sse =
        sse_200("data: {\"choices\":[{\"delta\":{\"content\":\"\"}}]}\n\ndata: [DONE]\n\n");
    let ok_sse = sse_200(
        "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"{\\\"spokenReply\\\":\\\"Fallback after empty stream!\\\"}\"}]}}]}\n\n",
    );
    let (p_groq, _) = spawn_raw_stub(vec![empty_sse]).await;
    let (p_gem, _) = spawn_raw_stub(vec![ok_sse]).await;
    let router = build_failover_router(p_groq, p_gem).await;

    let mut stream = router
        .stream_routed_reply(&[], &tx("Hello"), CorrectionMode::Natural)
        .await
        .expect("must fail over after empty SSE stream");
    let final_reply = collect_final(&mut stream)
        .await
        .expect("fallback stream must succeed");
    assert_eq!(final_reply.provider, ProviderId::Gemini);
    assert_eq!(
        final_reply.reply.spoken_reply,
        "Fallback after empty stream!"
    );
}

#[tokio::test]
async fn ollama_generate_includes_keep_alive_and_bounded_options_and_surfaces_stream_errors() {
    let ok = json_200(r#"{"response":"{\"spokenReply\":\"Hi there!\"}"}"#);
    let err_ndjson = "{\"error\":\"model requires more system memory\"}\n";
    let err_stream = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/x-ndjson\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{err_ndjson}",
        err_ndjson.len()
    );
    let (port, seen) = spawn_raw_stub(vec![ok, err_stream]).await;
    let provider = OllamaAiProvider::with_local_http("127.0.0.1", port, "qwen3.5:4b").unwrap();

    let reply = provider
        .generate_tutor_reply(&[], &tx("Hi"), CorrectionMode::Natural)
        .await
        .unwrap();
    assert_eq!(reply.spoken_reply, "Hi there!");
    let req0 = seen.lock().unwrap()[0].clone();
    assert!(
        req0.contains("\"keep_alive\":\"10m\""),
        "missing keep_alive: {req0}"
    );
    assert!(req0.contains("\"num_ctx\":2048"), "missing num_ctx: {req0}");
    assert!(
        req0.contains("\"num_predict\":384"),
        "missing num_predict: {req0}"
    );

    let mut stream = provider
        .stream_routed_reply(&[], &tx("Hi"), CorrectionMode::Natural)
        .await
        .unwrap();
    let err = collect_final(&mut stream).await.unwrap_err();
    let combined = format!(
        "{} {}",
        err.user_message,
        err.internal_detail.unwrap_or_default()
    );
    assert!(
        combined.to_ascii_lowercase().contains("memory"),
        "expected Ollama NDJSON error detail, got: {combined}"
    );
}
