//! Loopback HTTP integration tests for cloud failover, AI failure turn retention,
//! immediate stop-cancellation, and file-backed SQLite persistence across restart.

use crate::ai::{catalog::ProviderId, ollama::OllamaConfig, router::TutorRouter};
use crate::database::connection::init_sqlite_pool_at_path;
use crate::local_server::tests::{closed_port, ollama_http};
use crate::local_server::tests_cycling::{
    assert_no_secrets, http_json, openai_ok, setup_bridge, spawn_stub,
};
use crate::local_server::LocalBridgeServer;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;

#[tokio::test]
async fn sessions_cloud_failover_error_preservation_and_mismatch() {
    let (gp, _) =
        spawn_stub(|_, _| ollama_http("429 Too Many Requests", r#"{"error":"limit"}"#)).await;
    let (op, _) = spawn_stub(|_, idx| {
        if idx == 0 {
            openai_ok("Cloud OK! Next?")
        } else {
            ollama_http("500 Internal Server Error", r#"{"error":"boom"}"#)
        }
    })
    .await;
    let bridge = setup_bridge(
        closed_port().await,
        vec![(ProviderId::Groq, gp), (ProviderId::Openrouter, op)],
    )
    .await;
    let cfg = r#"{"cloudOptIn":true,"preferredProvider":"auto","apiKeys":{"groq":"gsk_sess_1","openrouter":"sk-or-sess_2"}}"#;
    assert_eq!(
        http_json(bridge, "POST", "/api/providers/config", cfg)
            .await
            .0,
        200
    );
    let (ss, _, started) = http_json(
        bridge,
        "POST",
        "/api/sessions",
        r#"{"correctionMode":"Detailed","pushToTalk":true}"#,
    )
    .await;
    assert_eq!((ss, started["pushToTalk"].as_bool()), (200, Some(true)));
    let sid = started["sessionId"].as_str().unwrap().to_string();
    assert_eq!(
        http_json(
            bridge,
            "POST",
            "/api/sessions/wrong-id/turn",
            r#"{"userText":"Hi"}"#,
        )
        .await
        .0,
        409
    );

    let (t1s, t1raw, t1) = http_json(
        bridge,
        "POST",
        &format!("/api/sessions/{sid}/turn"),
        r#"{"userText":"Cloud turn"}"#,
    )
    .await;
    assert_eq!(
        (t1s, t1["provider"].as_str(), t1["isCloud"].as_bool()),
        (200, Some("openrouter"), Some(true))
    );
    assert!(t1["failoverNote"].as_str().unwrap_or("").contains("groq"));
    assert_no_secrets(&t1raw, &["gsk_sess_1", "sk-or-sess_2"]);

    let (t2s, _, _) = http_json(
        bridge,
        "POST",
        &format!("/api/sessions/{sid}/turn"),
        r#"{"userText":"Keep this user turn"}"#,
    )
    .await;
    assert_eq!(t2s, 503);
    assert_eq!(
        http_json(bridge, "GET", &format!("/api/sessions/{sid}/status"), "")
            .await
            .2["state"],
        "Error"
    );
    let (_, _, hist) = http_json(bridge, "GET", &format!("/api/sessions/{sid}"), "").await;
    assert_eq!(hist["turns"].as_array().unwrap().len(), 3);
    assert_eq!(
        hist["turns"][2]["text"].as_str(),
        Some("Keep this user turn")
    );
}

#[tokio::test]
async fn sessions_stop_cancels_hanging_llm_and_sqlite_persists_across_restart() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let hang_port = listener.local_addr().unwrap().port();
    let (entered_tx, entered_rx) = tokio::sync::oneshot::channel::<()>();
    tokio::spawn(async move {
        let mut notify = Some(entered_tx);
        while let Ok((mut sock, _)) = listener.accept().await {
            if let Some(tx) = notify.take() {
                let _ = tx.send(());
            }
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            let _ = sock.shutdown().await;
        }
    });
    let dir = std::env::temp_dir().join(format!("fluentdesk_srv_db_{}", std::process::id()));
    let db_path = dir.join("fluentdesk.db");
    let ol_cfg = OllamaConfig::try_new("127.0.0.1", closed_port().await, "llama3.2:3b").unwrap();
    let r1 = Arc::new(TutorRouter::new(ol_cfg.clone()));
    r1.set_endpoint_override(
        ProviderId::Groq,
        format!("http://127.0.0.1:{hang_port}/v1/chat/completions"),
    );
    r1.apply_config_patch(
        serde_json::from_str(
            r#"{"cloudOptIn":true,"preferredProvider":"groq","apiKeys":{"groq":"gsk_hang_1"}}"#,
        )
        .unwrap(),
    )
    .await
    .unwrap();
    let pool1 = init_sqlite_pool_at_path(&db_path).await.unwrap();
    let srv1 = LocalBridgeServer::bind_with_router_and_pool(0, r1, pool1.clone())
        .await
        .unwrap();
    let p1 = srv1.local_addr().unwrap().port();
    tokio::spawn(async move {
        let _ = srv1.serve().await;
    });

    let sid = http_json(p1, "POST", "/api/sessions", "{}").await.2["sessionId"]
        .as_str()
        .unwrap()
        .to_string();
    let turn_path = format!("/api/sessions/{sid}/turn");
    let turn_fut = tokio::spawn(async move {
        http_json(p1, "POST", &turn_path, r#"{"userText":"Wait forever"}"#).await
    });
    let _ = entered_rx.await;
    let stop_res = http_json(p1, "POST", &format!("/api/sessions/{sid}/stop"), "").await;
    assert_eq!(
        (stop_res.0, stop_res.2["state"].as_str()),
        (200, Some("Ended"))
    );
    assert_eq!(turn_fut.await.unwrap().0, 409);
    pool1.close().await;

    let pool2 = init_sqlite_pool_at_path(&db_path).await.unwrap();
    let srv2 = LocalBridgeServer::bind_with_router_and_pool(
        0,
        Arc::new(TutorRouter::new(ol_cfg)),
        pool2.clone(),
    )
    .await
    .unwrap();
    let p2 = srv2.local_addr().unwrap().port();
    tokio::spawn(async move {
        let _ = srv2.serve().await;
    });
    let (ls, _, list) = http_json(p2, "GET", "/api/sessions", "").await;
    assert_eq!((ls, list.as_array().unwrap().len()), (200, 1));
    assert_eq!(
        http_json(p2, "GET", &format!("/api/sessions/{sid}"), "")
            .await
            .2["turns"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    pool2.close().await;
    let _ = std::fs::remove_dir_all(dir);
}
