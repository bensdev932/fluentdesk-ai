//! Tests for `401`/`403` invalid-key pausing, strict pinned cloud errors, and SQLite + DPAPI persistence.

use crate::ai::{
    catalog::{ProviderId, ProviderPreference},
    cloud_http::{build_reqwest, err_status, CloudHttpRequest},
    ollama::OllamaConfig,
    provider::generate_routed_with_cancellation,
    router::TutorRouter,
    router_cycling_tests::{dead_port, http_err, openai_ok, setup_three_cloud, view_for},
    router_tests::tx,
    router_types::RouterConfigPatch,
    secrets::KeySource,
};
use crate::database::{connection::init_sqlite_pool_at_path, repository::SessionRepository};
use crate::domain::{CorrectionMode, SessionErrorKind};
use crate::modules::conversation::cancellation::SessionCancellationToken;
use crate::telemetry::RedactedSecret;
use std::collections::HashMap;

#[tokio::test]
async fn auth_errors_mark_auth_invalid_while_404_skips_cooldown_in_pinned_mode() {
    let router = setup_three_cloud(
        vec![http_err("401 Unauthorized"), http_err("404 Not Found")],
        vec![http_err("403 Forbidden")],
        vec![openai_ok("OpenRouter OK")],
        dead_port().await,
    )
    .await;
    let r = router
        .route_turn(&[], &tx("Test auth"), CorrectionMode::Natural, None, None)
        .await
        .unwrap();
    assert_eq!(r.provider, ProviderId::Openrouter);
    let (gv, mv) = (
        view_for(&router, ProviderId::Groq).await,
        view_for(&router, ProviderId::Gemini).await,
    );
    assert!(gv.auth_invalid && gv.cooldown_secs == 0 && !gv.available);
    assert!(mv.auth_invalid && mv.cooldown_secs == 0 && !mv.available);
    router
        .apply_config_patch(RouterConfigPatch {
            api_keys: HashMap::from([(ProviderId::Groq, "gsk_fixed".into())]),
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(!view_for(&router, ProviderId::Groq).await.auth_invalid);
    assert!(view_for(&router, ProviderId::Gemini).await.auth_invalid);
    let err = router
        .route_turn(
            &[],
            &tx("Pinned"),
            CorrectionMode::Natural,
            Some(ProviderPreference::Groq),
            None,
        )
        .await
        .unwrap_err();
    assert_eq!(err.kind, SessionErrorKind::AiInferenceFailed);
    let g_after_404 = view_for(&router, ProviderId::Groq).await;
    assert!(!g_after_404.auth_invalid && g_after_404.cooldown_secs == 0);
    router
        .apply_config_patch(RouterConfigPatch {
            reset_cooldowns: true,
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(!view_for(&router, ProviderId::Gemini).await.auth_invalid);
}

#[tokio::test]
async fn persistent_router_reloads_sqlite_prefs_and_dpapi_secrets_without_json_prefs() {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let tag = format!("fluentdesk_router_persist_{}_{nanos}", std::process::id());
    let dir = std::env::temp_dir().join(tag);
    let db_path = dir.join("fluentdesk.db");
    let ol = OllamaConfig::try_new("127.0.0.1", dead_port().await, "llama3.2:3b").unwrap();

    let pool1 = init_sqlite_pool_at_path(&db_path).await.unwrap();
    let r1 = TutorRouter::new_with_persistence(
        ol.clone(),
        SessionRepository::new(pool1.clone()),
        dir.clone(),
    )
    .await
    .unwrap();
    r1.apply_config_patch(RouterConfigPatch {
        cloud_opt_in: Some(true),
        preferred_provider: Some(ProviderPreference::Gemini),
        model_overrides: HashMap::from([(ProviderId::Gemini, "gemini-3.5-flash".into())]),
        api_keys: HashMap::from([(ProviderId::Gemini, "AIzaPersistSecret".into())]),
        ..Default::default()
    })
    .await
    .unwrap();
    assert!(!dir.join("router-prefs.json").exists());
    pool1.close().await;

    let pool2 = init_sqlite_pool_at_path(&db_path).await.unwrap();
    let r2 =
        TutorRouter::new_with_persistence(ol, SessionRepository::new(pool2.clone()), dir.clone())
            .await
            .unwrap();
    let snap = r2.providers_snapshot().await;
    let json = serde_json::to_string(&snap).unwrap();
    assert!(snap.cloud_opt_in && snap.preferred_provider == ProviderPreference::Gemini);
    assert!(!json.contains("AIzaPersistSecret"));
    let gem = snap
        .providers
        .into_iter()
        .find(|p| p.id == ProviderId::Gemini)
        .unwrap();
    let expected = if cfg!(windows) {
        KeySource::Runtime
    } else {
        KeySource::None
    };
    assert_eq!(gem.selected_model, "gemini-3.5-flash");
    assert_eq!(gem.key_source, expected);

    let opt_out = RouterConfigPatch {
        cloud_opt_in: Some(false),
        ..Default::default()
    };
    r2.apply_config_patch(opt_out).await.unwrap();
    let after_opt_out = r2.providers_snapshot().await;
    assert!(!after_opt_out.cloud_opt_in);
    assert_eq!(after_opt_out.preferred_provider, ProviderPreference::Auto);
    assert!(!dir.join("router-prefs.json").exists());
    pool2.close().await;
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn ai_provider_generate_routed_reply_exposes_router_metadata_and_cancellation() {
    let router = setup_three_cloud(
        vec![http_err("429 Too Many Requests")],
        vec![http_err("429 Too Many Requests")],
        vec![openai_ok("Routed reply!")],
        dead_port().await,
    )
    .await;
    let mut tok = SessionCancellationToken::new();
    let routed = generate_routed_with_cancellation(
        &router,
        &[],
        &tx("Hello"),
        CorrectionMode::Natural,
        &mut tok,
    )
    .await
    .unwrap();
    assert_eq!(
        (routed.provider, routed.is_cloud, routed.model.as_str()),
        (ProviderId::Openrouter, true, "openrouter/free")
    );
    assert!(routed.failover_note.unwrap_or_default().contains("groq"));
    tok.cancel();
    let cancelled = generate_routed_with_cancellation(
        &router,
        &[],
        &tx("Again"),
        CorrectionMode::Natural,
        &mut tok,
    )
    .await
    .unwrap_err();
    assert_eq!(cancelled.kind, SessionErrorKind::Cancelled);
}

#[test]
fn reqwest_builder_puts_secrets_in_headers_and_redacts_errors() {
    let req_in = CloudHttpRequest {
        url: "https://api.groq.com/openai/v1/chat/completions".into(),
        secret_headers: vec![(
            "Authorization".into(),
            RedactedSecret::new("Bearer gsk_abc"),
        )],
        extra_headers: vec![],
        body: "{\"m\":1}".into(),
        timeout_secs: 12,
    };
    let (_, req) = build_reqwest(req_in, 5).unwrap();
    assert_eq!(
        req.headers().get("authorization").unwrap(),
        "Bearer gsk_abc"
    );
    let body = std::str::from_utf8(req.body().unwrap().as_bytes().unwrap()).unwrap();
    assert!(!req.url().as_str().contains("gsk_abc") && body == "{\"m\":1}");
    assert!(!err_status(401, "bad Bearer gsk_abcdefg")
        .message
        .contains("gsk_abcdefg"));
}
