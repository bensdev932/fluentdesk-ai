//! Loopback HTTP tests for cloud round-robin cycling and shared bridge test helpers.

use crate::ai::{catalog::ProviderId, ollama::OllamaConfig, router::TutorRouter};
use crate::local_server::tests::{closed_port, ollama_http, send_raw};
use crate::local_server::LocalBridgeServer;
use serde_json::Value;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

pub(super) async fn spawn_stub<F>(handler: F) -> (u16, Arc<Mutex<Vec<String>>>)
where
    F: Fn(&str, usize) -> String + Send + Sync + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sref = Arc::clone(&seen);
    tokio::spawn(async move {
        while let Ok((mut sock, _)) = listener.accept().await {
            let req = read_req(&mut sock).await;
            let idx = {
                let mut g = sref.lock().unwrap();
                g.push(req.clone());
                g.len() - 1
            };
            let _ = sock.write_all(handler(&req, idx).as_bytes()).await;
            let _ = sock.shutdown().await;
        }
    });
    (port, seen)
}

pub(super) async fn read_req(sock: &mut TcpStream) -> String {
    let (mut buf, mut tmp) = (Vec::with_capacity(2048), [0u8; 2048]);
    while !req_done(&buf) {
        match sock.read(&mut tmp).await {
            Ok(n) if n > 0 => buf.extend_from_slice(&tmp[..n]),
            _ => break,
        }
    }
    String::from_utf8_lossy(&buf).to_string()
}

fn req_done(buf: &[u8]) -> bool {
    let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") else {
        return false;
    };
    let head = String::from_utf8_lossy(&buf[..pos]);
    let cl = head.lines().find_map(parse_cl).unwrap_or(0);
    buf.len() >= pos + 4 + cl
}

fn parse_cl(line: &str) -> Option<usize> {
    let (k, v) = line.split_once(':')?;
    k.trim()
        .eq_ignore_ascii_case("content-length")
        .then(|| v.trim().parse().ok())
        .flatten()
}

pub(super) fn openai_ok(msg: &str) -> String {
    let b =
        format!(r#"{{"choices":[{{"message":{{"content":"{{\"spokenReply\":\"{msg}\"}}"}}}}]}}"#);
    ollama_http("200 OK", &b)
}

pub(super) fn gemini_ok(msg: &str) -> String {
    let b = format!(
        r#"{{"candidates":[{{"content":{{"parts":[{{"text":"{{\"spokenReply\":\"{msg}\"}}"}}]}}}}]}}"#
    );
    ollama_http("200 OK", &b)
}

pub(super) async fn spawn_ollama_ok() -> u16 {
    let tags = ollama_http("200 OK", r#"{"models":[{"name":"qwen2.5:7b"}]}"#);
    let gen_ok = ollama_http(
        "200 OK",
        r#"{"response":"{\"spokenReply\":\"Local Ollama caught it! What next?\"}"}"#,
    );
    spawn_stub(move |req, _| {
        if req.contains("/api/tags") {
            tags.clone()
        } else {
            gen_ok.clone()
        }
    })
    .await
    .0
}

pub(super) async fn setup_bridge(ollama_port: u16, endpoints: Vec<(ProviderId, u16)>) -> u16 {
    let cfg = OllamaConfig::try_new("127.0.0.1", ollama_port, "llama3.2:3b").unwrap();
    let router = Arc::new(TutorRouter::new(cfg));
    for (id, p) in endpoints {
        let path = if id == ProviderId::Gemini {
            "/v1beta/models"
        } else {
            "/v1/chat/completions"
        };
        router.set_endpoint_override(id, format!("http://127.0.0.1:{p}{path}"));
    }
    let srv = LocalBridgeServer::bind_with_router(0, router)
        .await
        .expect("bind");
    let port = srv.local_addr().expect("addr").port();
    tokio::spawn(async move {
        let _ = srv.serve().await;
    });
    port
}

pub(super) async fn http_json(
    port: u16,
    method: &str,
    path: &str,
    body: &str,
) -> (u16, String, Value) {
    let hdr = if body.is_empty() {
        String::new()
    } else {
        format!(
            "Content-Type: application/json\r\nContent-Length: {}\r\n",
            body.len()
        )
    };
    let req = format!("{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\n{hdr}\r\n{body}");
    let (status, raw) = send_raw(port, &req).await;
    let val = serde_json::from_str(&raw).unwrap_or(Value::Null);
    (status, raw, val)
}

pub(super) fn prov<'a>(snap: &'a Value, id: &str) -> &'a Value {
    snap["providers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == id)
        .unwrap()
}

pub(super) fn assert_no_secrets(raw: &str, secrets: &[&str]) {
    for s in secrets {
        assert!(!raw.contains(s), "leaked secret {s}");
    }
}

pub(super) async fn send_turn(port: u16, text: &str) -> (u16, String, Value) {
    let body = format!(r#"{{"history":[],"userText":"{text}","correctionMode":"Natural"}}"#);
    http_json(port, "POST", "/api/tutor/reply", &body).await
}

#[tokio::test]
async fn round_robin_cycles_across_three_cloud_endpoints() {
    let (gp, g_seen) = spawn_stub(|_, _| openai_ok("Groq turn! Book?")).await;
    let (mp, m_seen) = spawn_stub(|_, _| gemini_ok("Gemini turn! Author?")).await;
    let (op, o_seen) = spawn_stub(|_, _| openai_ok("OpenRouter turn! Chapter?")).await;
    let bridge = setup_bridge(
        closed_port().await,
        vec![
            (ProviderId::Groq, gp),
            (ProviderId::Gemini, mp),
            (ProviderId::Openrouter, op),
        ],
    )
    .await;
    let cfg = r#"{"cloudOptIn":true,"preferredProvider":"auto","apiKeys":{"groq":"gsk_cycle_1","gemini":"AIzaCycle_2","openrouter":"sk-or-cycle_3"}}"#;
    let (cs, craw, cjson) = http_json(bridge, "POST", "/api/providers/config", cfg).await;
    assert_eq!(
        (cs, prov(&cjson, "groq")["authInvalid"].as_bool()),
        (200, Some(false))
    );
    assert_no_secrets(&craw, &["gsk_cycle_1", "AIzaCycle_2", "sk-or-cycle_3"]);

    for (expected, text) in [
        ("groq", "One"),
        ("gemini", "Two"),
        ("openrouter", "Three"),
        ("groq", "Four"),
    ] {
        let (st, raw, json) = send_turn(bridge, text).await;
        assert_eq!(st, 200, "unexpected: {raw}");
        assert_eq!(
            (json["provider"].as_str(), json["isCloud"].as_bool()),
            (Some(expected), Some(true))
        );
    }
    assert!(g_seen.lock().unwrap()[0].contains("Bearer gsk_cycle_1"));
    assert!(m_seen.lock().unwrap()[0].contains("x-goog-api-key: AIzaCycle_2"));
    assert!(o_seen.lock().unwrap()[0].contains("Bearer sk-or-cycle_3"));
    assert_eq!(
        [&g_seen, &m_seen, &o_seen].map(|s| s.lock().unwrap().len()),
        [2, 1, 1]
    );
}
