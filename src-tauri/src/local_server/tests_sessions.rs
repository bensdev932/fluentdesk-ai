//! Loopback HTTP integration tests for `/api/sessions` lifecycle, turns, history, and validation.

use crate::local_server::tests::{closed_port, ollama_http};
use crate::local_server::tests_cycling::{http_json, setup_bridge, spawn_stub};
use std::sync::Arc;

async fn spawn_multi_turn_ollama() -> (u16, Arc<std::sync::Mutex<Vec<String>>>) {
    let tags = ollama_http("200 OK", r#"{"models":[{"name":"qwen2.5:7b"}]}"#);
    let r1 = ollama_http(
        "200 OK",
        r#"{"response":"{\"spokenReply\":\"Nice! Did you order espresso?\",\"correction\":\"Say 'went to the coffee shop'.\",\"followUpPrompt\":\"Did you order espresso?\"}"}"#,
    );
    let r2 = ollama_http(
        "200 OK",
        r#"{"response":"{\"spokenReply\":\"Iced lattes are great! Do you add syrup?\"}"}"#,
    );
    spawn_stub(move |req, _idx| {
        if req.contains("/api/tags") {
            tags.clone()
        } else if req.contains("coffee shop") {
            r1.clone()
        } else {
            r2.clone()
        }
    })
    .await
}

#[tokio::test]
async fn sessions_lifecycle_turns_backend_history_and_coaching_persist() {
    let (ol_port, seen) = spawn_multi_turn_ollama().await;
    let bridge = setup_bridge(ol_port, vec![]).await;

    let (st, _, started) = http_json(bridge, "POST", "/api/sessions", "{}").await;
    assert_eq!(st, 200);
    let sid = started["sessionId"].as_str().unwrap().to_string();
    assert_eq!(
        (
            started["state"].as_str(),
            started["correctionMode"].as_str(),
            started["pushToTalk"].as_bool()
        ),
        (Some("Listening"), Some("Gentle"), Some(false))
    );

    let turn_path = format!("/api/sessions/{sid}/turn");
    let (t1s, _, t1) = http_json(
        bridge,
        "POST",
        &turn_path,
        r#"{"userText":"Yesterday I go to coffee shop."}"#,
    )
    .await;
    assert_eq!(
        (
            t1s,
            t1["provider"].as_str(),
            t1["model"].as_str(),
            t1["isCloud"].as_bool(),
            t1["state"].as_str()
        ),
        (
            200,
            Some("ollama"),
            Some("qwen2.5:7b"),
            Some(false),
            Some("Listening")
        )
    );
    assert_eq!(
        t1["tutorTurn"]["correction"].as_str(),
        Some("Say 'went to the coffee shop'.")
    );
    assert_eq!(
        t1["tutorTurn"]["followUpPrompt"].as_str(),
        Some("Did you order espresso?")
    );

    let (t2s, _, _) = http_json(
        bridge,
        "POST",
        &turn_path,
        r#"{"userText":"I ordered iced latte.","correctionMode":"Gentle"}"#,
    )
    .await;
    assert_eq!(t2s, 200);
    assert!(seen
        .lock()
        .unwrap()
        .last()
        .unwrap()
        .contains("Yesterday I go to coffee shop."));

    assert_eq!(
        http_json(bridge, "POST", &format!("/api/sessions/{sid}/pause"), "")
            .await
            .2["state"],
        "Paused"
    );
    assert_eq!(
        http_json(bridge, "POST", &turn_path, r#"{"userText":"While paused"}"#)
            .await
            .0,
        409
    );
    assert_eq!(
        http_json(bridge, "POST", &format!("/api/sessions/{sid}/resume"), "")
            .await
            .2["state"],
        "Listening"
    );
    assert_eq!(
        http_json(bridge, "POST", &format!("/api/sessions/{sid}/stop"), "")
            .await
            .2["state"],
        "Ended"
    );
    assert_eq!(
        http_json(bridge, "GET", &format!("/api/sessions/{sid}/status"), "")
            .await
            .2["state"],
        "Ended"
    );

    let (ls, _, list) = http_json(bridge, "GET", "/api/sessions", "").await;
    assert_eq!((ls, list.as_array().unwrap().len()), (200, 1));
    assert_eq!(list[0]["turnCount"].as_u64(), Some(4));
    assert!(list[0]["endedAtUnixMs"].is_number());

    let (hs, _, hist) = http_json(bridge, "GET", &format!("/api/sessions/{sid}"), "").await;
    assert_eq!((hs, hist["turns"].as_array().unwrap().len()), (200, 4));
    assert_eq!(
        hist["turns"][1]["correction"].as_str(),
        Some("Say 'went to the coffee shop'.")
    );
    assert_eq!(
        http_json(bridge, "GET", "/api/sessions/nonexistent-id", "")
            .await
            .0,
        404
    );
}

#[tokio::test]
async fn sessions_reject_invalid_payloads_and_unavailable_ai() {
    let down_bridge = setup_bridge(closed_port().await, vec![]).await;
    assert_eq!(
        http_json(down_bridge, "POST", "/api/sessions", "{}")
            .await
            .0,
        503
    );
    assert_eq!(
        http_json(down_bridge, "POST", "/api/sessions", "{bad-json")
            .await
            .0,
        400
    );
    assert_eq!(
        http_json(
            down_bridge,
            "POST",
            "/api/sessions/s1/turn",
            r#"{"userText":"   "}"#,
        )
        .await
        .0,
        400
    );
}
