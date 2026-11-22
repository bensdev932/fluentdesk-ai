//! Live TCP loopback incremental streaming tests for Ollama NDJSON, OpenAI SSE, Gemini SSE, and TutorRouter failover.

use crate::ai::{
    catalog::{ProviderId, ProviderPreference},
    gemini::GeminiProvider,
    ollama::{OllamaAiProvider, OllamaConfig},
    provider::{AiProvider, TutorReplyStream, TutorStreamItem},
    router::TutorRouter,
    router_types::RouterConfigPatch,
};
use crate::domain::{CorrectionMode, Transcript};
use crate::telemetry::RedactedSecret;
use std::collections::HashMap;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::watch;

fn tx(s: &str) -> Transcript {
    Transcript::new(s, "en", 1000, Some(0.95)).unwrap()
}

fn chunk(data: &str) -> String {
    format!("{:x}\r\n{data}\r\n", data.len())
}

async fn recv_first_sentence(stream: &mut TutorReplyStream) -> String {
    match tokio::time::timeout(Duration::from_secs(2), stream.recv())
        .await
        .expect("sentence 1 must arrive before server gate opens")
        .expect("stream open")
        .expect("valid item")
    {
        TutorStreamItem::Sentence(s) => s,
        TutorStreamItem::Final(_) => panic!("expected incremental Sentence before Final"),
    }
}

async fn recv_final(stream: &mut TutorReplyStream) -> crate::ai::router_types::RoutedTutorReply {
    while let Some(item) = stream.recv().await {
        if let TutorStreamItem::Final(r) = item.expect("valid stream item") {
            return r;
        }
    }
    panic!("missing Final item")
}

#[tokio::test]
async fn ollama_stream_emits_sentence_one_before_done_chunk_arrives() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let (gate_tx, mut gate_rx) = watch::channel(false);
    tokio::spawn(async move {
        let (mut sock, _) = listener.accept().await.unwrap();
        let mut buf = [0u8; 4096];
        let _ = sock.read(&mut buf).await;
        sock.write_all(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n")
            .await
            .unwrap();
        let p1 = chunk("{\"response\":\"{\\\"spokenReply\\\":\\\"Hi there! \",\"done\":false}\n");
        sock.write_all(p1.as_bytes()).await.unwrap();
        sock.flush().await.unwrap();
        let _ = gate_rx.wait_for(|v| *v).await;
        let p2 =
            chunk("{\"response\":\"How are you?\\\",\\\"correction\\\":null}\",\"done\":true}\n");
        sock.write_all(p2.as_bytes()).await.unwrap();
        sock.write_all(b"0\r\n\r\n").await.unwrap();
        let _ = sock.shutdown().await;
    });

    let cfg = OllamaConfig::try_new("127.0.0.1", port, "llama3").unwrap();
    let provider = OllamaAiProvider::new(cfg);
    let mut stream = provider
        .stream_routed_reply(&[], &tx("Hello"), CorrectionMode::Natural)
        .await
        .unwrap();
    assert_eq!(recv_first_sentence(&mut stream).await, "Hi there!");
    let _ = gate_tx.send(true);
    let final_reply = recv_final(&mut stream).await;
    assert_eq!(final_reply.reply.spoken_reply, "Hi there! How are you?");
    assert_eq!(
        final_reply.reply.follow_up_prompt.as_deref(),
        Some("How are you?")
    );
}

#[tokio::test]
async fn openai_compat_and_router_stream_sse_and_fail_over_on_429() {
    let l429 = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let p429 = l429.local_addr().unwrap().port();
    tokio::spawn(async move {
        let (mut sock, _) = l429.accept().await.unwrap();
        let mut buf = [0u8; 2048];
        let _ = sock.read(&mut buf).await;
        sock.write_all(b"HTTP/1.1 429 Too Many Requests\r\nContent-Length: 4\r\n\r\nbusy")
            .await
            .unwrap();
    });

    let lsse = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let psse = lsse.local_addr().unwrap().port();
    let (gate_tx, mut gate_rx) = watch::channel(false);
    tokio::spawn(async move {
        let (mut sock, _) = lsse.accept().await.unwrap();
        let mut buf = [0u8; 4096];
        let _ = sock.read(&mut buf).await;
        sock.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n\r\n")
            .await
            .unwrap();
        sock.write_all(b"data: {\"choices\":[{\"delta\":{\"content\":\"{\\\"spokenReply\\\":\\\"Great start! \"}}]}\n\n").await.unwrap();
        sock.flush().await.unwrap();
        let _ = gate_rx.wait_for(|v| *v).await;
        sock.write_all(b"data: {\"choices\":[{\"delta\":{\"content\":\"Keep going.\\\"}\"}}]}\n\ndata: [DONE]\n\n").await.unwrap();
        let _ = sock.shutdown().await;
    });

    let base_ollama = OllamaConfig::try_new("127.0.0.1", 1, "llama3").unwrap();
    let router = TutorRouter::new(base_ollama);
    router.set_endpoint_override(ProviderId::Groq, format!("http://127.0.0.1:{p429}/chat"));
    router.set_endpoint_override(
        ProviderId::Openrouter,
        format!("http://127.0.0.1:{psse}/chat"),
    );
    router
        .apply_config_patch(RouterConfigPatch {
            cloud_opt_in: Some(true),
            preferred_provider: Some(ProviderPreference::Auto),
            api_keys: HashMap::from([
                (ProviderId::Groq, "gsk_test123".into()),
                (ProviderId::Openrouter, "sk-or-test".into()),
            ]),
            ..Default::default()
        })
        .await
        .unwrap();

    let mut stream = router
        .stream_routed_reply(&[], &tx("Test SSE"), CorrectionMode::Natural)
        .await
        .unwrap();
    assert_eq!(recv_first_sentence(&mut stream).await, "Great start!");
    let _ = gate_tx.send(true);
    let final_reply = recv_final(&mut stream).await;
    assert_eq!(final_reply.provider, ProviderId::Openrouter);
    assert!(final_reply.failover_note.is_some());
    assert_eq!(final_reply.reply.spoken_reply, "Great start! Keep going.");
}

#[tokio::test]
async fn gemini_stream_parses_sse_chunks_incrementally() {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = l.local_addr().unwrap().port();
    let (gate_tx, mut gate_rx) = watch::channel(false);
    tokio::spawn(async move {
        let (mut sock, _) = l.accept().await.unwrap();
        let mut buf = [0u8; 4096];
        let _ = sock.read(&mut buf).await;
        sock.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n\r\n")
            .await
            .unwrap();
        sock.write_all(b"data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"{\\\"spokenReply\\\":\\\"Hello friend! \"}]}}]}\n\n").await.unwrap();
        sock.flush().await.unwrap();
        let _ = gate_rx.wait_for(|v| *v).await;
        sock.write_all(b"data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"How are you?\\\"}\"}]}}]}\n\n").await.unwrap();
        let _ = sock.shutdown().await;
    });

    let gem = GeminiProvider::new(
        format!("http://127.0.0.1:{port}"),
        "gemini-2.5-flash",
        RedactedSecret::new("AIzaTest"),
    );
    let mut stream = gem
        .stream_routed_reply(&[], &tx("Hi Gemini"), CorrectionMode::Off)
        .await
        .unwrap();
    assert_eq!(recv_first_sentence(&mut stream).await, "Hello friend!");
    let _ = gate_tx.send(true);
    let final_reply = recv_final(&mut stream).await;
    assert_eq!(final_reply.reply.spoken_reply, "Hello friend! How are you?");
}
