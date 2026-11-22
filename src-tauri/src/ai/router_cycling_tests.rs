//! Tests for round-robin cloud cycling, outage/network cooldowns, and Ollama fallback.

use crate::ai::{
    catalog::{ProviderId, ProviderPreference},
    ollama::OllamaConfig,
    router::TutorRouter,
    router_tests::{ok_json, stub, tx},
    router_types::{ProviderDescriptorView, RouterConfigPatch},
};
use crate::domain::CorrectionMode;
use std::collections::HashMap;
use tokio::net::TcpListener;

pub(super) async fn dead_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

pub(super) fn openai_ok(reply: &str) -> String {
    let inner = format!(r#"{{"spokenReply":"{reply}"}}"#).replace('"', "\\\"");
    ok_json(&format!(
        r#"{{"choices":[{{"message":{{"content":"{inner}"}}}}]}}"#
    ))
}

pub(super) fn gemini_ok(reply: &str) -> String {
    let inner = format!(r#"{{"spokenReply":"{reply}"}}"#).replace('"', "\\\"");
    ok_json(&format!(
        r#"{{"candidates":[{{"content":{{"parts":[{{"text":"{inner}"}}]}}}}]}}"#
    ))
}

pub(super) fn http_err(status: &str) -> String {
    format!("HTTP/1.1 {status}\r\nContent-Length: 3\r\nConnection: close\r\n\r\nerr")
}

pub(super) async fn view_for(router: &TutorRouter, id: ProviderId) -> ProviderDescriptorView {
    router
        .providers_snapshot()
        .await
        .providers
        .into_iter()
        .find(|p| p.id == id)
        .unwrap()
}

pub(super) async fn setup_three_cloud(
    groq: Vec<String>,
    gem: Vec<String>,
    or: Vec<String>,
    ollama_port: u16,
) -> TutorRouter {
    let ol = OllamaConfig::try_new("127.0.0.1", ollama_port, "llama3.2:3b").unwrap();
    let router = TutorRouter::new(ol);
    let ((gp, _), (mp, _), (op, _)) = (stub(groq).await, stub(gem).await, stub(or).await);
    router.set_endpoint_override(ProviderId::Groq, format!("http://127.0.0.1:{gp}/chat"));
    router.set_endpoint_override(ProviderId::Gemini, format!("http://127.0.0.1:{mp}/models"));
    router.set_endpoint_override(
        ProviderId::Openrouter,
        format!("http://127.0.0.1:{op}/chat"),
    );
    router
        .apply_config_patch(RouterConfigPatch {
            cloud_opt_in: Some(true),
            preferred_provider: Some(ProviderPreference::Auto),
            api_keys: HashMap::from([
                (ProviderId::Groq, "gsk_1".into()),
                (ProviderId::Gemini, "AIza_2".into()),
                (ProviderId::Openrouter, "sk_or_3".into()),
            ]),
            ..Default::default()
        })
        .await
        .unwrap();
    router
}

#[tokio::test]
async fn auto_cycles_round_robin_across_three_and_two_cloud_providers() {
    let router = setup_three_cloud(
        vec![openai_ok("G1"), openai_ok("G2"), openai_ok("G3")],
        vec![gemini_ok("M1")],
        vec![openai_ok("O1"), openai_ok("O2")],
        dead_port().await,
    )
    .await;
    let mut seq = Vec::new();
    for _ in 0..4 {
        let r = router
            .route_turn(&[], &tx("Hi"), CorrectionMode::Natural, None, None)
            .await
            .unwrap();
        seq.push(r.provider);
    }
    assert_eq!(
        seq,
        vec![
            ProviderId::Groq,
            ProviderId::Gemini,
            ProviderId::Openrouter,
            ProviderId::Groq
        ]
    );
    router
        .apply_config_patch(RouterConfigPatch {
            clear_keys: vec![ProviderId::Gemini],
            ..Default::default()
        })
        .await
        .unwrap();
    let r5 = router
        .route_turn(&[], &tx("Two"), CorrectionMode::Natural, None, None)
        .await
        .unwrap();
    let r6 = router
        .route_turn(&[], &tx("Three"), CorrectionMode::Natural, None, None)
        .await
        .unwrap();
    assert_eq!(
        (r5.provider, r6.provider),
        (ProviderId::Openrouter, ProviderId::Groq)
    );
}

#[tokio::test]
async fn outage_and_network_errors_cooldown_and_fall_back_to_ollama() {
    let tags = ok_json(r#"{"models":[{"name":"llama3.2:3b"}]}"#);
    let gen = ok_json(r#"{"response":"{\"spokenReply\":\"Local Ollama fallback ready!\"}"}"#);
    let (ol_port, _) = stub(vec![tags.clone(), gen.clone(), tags.clone(), tags, gen]).await;
    let router = setup_three_cloud(
        vec![http_err("503 Service Unavailable")],
        vec![],
        vec![http_err("429 Too Many Requests")],
        ol_port,
    )
    .await;
    let dead_gem = dead_port().await;
    router.set_endpoint_override(
        ProviderId::Gemini,
        format!("http://127.0.0.1:{dead_gem}/models"),
    );
    let r1 = router
        .route_turn(&[], &tx("Coffee"), CorrectionMode::Natural, None, None)
        .await
        .unwrap();
    assert_eq!((r1.provider, r1.is_cloud), (ProviderId::Ollama, false));
    let note = r1.failover_note.unwrap_or_default();
    assert!(note.contains("groq") && note.contains("gemini") && note.contains("openrouter"));
    let snap = router.providers_snapshot().await;
    for v in snap.providers.iter().filter(|p| p.is_cloud) {
        assert!(v.cooldown_secs > 0 && !v.available && !v.auth_invalid);
    }
    let r2 = router
        .route_turn(&[], &tx("Again"), CorrectionMode::Natural, None, None)
        .await
        .unwrap();
    assert!(r2.provider == ProviderId::Ollama && r2.failover_note.is_none());
    router
        .apply_config_patch(RouterConfigPatch {
            reset_cooldowns: true,
            ..Default::default()
        })
        .await
        .unwrap();
    let reset_snap = router.providers_snapshot().await;
    for v in reset_snap.providers.iter().filter(|p| p.is_cloud) {
        assert!(v.cooldown_secs == 0 && v.available);
    }
}
