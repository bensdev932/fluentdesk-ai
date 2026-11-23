//! Tests for `ProviderId::Freellmapi` defaults, consent gating, redaction, and routing.

use crate::ai::{
    catalog::{ProviderId, ProviderPreference, FREELMAPI_DEFAULT_URL},
    ollama::OllamaConfig,
    router::TutorRouter,
    router_tests::{ok_json, stub, tx},
    router_types::RouterConfigPatch,
};
use crate::domain::{CorrectionMode, SessionErrorKind};
use crate::telemetry::redact_sensitive_log_text;
use std::collections::HashMap;
use tokio::net::TcpListener;

#[test]
fn freellmapi_catalog_and_secret_redaction_match_contract() {
    assert!(ProviderId::Freellmapi.is_cloud());
    assert_eq!(ProviderId::Freellmapi.as_str(), "freellmapi");
    assert_eq!(ProviderId::Freellmapi.default_model(), "auto:fast");
    assert_eq!(
        ProviderId::Freellmapi.env_var_name(),
        Some("FREELMAPI_API_KEY")
    );
    assert_eq!(
        ProviderId::Freellmapi.resolve_endpoint_url(),
        FREELMAPI_DEFAULT_URL
    );
    assert!(ProviderId::Freellmapi.free_models().contains(&"auto:fast"));
    assert_eq!(
        ProviderPreference::Freellmapi.as_pinned(),
        Some(ProviderId::Freellmapi)
    );
    let scrubbed = redact_sensitive_log_text("auth failed with freellmapi-secret-token-12345");
    assert!(!scrubbed.contains("freellmapi-secret-token-12345"));
    assert!(scrubbed.contains("[REDACTED_SECRET]"));
}

#[tokio::test]
async fn freellmapi_respects_cloud_opt_in_and_routes_through_openai_compat() {
    let dead = TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let router = TutorRouter::new(OllamaConfig::try_new("127.0.0.1", dead, "llama3.2:3b").unwrap());
    let body = ok_json(
        r#"{"choices":[{"message":{"content":"{\"spokenReply\":\"Gateway ready! What would you like to order?\"}"}}]}"#,
    );
    let (port, seen) = stub(vec![body]).await;
    router.set_endpoint_override(
        ProviderId::Freellmapi,
        format!("http://127.0.0.1:{port}/v1/chat/completions"),
    );

    router
        .apply_config_patch(RouterConfigPatch {
            cloud_opt_in: Some(false),
            api_keys: HashMap::from([(ProviderId::Freellmapi, "freellmapi-live-key-999".into())]),
            ..Default::default()
        })
        .await
        .unwrap();

    let blocked = router
        .route_turn(
            &[],
            &tx("Hi"),
            CorrectionMode::Gentle,
            Some(ProviderPreference::Freellmapi),
            None,
        )
        .await
        .unwrap_err();
    assert_eq!(blocked.kind, SessionErrorKind::AiProviderUnavailable);

    router
        .apply_config_patch(RouterConfigPatch {
            cloud_opt_in: Some(true),
            preferred_provider: Some(ProviderPreference::Freellmapi),
            ..Default::default()
        })
        .await
        .unwrap();

    let routed = router
        .route_turn(
            &[],
            &tx("Give me coffee."),
            CorrectionMode::Gentle,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(routed.provider, ProviderId::Freellmapi);
    assert_eq!(routed.model, "auto:fast");
    assert!(routed.is_cloud);
    let req = &seen.lock().unwrap()[0];
    assert!(req.contains("Bearer freellmapi-live-key-999"));
    assert!(req.contains("\"model\":\"auto:fast\""));
}
