//! Regression tests for chunked UTF-8 split boundaries and Gemini whitespace-only SSE deltas.

use crate::ai::{
    catalog::{ProviderId, ProviderPreference},
    cloud_stream_extract::extract_gemini_delta,
    gemini::{extract_gemini_text, GeminiProvider},
    ollama::{OllamaAiProvider, OllamaConfig},
    provider::{AiProvider, TutorStreamItem},
    router::TutorRouter,
    router_tests::{stub, tx},
    router_types::RouterConfigPatch,
};
use crate::domain::CorrectionMode;
use crate::telemetry::RedactedSecret;
use std::collections::HashMap;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

fn chunk_bytes(slice: &[u8]) -> Vec<u8> {
    let mut out = format!("{:x}\r\n", slice.len()).into_bytes();
    out.extend_from_slice(slice);
    out.extend_from_slice(b"\r\n");
    out
}

#[tokio::test]
async fn ollama_chunked_stream_preserves_multibyte_utf8_split_across_http_chunks() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let line = "{\"response\":\"{\\\"spokenReply\\\":\\\"It\u{2019}s great \u{2014} caf\u{00e9} \u{1f60a}!\\\"}\",\"done\":true}\n";
    let raw = line.as_bytes().to_vec();
    let p_quote = line.find('\u{2019}').unwrap() + 1;
    let p_dash = line.find('\u{2014}').unwrap() + 2;
    let p_emoji = line.find('\u{1f60a}').unwrap() + 2;

    tokio::spawn(async move {
        let (mut sock, _) = listener.accept().await.unwrap();
        let mut buf = [0u8; 4096];
        let _ = sock.read(&mut buf).await;
        sock.write_all(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n")
            .await
            .unwrap();
        for slice in [
            &raw[..p_quote],
            &raw[p_quote..p_dash],
            &raw[p_dash..p_emoji],
            &raw[p_emoji..],
        ] {
            sock.write_all(&chunk_bytes(slice)).await.unwrap();
            sock.flush().await.unwrap();
            tokio::task::yield_now().await;
        }
        sock.write_all(b"0\r\n\r\n").await.unwrap();
        let _ = sock.shutdown().await;
    });

    let cfg = OllamaConfig::try_new("127.0.0.1", port, "llama3").unwrap();
    let provider = OllamaAiProvider::new(cfg);
    let mut stream = provider
        .stream_routed_reply(&[], &tx("Hello"), CorrectionMode::Off)
        .await
        .unwrap();
    let mut final_reply = None;
    while let Some(item) = stream.recv().await {
        if let TutorStreamItem::Final(r) = item.unwrap() {
            final_reply = Some(r.reply.spoken_reply);
        }
    }
    let spoken = final_reply.expect("expected final reply");
    assert_eq!(spoken, "It\u{2019}s great \u{2014} caf\u{00e9} \u{1f60a}!");
    assert!(!spoken.contains('\u{FFFD}'));
}

#[test]
fn gemini_delta_preserves_whitespace_while_non_streaming_rejects_blank() {
    let space = r#"{"candidates":[{"content":{"parts":[{"text":" "}]}}]}"#;
    let newline = r#"{"candidates":[{"content":{"parts":[{"text":"\n"}]}}]}"#;
    let thought_plus_space =
        r#"{"candidates":[{"content":{"parts":[{"thought":true,"text":"plan"},{"text":" "}]}}]}"#;
    let thought_only = r#"{"candidates":[{"content":{"parts":[{"thought":true,"text":"only"}]}}]}"#;
    let empty_part = r#"{"candidates":[{"content":{"parts":[{"text":""}]}}]}"#;

    assert_eq!(extract_gemini_delta(space).as_deref(), Some(" "));
    assert_eq!(extract_gemini_delta(newline).as_deref(), Some("\n"));
    assert_eq!(
        extract_gemini_delta(thought_plus_space).as_deref(),
        Some(" ")
    );
    assert_eq!(extract_gemini_delta(thought_only), None);
    assert_eq!(extract_gemini_delta(empty_part), None);
    assert!(extract_gemini_text(space).is_err());
    assert!(extract_gemini_text(newline).is_err());
    assert!(extract_gemini_text(thought_plus_space).is_err());
}

fn gemini_sse(chunks: &[&str]) -> String {
    let mut body = String::new();
    for text in chunks {
        let escaped = serde_json::to_string(text).unwrap();
        body.push_str(&format!(
            "data: {{\"candidates\":[{{\"content\":{{\"parts\":[{{\"text\":{escaped}}}]}}}}]}}\n\n"
        ));
    }
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

#[tokio::test]
async fn gemini_sse_stream_preserves_space_deltas_and_still_fails_over_on_all_whitespace() {
    let ok_sse = gemini_sse(&[
        "{\"spokenReply\":\"Hello",
        " ",
        "world!",
        " ",
        "How are you?\"}",
    ]);
    let (port, _) = stub(vec![ok_sse]).await;
    let gem = GeminiProvider::new(
        format!("http://127.0.0.1:{port}"),
        "gemini-2.5-flash",
        RedactedSecret::new("AIzaTest"),
    );
    let mut stream = gem
        .stream_routed_reply(&[], &tx("Hi"), CorrectionMode::Off)
        .await
        .unwrap();
    let mut sentences = Vec::new();
    let mut spoken = String::new();
    while let Some(item) = stream.recv().await {
        match item.unwrap() {
            TutorStreamItem::Sentence(s) => sentences.push(s),
            TutorStreamItem::Final(r) => spoken = r.reply.spoken_reply,
        }
    }
    assert_eq!(sentences, vec!["Hello world!", "How are you?"]);
    assert_eq!(spoken, "Hello world! How are you?");

    let blank_sse = gemini_sse(&[" ", "\n"]);
    let fallback_sse =
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: 75\r\nConnection: close\r\n\r\ndata: {\"choices\":[{\"delta\":{\"content\":\"{\\\"spokenReply\\\":\\\"Groq ok!\\\"}\"}}]}\n\n";
    let (p_gem, _) = stub(vec![blank_sse]).await;
    let (p_groq, _) = stub(vec![fallback_sse.to_string()]).await;
    let router = TutorRouter::new(OllamaConfig::try_new("127.0.0.1", 1, "llama3").unwrap());
    router.set_endpoint_override(
        ProviderId::Gemini,
        format!("http://127.0.0.1:{p_gem}/models"),
    );
    router.set_endpoint_override(ProviderId::Groq, format!("http://127.0.0.1:{p_groq}/chat"));
    router
        .apply_config_patch(RouterConfigPatch {
            cloud_opt_in: Some(true),
            preferred_provider: Some(ProviderPreference::Auto),
            api_keys: HashMap::from([
                (ProviderId::Gemini, "AIza_test".into()),
                (ProviderId::Groq, "gsk_test".into()),
            ]),
            ..Default::default()
        })
        .await
        .unwrap();
    let mut rs = router
        .stream_routed_reply(&[], &tx("Hi"), CorrectionMode::Off)
        .await
        .unwrap();
    let mut final_text = String::new();
    while let Some(item) = rs.recv().await {
        if let TutorStreamItem::Final(r) = item.unwrap() {
            final_text = r.reply.spoken_reply;
        }
    }
    assert_eq!(final_text, "Groq ok!");
}
