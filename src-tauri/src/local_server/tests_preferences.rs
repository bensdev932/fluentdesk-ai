//! Integration tests for `/api/preferences`, validation, session defaults, and router sync.

use crate::local_server::tests_cycling::{http_json, prov, setup_bridge, spawn_ollama_ok};

#[tokio::test]
async fn preferences_endpoints_validate_speed_inherit_session_defaults_and_sync_with_router() {
    let bridge = setup_bridge(spawn_ollama_ok().await, vec![]).await;

    let (gs, _, initial) = http_json(bridge, "GET", "/api/preferences", "").await;
    assert_eq!(gs, 200);
    assert_eq!(
        (
            initial["correctionMode"].as_str(),
            initial["speakingSpeed"].as_f64(),
            initial["pushToTalkEnabled"].as_bool(),
            initial["cloudOptIn"].as_bool(),
            initial["rawAudioRecordingOptIn"].as_bool(),
            initial["preferredProvider"].as_str(),
        ),
        (
            Some("Gentle"),
            Some(1.0),
            Some(false),
            Some(false),
            Some(false),
            Some("auto")
        )
    );

    for bad_speed in [r#"{"speakingSpeed":0.4}"#, r#"{"speakingSpeed":2.2}"#] {
        let (st, body, _) = http_json(bridge, "PATCH", "/api/preferences", bad_speed).await;
        assert_eq!(st, 400, "expected 400 for {bad_speed}: {body}");
    }

    let patch = r#"{"correctionMode":"Gentle","speakingSpeed":1.25,"pushToTalkEnabled":true,"rawAudioRecordingOptIn":true,"cloudOptIn":true,"preferredProvider":"gemini","modelOverrides":{"gemini":"gemini-3.5-flash"}}"#;
    let (ps, _, updated) = http_json(bridge, "PATCH", "/api/preferences", patch).await;
    assert_eq!(ps, 200);
    assert_eq!(
        (
            updated["correctionMode"].as_str(),
            updated["speakingSpeed"].as_f64(),
            updated["pushToTalkEnabled"].as_bool(),
            updated["preferredProvider"].as_str(),
        ),
        (Some("Gentle"), Some(1.25), Some(true), Some("gemini"))
    );

    let (_, _, prov_snap) = http_json(bridge, "GET", "/api/providers", "").await;
    assert_eq!(prov_snap["cloudOptIn"].as_bool(), Some(true));
    assert_eq!(prov_snap["preferredProvider"].as_str(), Some("gemini"));
    assert_eq!(
        prov(&prov_snap, "gemini")["selectedModel"].as_str(),
        Some("gemini-3.5-flash")
    );

    let (ds, _, disabled) = http_json(
        bridge,
        "POST",
        "/api/providers/config",
        r#"{"cloudOptIn":false}"#,
    )
    .await;
    assert_eq!(ds, 200);
    assert_eq!(disabled["cloudOptIn"].as_bool(), Some(false));
    assert_eq!(disabled["preferredProvider"].as_str(), Some("auto"));

    let (_, _, after_prefs) = http_json(bridge, "GET", "/api/preferences", "").await;
    assert_eq!(after_prefs["cloudOptIn"].as_bool(), Some(false));
    assert_eq!(after_prefs["preferredProvider"].as_str(), Some("auto"));

    let (ss, _, started) = http_json(bridge, "POST", "/api/sessions", "{}").await;
    assert_eq!(ss, 200);
    assert_eq!(started["correctionMode"].as_str(), Some("Gentle"));
    assert_eq!(started["pushToTalk"].as_bool(), Some(true));

    let (_, _, a1_prefs) = http_json(
        bridge,
        "PATCH",
        "/api/preferences",
        r#"{"cefrLevel":"A1","preConversationScriptEnabled":true}"#,
    )
    .await;
    assert_eq!(a1_prefs["cefrLevel"].as_str(), Some("A1"));
    assert_eq!(
        a1_prefs["preConversationScriptEnabled"].as_bool(),
        Some(true)
    );
    let sid = started["sessionId"].as_str().unwrap();
    let (_, _, mid_status) =
        http_json(bridge, "GET", &format!("/api/sessions/{sid}/status"), "").await;
    assert_eq!(mid_status["cefrLevel"].as_str(), Some("A1"));

    let (j_st, _, job_started) = http_json(
        bridge,
        "POST",
        "/api/sessions",
        r#"{"scenarioId":"job_interview"}"#,
    )
    .await;
    assert_eq!(j_st, 200);
    assert_eq!(job_started["cefrLevel"].as_str(), Some("A1"));
}
