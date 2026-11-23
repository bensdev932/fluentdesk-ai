//! Loopback HTTP tests for `/api/sessions/:id/review` and `/api/learner/facts` CRUD.

use crate::local_server::tests::ollama_http;
use crate::local_server::tests_cycling::{http_json, setup_bridge, spawn_stub};

#[tokio::test]
async fn http_review_and_learner_facts_roundtrip_and_delete() {
    let tags = ollama_http("200 OK", r#"{"models":[{"name":"qwen2.5:7b"}]}"#);
    let reply = ollama_http(
        "200 OK",
        r#"{"response":"{\"spokenReply\":\"Nice to meet you!\"}"}"#,
    );
    let (ol, _) = spawn_stub(move |req, _| {
        if req.contains("/api/tags") {
            tags.clone()
        } else {
            reply.clone()
        }
    })
    .await;
    let port = setup_bridge(ol, vec![]).await;

    let (_, _, s) = http_json(
        port,
        "POST",
        "/api/sessions",
        r#"{"scenarioId":"job_interview"}"#,
    )
    .await;
    let sid = s["sessionId"].as_str().unwrap().to_string();
    http_json(
        port,
        "POST",
        &format!("/api/sessions/{sid}/turn"),
        r#"{"userText":"I live in Seattle and solved a tough deadline challenge."}"#,
    )
    .await;
    let (_, _, stopped) = http_json(port, "POST", &format!("/api/sessions/{sid}/stop"), "").await;
    assert_eq!(
        stopped["sessionReview"]["estimateLabel"].as_str(),
        Some("AI Estimate")
    );

    let (ps, _, prog) = http_json(port, "GET", "/api/learner/progress", "").await;
    assert_eq!(ps, 200);
    assert_eq!(prog["totalSessions"].as_u64(), Some(1));
    assert!(prog["practiceMinutes"].as_u64().unwrap_or(0) >= 1);
    assert!(prog["dayStreak"].as_u64().unwrap_or(0) >= 1);

    let (rs, _, rev) = http_json(port, "GET", &format!("/api/sessions/{sid}/review"), "").await;
    assert_eq!(
        (
            rs,
            rev["estimateLabel"].as_str(),
            rev["scenarioId"].as_str()
        ),
        (200, Some("AI Estimate"), Some("job_interview"))
    );

    let (fs, _, facts) = http_json(port, "GET", "/api/learner/facts", "").await;
    let arr = facts.as_array().unwrap();
    assert_eq!(fs, 200);
    assert!(!arr.is_empty());
    let fid = arr[0]["id"].as_str().unwrap();

    assert_eq!(
        http_json(port, "DELETE", &format!("/api/learner/facts/{fid}"), "")
            .await
            .0,
        200
    );
    assert_eq!(
        http_json(port, "GET", "/api/learner/facts", "")
            .await
            .2
            .as_array()
            .unwrap()
            .len(),
        0
    );
}

#[tokio::test]
async fn scenarios_endpoint_lists_presets_and_starts_scenario_session() {
    let tags = ollama_http("200 OK", r#"{"models":[{"name":"qwen2.5:7b"}]}"#);
    let ok = ollama_http(
        "200 OK",
        r#"{"response":"{\"spokenReply\":\"Hi there!\"}"}"#,
    );
    let (ol_port, _) = spawn_stub(move |req, _| {
        if req.contains("/api/tags") {
            tags.clone()
        } else {
            ok.clone()
        }
    })
    .await;
    let bridge = setup_bridge(ol_port, vec![]).await;

    let (st, _, scenarios) = http_json(bridge, "GET", "/api/scenarios", "").await;
    assert_eq!(st, 200);
    assert_eq!(scenarios.as_array().unwrap().len(), 8);

    let (s_st, _, started) = http_json(
        bridge,
        "POST",
        "/api/sessions",
        r#"{"scenarioId":"cafe_order"}"#,
    )
    .await;
    assert_eq!(s_st, 200);
    assert_eq!(started["scenarioId"], "cafe_order");
    assert_eq!(started["cefrLevel"], "B1");
    assert_eq!(started["goals"].as_array().unwrap().len(), 3);
    assert!(started["openerReply"]["spokenReply"]
        .as_str()
        .unwrap_or("")
        .contains("Sunrise Cafe"));
}
