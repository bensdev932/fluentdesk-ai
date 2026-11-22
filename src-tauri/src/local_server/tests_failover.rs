//! Loopback HTTP tests for `503`/closed-port/`429` cooldown fallback, `401` `authInvalid`, and reset.

use crate::ai::catalog::ProviderId;
use crate::local_server::tests::{closed_port, ollama_http};
use crate::local_server::tests_cycling::{
    assert_no_secrets, gemini_ok, http_json, openai_ok, prov, send_turn, setup_bridge,
    spawn_ollama_ok, spawn_stub,
};

#[tokio::test]
async fn outage_503_closed_port_and_429_trip_cooldowns_and_fall_back_to_ollama() {
    let (gp, g_seen) =
        spawn_stub(|_, _| ollama_http("503 Service Unavailable", r#"{"error":"down"}"#)).await;
    let (op, o_seen) =
        spawn_stub(|_, _| ollama_http("429 Too Many Requests", r#"{"error":"limit"}"#)).await;
    let bridge = setup_bridge(
        spawn_ollama_ok().await,
        vec![
            (ProviderId::Groq, gp),
            (ProviderId::Gemini, closed_port().await),
            (ProviderId::Openrouter, op),
        ],
    )
    .await;
    let cfg = r#"{"cloudOptIn":true,"preferredProvider":"auto","apiKeys":{"groq":"gsk_out_1","gemini":"AIzaOut_2","openrouter":"sk-or-out_3"}}"#;
    assert_eq!(
        http_json(bridge, "POST", "/api/providers/config", cfg)
            .await
            .0,
        200
    );
    let (st, raw, json) = send_turn(bridge, "Hello fallback").await;
    assert_eq!(
        (
            st,
            json["provider"].as_str(),
            json["isCloud"].as_bool(),
            json["model"].as_str()
        ),
        (200, Some("ollama"), Some(false), Some("qwen2.5:7b"))
    );
    let note = json["failoverNote"].as_str().unwrap_or("");
    assert!(note.contains("groq") && note.contains("gemini") && note.contains("openrouter"));
    assert_no_secrets(&raw, &["gsk_out_1", "AIzaOut_2", "sk-or-out_3"]);

    let (gs, _, snap) = http_json(bridge, "GET", "/api/providers", "").await;
    assert_eq!(gs, 200);
    for id in ["groq", "gemini", "openrouter"] {
        assert!(prov(&snap, id)["cooldownSecs"].as_u64().unwrap_or(0) > 0);
    }
    assert_eq!(send_turn(bridge, "Turn two").await.2["provider"], "ollama");
    assert_eq!(
        (g_seen.lock().unwrap().len(), o_seen.lock().unwrap().len()),
        (1, 1)
    );
}

#[tokio::test]
async fn auth_401_sets_auth_invalid_and_reset_cooldowns_clears_state() {
    let (gp, _) = spawn_stub(|req, _| {
        if req.contains("gsk_bad_401") {
            ollama_http(
                "401 Unauthorized",
                r#"{"error":"invalid Bearer gsk_bad_401"}"#,
            )
        } else {
            openai_ok("Groq fixed! Next?")
        }
    })
    .await;
    let (mp, _) = spawn_stub(|_, idx| {
        if idx == 0 {
            ollama_http("429 Too Many Requests", r#"{"error":"limit"}"#)
        } else {
            gemini_ok("Gemini ok! Next?")
        }
    })
    .await;
    let bridge = setup_bridge(
        spawn_ollama_ok().await,
        vec![(ProviderId::Groq, gp), (ProviderId::Gemini, mp)],
    )
    .await;
    let secrets = ["gsk_bad_401", "AIzaGoodKey_2"];
    let cfg = r#"{"cloudOptIn":true,"preferredProvider":"auto","apiKeys":{"groq":"gsk_bad_401","gemini":"AIzaGoodKey_2"}}"#;
    assert_eq!(
        http_json(bridge, "POST", "/api/providers/config", cfg)
            .await
            .0,
        200
    );
    let (st1, raw1, json1) = send_turn(bridge, "Auth test").await;
    assert_eq!((st1, json1["provider"].as_str()), (200, Some("ollama")));
    assert_no_secrets(&raw1, &secrets);

    let (gs, graw, snap) = http_json(bridge, "GET", "/api/providers", "").await;
    assert_eq!(
        (gs, prov(&snap, "groq")["authInvalid"].as_bool()),
        (200, Some(true))
    );
    assert!(prov(&snap, "gemini")["cooldownSecs"].as_u64().unwrap_or(0) > 0);
    assert_no_secrets(&graw, &secrets);

    let (rs, rraw, rsnap) = http_json(
        bridge,
        "POST",
        "/api/providers/config",
        r#"{"resetCooldowns":true}"#,
    )
    .await;
    assert_eq!(
        (
            rs,
            prov(&rsnap, "groq")["authInvalid"].as_bool(),
            prov(&rsnap, "gemini")["cooldownSecs"].as_u64()
        ),
        (200, Some(false), Some(0))
    );
    assert_no_secrets(&rraw, &secrets);

    let (es, eraw, _) = http_json(
        bridge,
        "POST",
        "/api/tutor/reply",
        r#"{"history":[],"userText":"Pinned 401","provider":"groq"}"#,
    )
    .await;
    assert_eq!(es, 503);
    assert_no_secrets(&eraw, &secrets);

    assert_eq!(
        http_json(
            bridge,
            "POST",
            "/api/providers/config",
            r#"{"apiKeys":{"groq":"gsk_good_1"}}"#
        )
        .await
        .0,
        200
    );
    let (st2, _, json2) = send_turn(bridge, "Recovered").await;
    assert_eq!((st2, json2["isCloud"].as_bool()), (200, Some(true)));
}
