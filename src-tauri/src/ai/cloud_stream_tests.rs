//! Unit tests for Groq reasoning suppression flags and SSE error retry/redaction.

use crate::ai::catalog::ProviderId;
use crate::ai::cloud_error::CompatMode;
use crate::ai::openai_compat::OpenAiCompatProvider;
use crate::ai::provider::{AiProvider, TutorStreamItem};
use crate::ai::router_tests::{stub, tx};
use crate::domain::CorrectionMode::Natural;
use crate::telemetry::RedactedSecret;
use serde_json::json;

fn sse_res(data: &str) -> String {
    let body = format!("data: {data}\n\ndata: [DONE]\n\n");
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

#[test]
fn groq_qwen_and_deepseek_use_hidden_reasoning_format() {
    let mut p = json!({});
    CompatMode::Full.apply_to(&mut p, ProviderId::Groq, "qwen/qwen3-32b");
    assert!(
        p["reasoning_effort"] == "low"
            && p["reasoning_format"] == "hidden"
            && p.get("include_reasoning").is_none()
    );
}

#[tokio::test]
async fn sse_json_validate_failed_retries_without_json_and_errors_redact_secrets() {
    let jvf = sse_res(
        r#"{"error":{"type":"invalid_request_error","code":"json_validate_failed","message":"bad"}}"#,
    );
    let ok = sse_res(r#"{"choices":[{"delta":{"content":"{\"spokenReply\":\"Recovered!\"}"}}]}"#);
    let (p1, s1) = stub(vec![jvf, ok.clone(), ok]).await;
    let url1 = format!("http://127.0.0.1:{p1}/chat");
    let g = OpenAiCompatProvider::new(
        ProviderId::Groq,
        url1,
        "qwen/qwen3-32b",
        RedactedSecret::new("gsk_t"),
    );
    let mut st1 = g
        .stream_routed_reply(&[], &tx("Hi"), Natural)
        .await
        .unwrap();
    while let Some(it) = st1.recv().await {
        if let TutorStreamItem::Final(r) = it.unwrap() {
            assert_eq!(r.reply.spoken_reply, "Recovered!");
        }
    }
    assert!(g
        .stream_routed_reply(&[], &tx("T2"), Natural)
        .await
        .unwrap()
        .recv()
        .await
        .is_some());
    {
        let reqs = s1.lock().unwrap();
        assert!(
            reqs.len() == 3 && reqs[0].contains("json_object") && !reqs[1].contains("json_object")
        );
        assert!(reqs[1].contains("reasoning_format") && !reqs[2].contains("json_object"));
    }

    let rl = sse_res(
        r#"{"error":{"type":"rate_limit","code":"rate_limit_exceeded","message":"Bearer gsk_secret123"}}"#,
    );
    let (p2, _) = stub(vec![rl]).await;
    let g2 = OpenAiCompatProvider::new(
        ProviderId::Groq,
        format!("http://127.0.0.1:{p2}/chat"),
        "openai/gpt-oss-20b",
        RedactedSecret::new("gsk_t"),
    );
    let err = g2
        .stream_routed_reply(&[], &tx("Hi"), Natural)
        .await
        .unwrap()
        .recv()
        .await
        .unwrap()
        .unwrap_err();
    let det = err.internal_detail.unwrap_or_default();
    assert!(det.contains("rate_limit:") && !det.contains("gsk_secret123"));
}
