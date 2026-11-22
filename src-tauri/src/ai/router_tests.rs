//! Loopback tests for Groq/OpenRouter/Gemini adapters, `cloud_opt_in`, secrets, and `429` failover.

use crate::ai::{
    catalog::{ProviderId, ProviderPreference},
    gemini::GeminiProvider,
    ollama::OllamaConfig,
    openai_compat::OpenAiCompatProvider,
    provider::AiProvider,
    router::TutorRouter,
    router_types::RouterConfigPatch,
};
use crate::domain::{
    ConversationRole, ConversationTurn, CorrectionMode, SessionErrorKind, SessionId, Transcript,
};
use crate::telemetry::RedactedSecret;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

pub(super) async fn stub(responses: Vec<String>) -> (u16, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let s = Arc::clone(&seen);
    tokio::spawn(async move {
        for raw in responses {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 4096];
            let n = sock.read(&mut buf).await.unwrap_or(0);
            s.lock()
                .unwrap()
                .push(String::from_utf8_lossy(&buf[..n]).into());
            sock.write_all(raw.as_bytes()).await.unwrap();
            sock.shutdown().await.unwrap();
        }
    });
    (port, seen)
}

pub(super) fn ok_json(body: &str) -> String {
    let n = body.len();
    format!("HTTP/1.1 200 OK\r\nContent-Length: {n}\r\nConnection: close\r\n\r\n{body}")
}

pub(super) fn tx(s: &str) -> Transcript {
    Transcript::new(s, "en", 1000, Some(0.95)).unwrap()
}

#[tokio::test]
async fn openai_compat_formats_history_and_caches_three_tier_400_fallback() {
    let e400 = "HTTP/1.1 400 Bad Request\r\nContent-Length: 3\r\n\r\nbad";
    let ok = ok_json(
        r#"{"choices":[{"message":{"content":"{\"spokenReply\":\"Ramen in Kyoto! Which shop did you try?\"}"}}]}"#,
    );
    let (port, seen) = stub(vec![e400.into(), e400.into(), ok.clone(), ok]).await;
    let url = format!("http://127.0.0.1:{port}/chat/completions");
    let key = RedactedSecret::new("sk-or-test-key");
    let p = OpenAiCompatProvider::new(ProviderId::Openrouter, url, "openrouter/free", key);
    let hist = ConversationTurn {
        id: "1".into(),
        session_id: SessionId::new("s1").unwrap(),
        role: ConversationRole::User,
        text: "I landed in Kyoto.".into(),
        correction: None,
        follow_up_prompt: None,
        timestamp_unix_ms: 100,
        stage_metadata: None,
    };
    let r = p
        .generate_tutor_reply(&[hist], &tx("Found ramen."), CorrectionMode::Gentle)
        .await
        .unwrap();
    assert_eq!(
        r.follow_up_prompt.as_deref(),
        Some("Which shop did you try?")
    );
    p.generate_tutor_reply(&[], &tx("Second turn."), CorrectionMode::Gentle)
        .await
        .unwrap();
    let reqs = seen.lock().unwrap();
    assert_eq!(reqs.len(), 4);
    assert!(reqs[0].contains("Bearer sk-or-test-key") && reqs[0].contains("Kyoto."));
    assert!(reqs[0].contains("\"reasoning\"") && reqs[0].contains("json_object"));
    assert!(!reqs[1].contains("\"reasoning\"") && reqs[1].contains("json_object"));
    assert!(!reqs[2].contains("\"reasoning\"") && !reqs[2].contains("json_object"));
    assert!(!reqs[3].contains("\"reasoning\"") && !reqs[3].contains("json_object"));
}

#[tokio::test]
async fn gemini_uses_header_auth_never_url_key_and_parses_response() {
    let ok = ok_json(
        r#"{"candidates":[{"content":{"parts":[{"text":"{\"spokenReply\":\"Morning runs help! How many miles did you run?\"}"}]}}]}"#,
    );
    let (port, seen) = stub(vec![ok]).await;
    let base = format!("http://127.0.0.1:{port}/v1beta/models");
    let g = GeminiProvider::new(
        base,
        "gemini-3.5-flash-lite",
        RedactedSecret::new("AIzaSyHeaderOnlyTest"),
    );
    let r = g
        .generate_tutor_reply(&[], &tx("Ran before work."), CorrectionMode::Natural)
        .await
        .unwrap();
    assert_eq!(
        r.follow_up_prompt.as_deref(),
        Some("How many miles did you run?")
    );
    let raw = &seen.lock().unwrap()[0];
    assert!(raw.contains("x-goog-api-key: AIzaSyHeaderOnlyTest") && !raw.contains("?key="));
}

#[tokio::test]
async fn router_blocks_cloud_without_opt_in_and_fails_over_on_429() {
    let dead = TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let router = TutorRouter::new(OllamaConfig::try_new("127.0.0.1", dead, "llama3.2:3b").unwrap());
    let (gp, _) = stub(vec![
        "HTTP/1.1 429 Too Many Requests\r\nContent-Length: 3\r\n\r\n429".into(),
    ])
    .await;
    let gem = ok_json(
        r#"{"candidates":[{"content":{"parts":[{"text":"{\"spokenReply\":\"Switching smoothly! What song did you play?\"}"}]}}]}"#,
    );
    let (mp, _) = stub(vec![gem]).await;
    router.set_endpoint_override(
        ProviderId::Groq,
        format!("http://127.0.0.1:{gp}/v1/chat/completions"),
    );
    router.set_endpoint_override(
        ProviderId::Gemini,
        format!("http://127.0.0.1:{mp}/v1beta/models"),
    );

    let keys = HashMap::from([
        (ProviderId::Groq, "gsk_test12345".into()),
        (ProviderId::Gemini, "AIzaTest12345".into()),
    ]);
    router
        .apply_config_patch(RouterConfigPatch {
            cloud_opt_in: Some(false),
            api_keys: keys,
            ..Default::default()
        })
        .await
        .unwrap();
    let blocked = router
        .route_turn(
            &[],
            &tx("Hi"),
            CorrectionMode::Gentle,
            Some(ProviderPreference::Groq),
            None,
        )
        .await
        .unwrap_err();
    assert_eq!(blocked.kind, SessionErrorKind::AiProviderUnavailable);

    let enable = RouterConfigPatch {
        cloud_opt_in: Some(true),
        preferred_provider: Some(ProviderPreference::Auto),
        ..Default::default()
    };
    router.apply_config_patch(enable).await.unwrap();
    let routed = router
        .route_turn(
            &[],
            &tx("Played guitar."),
            CorrectionMode::Gentle,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(
        (routed.provider, routed.is_cloud),
        (ProviderId::Gemini, true)
    );
    assert!(routed.failover_note.unwrap_or_default().contains("groq"));

    let snap = serde_json::to_string(&router.providers_snapshot().await).unwrap();
    assert!(!snap.contains("gsk_test12345") && !snap.contains("AIzaTest12345"));
}
