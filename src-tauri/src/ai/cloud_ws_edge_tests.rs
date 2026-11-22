//! Edge-case tests for outbound `wss://` TLS enforcement and query-parameter secret rejection.

use crate::ai::cloud_ws::{connect_ws_stream, CloudWsRequest};
use crate::ai::cloud_ws_policy::validate_ws_url;
use crate::telemetry::RedactedSecret;
use tokio::io::AsyncReadExt;
use tokio::net::TcpListener;

#[test]
fn validate_ws_url_rejects_extended_secret_query_params() {
    for bad in [
        "wss://api.groq.com/ws?api-key=gsk_abc",
        "wss://api.groq.com/ws?client_secret=sec_123",
        "wss://api.groq.com/ws?auth_token=tok_123",
        "wss://api.groq.com/ws?authorization=Bearer123",
    ] {
        assert!(validate_ws_url(bad).is_err(), "expected error for {bad}");
    }
}

#[tokio::test]
async fn uppercase_wss_enforces_native_tls_and_fails_against_plaintext_tcp() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let (byte_tx, byte_rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        if let Ok((mut sock, _)) = listener.accept().await {
            let mut buf = [0u8; 16];
            let n = sock.read(&mut buf).await.unwrap_or(0);
            let _ = byte_tx.send(if n > 0 { buf[0] } else { 0 });
        }
    });
    let req = CloudWsRequest {
        url: format!("WSS://127.0.0.1:{port}/ws"),
        secret_headers: vec![(
            "Authorization".into(),
            RedactedSecret::new("Bearer gsk_tls_probe"),
        )],
        extra_headers: vec![],
        initial_messages: vec!["{}".into()],
        timeout_secs: 5,
    };
    let err = connect_ws_stream(req).await.unwrap_err();
    assert!(!err.message.contains("gsk_tls_probe"));
    assert!(err.is_network);
    assert_eq!(
        byte_rx.await.unwrap_or(0),
        0x16,
        "expected TLS ClientHello record byte 0x16 for WSS:// URL"
    );
}
