//! Cross-restart SQLite + DPAPI vault integration tests over `/api/providers` & `/api/preferences`.

use crate::ai::{catalog::ProviderId, ollama::OllamaConfig, router::TutorRouter};
use crate::database::connection::init_sqlite_pool_at_path;
use crate::database::repository::SessionRepository;
use crate::local_server::tests::{closed_port, ollama_http};
use crate::local_server::tests_cycling::{assert_no_secrets, http_json, prov, spawn_stub};
use crate::local_server::LocalBridgeServer;
use std::sync::Arc;

#[tokio::test]
async fn disabling_cloud_via_preferences_resets_pinned_cloud_provider_and_persists_across_restart()
{
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "fluentdesk_srv_prefs_persist_{}_{}",
        std::process::id(),
        nanos
    ));
    let db_path = dir.join("fluentdesk.db");
    let ol_cfg = OllamaConfig::try_new("127.0.0.1", closed_port().await, "llama3.2:3b").unwrap();

    let (gp, _) = spawn_stub(|_, _| {
        ollama_http(
            "200 OK",
            r#"{"choices":[{"message":{"content":"{\"spokenReply\":\"Groq persisted!\"}"}}]}"#,
        )
    })
    .await;

    let pool1 = init_sqlite_pool_at_path(&db_path).await.unwrap();
    let r1 = Arc::new(
        TutorRouter::new_with_persistence(
            ol_cfg.clone(),
            SessionRepository::new(pool1.clone()),
            dir.clone(),
        )
        .await
        .unwrap(),
    );
    let srv1 = LocalBridgeServer::bind_with_router_and_pool(0, r1, pool1.clone())
        .await
        .unwrap();
    let p1 = srv1.local_addr().unwrap().port();
    tokio::spawn(async move {
        let _ = srv1.serve().await;
    });

    let cfg = r#"{"cloudOptIn":true,"preferredProvider":"groq","modelOverrides":{"groq":"openai/gpt-oss-120b"},"apiKeys":{"groq":"gsk_persist_secret_88"}}"#;
    let (cs, craw, cjson) = http_json(p1, "POST", "/api/providers/config", cfg).await;
    assert_eq!(cs, 200);
    assert_eq!(cjson["preferredProvider"].as_str(), Some("groq"));
    assert_no_secrets(&craw, &["gsk_persist_secret_88"]);
    assert_eq!(
        http_json(
            p1,
            "PATCH",
            "/api/preferences",
            r#"{"correctionMode":"Detailed","speakingSpeed":1.5}"#
        )
        .await
        .0,
        200
    );
    assert!(!dir.join("router-prefs.json").exists());
    pool1.close().await;

    let pool2 = init_sqlite_pool_at_path(&db_path).await.unwrap();
    let r2 = Arc::new(
        TutorRouter::new_with_persistence(
            ol_cfg,
            SessionRepository::new(pool2.clone()),
            dir.clone(),
        )
        .await
        .unwrap(),
    );
    r2.set_endpoint_override(
        ProviderId::Groq,
        format!("http://127.0.0.1:{gp}/v1/chat/completions"),
    );
    let srv2 = LocalBridgeServer::bind_with_router_and_pool(0, r2, pool2.clone())
        .await
        .unwrap();
    let p2 = srv2.local_addr().unwrap().port();
    tokio::spawn(async move {
        let _ = srv2.serve().await;
    });

    let (gs, graw, gsnap) = http_json(p2, "GET", "/api/providers", "").await;
    assert_eq!(gs, 200);
    assert_eq!(gsnap["cloudOptIn"].as_bool(), Some(true));
    assert_eq!(gsnap["preferredProvider"].as_str(), Some("groq"));
    assert_eq!(
        prov(&gsnap, "groq")["selectedModel"].as_str(),
        Some("openai/gpt-oss-120b")
    );
    assert_eq!(prov(&gsnap, "groq")["keySource"].as_str(), Some("runtime"));
    assert_no_secrets(&graw, &["gsk_persist_secret_88"]);

    let (rs, _, rprefs) = http_json(p2, "GET", "/api/preferences", "").await;
    assert_eq!(rs, 200);
    assert_eq!(rprefs["correctionMode"].as_str(), Some("Detailed"));
    assert_eq!(rprefs["speakingSpeed"].as_f64(), Some(1.5));
    assert_eq!(rprefs["preferredProvider"].as_str(), Some("groq"));

    let (ts, traw, turn) = http_json(
        p2,
        "POST",
        "/api/tutor/reply",
        r#"{"history":[],"userText":"Hello after restart","correctionMode":"Natural"}"#,
    )
    .await;
    assert_eq!(ts, 200);
    assert_eq!(turn["provider"].as_str(), Some("groq"));
    assert_no_secrets(&traw, &["gsk_persist_secret_88"]);

    let (os, _, opted_out) =
        http_json(p2, "PATCH", "/api/preferences", r#"{"cloudOptIn":false}"#).await;
    assert_eq!(os, 200);
    assert_eq!(opted_out["cloudOptIn"].as_bool(), Some(false));
    assert_eq!(opted_out["preferredProvider"].as_str(), Some("auto"));
    let (_, _, after_prov) = http_json(p2, "GET", "/api/providers", "").await;
    assert_eq!(after_prov["preferredProvider"].as_str(), Some("auto"));

    assert!(!dir.join("router-prefs.json").exists());
    pool2.close().await;
    let _ = std::fs::remove_dir_all(&dir);
}
