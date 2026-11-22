//! Adversarial regression tests for SSE error/null handling, mid-stream breakers, and Ollama fallback.

use crate::ai::{
    catalog::{ProviderId, ProviderPreference},
    cloud_stream_extract::extract_sse_error,
    ollama::{OllamaAiProvider, OllamaConfig},
    provider::{AiProvider, TutorStreamItem},
    router::TutorRouter,
    router_tests::{stub, tx},
    router_types::RouterConfigPatch,
};
use crate::domain::CorrectionMode;
use crate::modules::tutor::feedback::parse_tutor_response_or_fallback;
use std::collections::HashMap;

fn sse_200(body: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

fn http_resp(status: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

async fn build_two_cloud_router(p_groq: u16, p_gem: u16) -> TutorRouter {
    let router = TutorRouter::new(OllamaConfig::try_new("127.0.0.1", 1, "llama3").unwrap());
    router.set_endpoint_override(ProviderId::Groq, format!("http://127.0.0.1:{p_groq}/chat"));
    router.set_endpoint_override(
        ProviderId::Gemini,
        format!("http://127.0.0.1:{p_gem}/models"),
    );
    let patch = RouterConfigPatch {
        cloud_opt_in: Some(true),
        preferred_provider: Some(ProviderPreference::Auto),
        api_keys: HashMap::from([
            (ProviderId::Groq, "gsk_test".into()),
            (ProviderId::Gemini, "AIza_test".into()),
        ]),
        ..Default::default()
    };
    router.apply_config_patch(patch).await.unwrap();
    router
}

#[test]
fn sse_error_ignores_null_and_maps_gemini_grpc_statuses() {
    assert!(
        extract_sse_error(r#"{"choices":[{"delta":{"content":"Hi"}}],"error":null}"#).is_none()
    );
    let quota = extract_sse_error(r#"{"error":{"status":"RESOURCE_EXHAUSTED","message":"Quota"}}"#)
        .unwrap();
    assert_eq!(quota.status, Some(429));
    let unavail =
        extract_sse_error(r#"{"error":{"status":"UNAVAILABLE","message":"Busy"}}"#).unwrap();
    assert_eq!(unavail.status, Some(503));
    let str_err = extract_sse_error(r#"{"error":"upstream overloaded"}"#).unwrap();
    assert!(str_err.message.contains("upstream overloaded"));
}

#[tokio::test]
async fn empty_and_midstream_sse_failures_trip_circuit_breaker_cooldown() {
    let empty_sse =
        sse_200("data: {\"choices\":[{\"delta\":{\"content\":\"\"}}]}\n\ndata: [DONE]\n\n");
    let mid_fail_sse = sse_200(concat!(
        "data: {\"choices\":[{\"delta\":{\"content\":\"{\\\"spokenReply\\\":\\\"Sentence one! \"}}]}\n\n",
        "data: {\"error\":{\"code\":503,\"message\":\"Upstream dropped\"}}\n\n"
    ));
    let gem_ok = sse_200(
        "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"{\\\"spokenReply\\\":\\\"Gemini ok!\\\"}\"}]}}]}\n\n",
    );
    let (p_groq, _) = stub(vec![empty_sse]).await;
    let (p_gem, _) = stub(vec![gem_ok.clone()]).await;
    let r1 = build_two_cloud_router(p_groq, p_gem).await;
    let mut s1 = r1
        .stream_routed_reply(&[], &tx("Hi"), CorrectionMode::Natural)
        .await
        .unwrap();
    while s1.recv().await.is_some() {}
    let groq1 = r1
        .providers_snapshot()
        .await
        .providers
        .into_iter()
        .find(|p| p.id == ProviderId::Groq)
        .unwrap();
    assert!(
        groq1.cooldown_secs > 0,
        "empty post-200 SSE stream must cool down broken provider"
    );

    let (p_groq2, _) = stub(vec![mid_fail_sse]).await;
    let (p_gem2, _) = stub(vec![gem_ok]).await;
    let r2 = build_two_cloud_router(p_groq2, p_gem2).await;
    let mut s2 = r2
        .stream_routed_reply(&[], &tx("Hi"), CorrectionMode::Natural)
        .await
        .unwrap();
    assert!(matches!(
        s2.recv().await.unwrap().unwrap(),
        TutorStreamItem::Sentence(_)
    ));
    assert!(s2.recv().await.unwrap().is_err());
    let groq2 = r2
        .providers_snapshot()
        .await
        .providers
        .into_iter()
        .find(|p| p.id == ProviderId::Groq)
        .unwrap();
    assert!(
        groq2.cooldown_secs > 0,
        "mid-stream 503 after sentence 1 must trip cooldown"
    );
}

#[tokio::test]
async fn ollama_retries_without_think_on_http_400_and_surfaces_http_500_body() {
    let err_400 = http_resp(
        "400 Bad Request",
        r#"{"error":"model does not support think"}"#,
    );
    let ok_json = http_resp(
        "200 OK",
        r#"{"response":"{\"spokenReply\":\"Fallback ok!\"}"}"#,
    );
    let err_500 = http_resp(
        "500 Internal Server Error",
        r#"{"error":"CUDA out of memory"}"#,
    );
    let stream_ok = "HTTP/1.1 200 OK\r\nContent-Type: application/x-ndjson\r\nContent-Length: 55\r\nConnection: close\r\n\r\n{\"response\":\"{\\\"spokenReply\\\":\\\"Stream ok!\\\"}\",\"done\":true}\n".to_string();
    let (port, seen) = stub(vec![err_400.clone(), ok_json, err_400, stream_ok, err_500]).await;

    let provider = OllamaAiProvider::with_local_http("127.0.0.1", port, "gpt-oss:20b").unwrap();
    let r = provider
        .generate_tutor_reply(&[], &tx("Hi"), CorrectionMode::Natural)
        .await
        .unwrap();
    assert_eq!(r.spoken_reply, "Fallback ok!");

    let mut s = provider
        .stream_routed_reply(&[], &tx("Hi"), CorrectionMode::Natural)
        .await
        .unwrap();
    let mut final_text = String::new();
    while let Some(it) = s.recv().await {
        if let TutorStreamItem::Final(f) = it.unwrap() {
            final_text = f.reply.spoken_reply;
        }
    }
    assert_eq!(final_text, "Stream ok!");
    let reqs = seen.lock().unwrap().clone();
    assert!(reqs[0].contains("\"think\":false") && !reqs[1].contains("\"think\""));
    assert!(reqs[2].contains("\"think\":false") && !reqs[3].contains("\"think\""));

    let err = provider
        .stream_routed_reply(&[], &tx("Hi"), CorrectionMode::Natural)
        .await
        .unwrap_err();
    assert!(
        err.user_message.contains("CUDA out of memory"),
        "got: {}",
        err.user_message
    );
}

#[test]
fn truncated_tutor_json_recovers_spoken_reply_instead_of_raw_braces() {
    let truncated = r#"{"spokenReply":"Nice work on your coffee order! Would you like oat milk?","correction":"Try saying 'I would like' instead of"#;
    let reply = parse_tutor_response_or_fallback(truncated, CorrectionMode::Detailed).unwrap();
    assert_eq!(
        reply.spoken_reply,
        "Nice work on your coffee order! Would you like oat milk?"
    );
    assert!(!reply.spoken_reply.contains("spokenReply"));
}
