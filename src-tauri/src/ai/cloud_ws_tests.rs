//! Unit and loopback tests for outbound `wss://` / loopback `ws://` AI streaming via `tokio-tungstenite`.

use crate::ai::{
    catalog::{ProviderId, ProviderPreference},
    ollama::OllamaConfig,
    openai_compat::OpenAiCompatProvider,
    provider::{AiProvider, TutorStreamItem},
    router::TutorRouter,
    router_tests::{ok_json, stub, tx},
    router_types::RouterConfigPatch,
};
use crate::domain::CorrectionMode;
use crate::telemetry::RedactedSecret;
use futures_util::{SinkExt, StreamExt};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::watch;
use tokio_tungstenite::{accept_hdr_async, tungstenite::handshake::server};

async fn spawn_ws_stub(
    gate: Option<watch::Receiver<bool>>,
    frames: Vec<String>,
) -> (u16, Arc<AtomicUsize>, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let hits = Arc::new(AtomicUsize::new(0));
    let seen = Arc::new(Mutex::new(Vec::new()));
    let (port, h_ref, s_ref) = (
        listener.local_addr().unwrap().port(),
        hits.clone(),
        seen.clone(),
    );
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            h_ref.fetch_add(1, Ordering::SeqCst);
            let (s_hdr, mut g_rx, f_list) = (s_ref.clone(), gate.clone(), frames.clone());
            tokio::spawn(async move {
                #[allow(clippy::result_large_err)]
                let cb = |req: &server::Request, res| {
                    s_hdr.lock().unwrap().push(format!("{:?}", req.headers()));
                    Ok(res)
                };
                let (mut sink, mut rx) = accept_hdr_async(stream, cb).await.unwrap().split();
                let _ = rx.next().await;
                for (i, frame) in f_list.into_iter().enumerate() {
                    if i == 1 {
                        if let Some(ref mut g) = g_rx {
                            let _ = g.wait_for(|v| *v).await;
                        }
                    }
                    if sink.send(frame.into()).await.is_err() {
                        return;
                    }
                }
                let _ = sink.close().await;
            });
        }
    });
    (port, hits, seen)
}

async fn set_patch(
    r: &TutorRouter,
    opt_in: bool,
    pref: Option<ProviderPreference>,
    key: Option<(ProviderId, &str)>,
) {
    let p = RouterConfigPatch {
        cloud_opt_in: Some(opt_in),
        preferred_provider: pref,
        api_keys: key.into_iter().map(|(k, v)| (k, v.into())).collect(),
        ..Default::default()
    };
    r.apply_config_patch(p).await.unwrap();
}

#[tokio::test]
async fn router_enforces_cloud_opt_in_and_streams_ws_sentences_with_header_auth() {
    let (gate_tx, gate_rx) = watch::channel(false);
    let f1 = r#"{"serverContent":{"modelTurn":{"parts":[{"text":"{\"spokenReply\":\"Hello via WS! "}]}}}"#;
    let f2 = r#"{"serverContent":{"modelTurn":{"parts":[{"text":"How is your day?\"}"}]},"turnComplete":true}}"#;
    let (ws_port, hits, hdrs) = spawn_ws_stub(Some(gate_rx), vec![f1.into(), f2.into()]).await;
    let router = TutorRouter::new(OllamaConfig::try_new("127.0.0.1", 1, "llama3").unwrap());
    router.set_endpoint_override(ProviderId::Gemini, format!("ws://127.0.0.1:{ws_port}/ws"));
    set_patch(
        &router,
        false,
        None,
        Some((ProviderId::Gemini, "AIzaWsSecret99")),
    )
    .await;

    let m = CorrectionMode::Natural;
    let gem = Some(ProviderPreference::Gemini);
    let denied = router
        .route_stream_turn(&[], &tx("Hi"), m, gem, None)
        .await
        .unwrap_err();
    assert!(
        denied.user_message.contains("Cloud AI is disabled") && hits.load(Ordering::SeqCst) == 0
    );
    assert!(router
        .route_stream_turn(&[], &tx("Hi"), m, None, None)
        .await
        .is_err());
    assert_eq!(hits.load(Ordering::SeqCst), 0);

    set_patch(&router, true, gem, None).await;
    let mut stream = router
        .route_stream_turn(&[], &tx("Hi WS"), m, None, None)
        .await
        .unwrap();
    let first = tokio::time::timeout(Duration::from_secs(2), stream.recv())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(first, TutorStreamItem::Sentence("Hello via WS!".into()));
    let _ = gate_tx.send(true);
    let mut final_reply = None;
    while let Some(item) = stream.recv().await {
        if let TutorStreamItem::Final(r) = item.unwrap() {
            final_reply = Some(r);
        }
    }
    let reply = final_reply.expect("final reply");
    assert_eq!(
        (reply.provider, reply.reply.spoken_reply.as_str()),
        (ProviderId::Gemini, "Hello via WS! How is your day?")
    );
    let seen = &hdrs.lock().unwrap()[0];
    assert!(seen.contains("x-goog-api-key") && seen.contains("AIzaWsSecret99"));
}

#[tokio::test]
async fn openai_ws_handles_non_streaming_and_failover_on_401_without_leaking_secrets() {
    let tags = ok_json(r#"{"models":[{"name":"qwen2.5:7b"}]}"#);
    let gen = ok_json(r#"{"response":"{\"spokenReply\":\"Fallback Ollama ok.\"}"}"#);
    let (ol_port, _) = stub(vec![tags, gen]).await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let bad_ws = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        let (mut sock, _) = listener.accept().await.unwrap();
        let _ = sock.read(&mut [0u8; 2048]).await;
        let _ = sock
            .write_all(
                b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 21\r\n\r\nBearer gsk_leak_probe",
            )
            .await;
    });
    let router =
        TutorRouter::new(OllamaConfig::try_new("127.0.0.1", ol_port, "qwen2.5:7b").unwrap());
    router.set_endpoint_override(ProviderId::Groq, format!("ws://127.0.0.1:{bad_ws}/v1/ws"));
    set_patch(
        &router,
        true,
        Some(ProviderPreference::Auto),
        Some((ProviderId::Groq, "gsk_leak_probe")),
    )
    .await;

    let m = CorrectionMode::Natural;
    let routed = router
        .route_turn(&[], &tx("Test failover"), m, None, None)
        .await
        .unwrap();
    assert_eq!(routed.provider, ProviderId::Ollama);
    let note = routed.failover_note.unwrap_or_default();
    assert!(note.contains("groq") && !note.contains("gsk_leak_probe"));

    let f = r#"{"choices":[{"delta":{"content":"{\"spokenReply\":\"WS reply ok!\"}"}}]}"#;
    let (ok_ws, _, _) = spawn_ws_stub(None, vec![f.into(), "[DONE]".into()]).await;
    let url = format!("ws://127.0.0.1:{ok_ws}/ws");
    let groq = OpenAiCompatProvider::new(
        ProviderId::Groq,
        url,
        "openai/gpt-oss-20b",
        RedactedSecret::new("gsk_direct"),
    );
    let out = groq
        .generate_tutor_reply(&[], &tx("Hi"), CorrectionMode::Off)
        .await
        .unwrap();
    assert_eq!(out.spoken_reply, "WS reply ok!");
}
