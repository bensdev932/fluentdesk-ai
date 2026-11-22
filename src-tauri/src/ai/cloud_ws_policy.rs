//! URL, TLS, and header-only secret policy for outbound `wss://` / loopback `ws://` requests.

use crate::ai::cloud_http::{err_msg, CloudHttpError, CloudHttpRequest};
use crate::telemetry::RedactedSecret;
use reqwest::Url;
use std::net::IpAddr;
use tokio_tungstenite::tungstenite::{
    client::IntoClientRequest,
    http::{HeaderMap, HeaderName, HeaderValue, Request},
};

pub(crate) struct CloudWsRequest {
    pub url: String,
    pub secret_headers: Vec<(String, RedactedSecret)>,
    pub extra_headers: Vec<(String, String)>,
    pub initial_messages: Vec<String>,
    pub timeout_secs: u64,
}

impl CloudWsRequest {
    pub(crate) fn from_http(req: CloudHttpRequest) -> Self {
        Self {
            url: req.url,
            secret_headers: req.secret_headers,
            extra_headers: req.extra_headers,
            initial_messages: vec![req.body],
            timeout_secs: req.timeout_secs,
        }
    }
}

pub(crate) fn validate_ws_url(raw: &str) -> Result<Url, CloudHttpError> {
    let url =
        Url::parse(raw).map_err(|e| err_msg(None, format!("Invalid WebSocket URL: {e}"), false))?;
    reject_url_secrets(&url)?;
    check_ws_scheme(&url)?;
    Ok(url)
}

pub(crate) fn build_ws_handshake_request(
    req: &CloudWsRequest,
) -> Result<(Request<()>, bool), CloudHttpError> {
    let url = validate_ws_url(&req.url)?;
    let is_tls = url.scheme() == "wss";
    let mut http_req = url
        .as_str()
        .into_client_request()
        .map_err(|e| err_msg(None, format!("Invalid WebSocket handshake: {e}"), false))?;
    append_ws_headers(http_req.headers_mut(), req)?;
    Ok((http_req, is_tls))
}

fn reject_url_secrets(url: &Url) -> Result<(), CloudHttpError> {
    if !url.username().is_empty() || url.password().is_some() {
        return Err(policy_err("WebSocket URL must not embed credentials."));
    }
    if url.query_pairs().any(|(k, _)| is_secret_param(&k)) {
        return Err(policy_err(
            "Secrets must be passed in headers, not URL query.",
        ));
    }
    Ok(())
}

fn is_secret_param(key: &str) -> bool {
    let k = key.to_ascii_lowercase();
    matches!(k.as_str(), "key" | "auth" | "authorization" | "bearer")
        || k.contains("api_key")
        || k.contains("api-key")
        || k.contains("apikey")
        || k.contains("x-goog-api-key")
        || k.contains("token")
        || k.contains("secret")
        || k.contains("password")
}

fn check_ws_scheme(url: &Url) -> Result<(), CloudHttpError> {
    let host = url
        .host_str()
        .filter(|h| !h.is_empty())
        .ok_or_else(|| policy_err("WebSocket URL missing host."))?;
    match url.scheme() {
        "wss" => Ok(()),
        "ws" if is_loopback_ws_host(host) => Ok(()),
        "ws" => Err(policy_err(
            "Non-TLS ws:// is restricted to loopback test hosts.",
        )),
        _ => Err(policy_err("Unsupported WebSocket AI endpoint scheme.")),
    }
}

fn is_loopback_ws_host(host: &str) -> bool {
    let clean = host.trim_matches(|c| c == '[' || c == ']');
    clean.eq_ignore_ascii_case("localhost")
        || clean.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback())
}

fn append_ws_headers(headers: &mut HeaderMap, req: &CloudWsRequest) -> Result<(), CloudHttpError> {
    for (k, v) in &req.extra_headers {
        append_header(headers, k, v)?;
    }
    for (k, s) in &req.secret_headers {
        append_header(headers, k, s.expose_secret())?;
    }
    Ok(())
}

fn append_header(headers: &mut HeaderMap, name: &str, value: &str) -> Result<(), CloudHttpError> {
    let hn = HeaderName::from_bytes(name.as_bytes())
        .map_err(|e| err_msg(None, format!("Invalid header name: {e}"), false))?;
    let hv = HeaderValue::from_str(value).map_err(|_| policy_err("Invalid header value."))?;
    headers.insert(hn, hv);
    Ok(())
}

fn policy_err(msg: &str) -> CloudHttpError {
    err_msg(None, msg, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_ws_url_enforces_wss_or_loopback_and_rejects_url_secrets() {
        assert!(validate_ws_url("wss://generativelanguage.googleapis.com/ws/v1beta").is_ok());
        assert!(validate_ws_url("ws://127.0.0.1:9001/ws").is_ok());
        assert!(validate_ws_url("ws://localhost:9001/ws").is_ok());
        assert!(validate_ws_url("ws://[::1]:9001/ws").is_ok());
        for bad in [
            "ws://api.groq.com/ws",
            "ws://localhost.evil.com/ws",
            "ws://127.0.0.1.nip.io/ws",
            "ws://user:pass@127.0.0.1:9001/ws",
            "wss://api.groq.com/ws?key=AIzaSecret123",
            "https://api.groq.com/openai/v1/chat/completions",
        ] {
            assert!(validate_ws_url(bad).is_err(), "expected error for {bad}");
        }
        let req = CloudWsRequest {
            url: "wss://api.groq.com/ws".into(),
            secret_headers: vec![(
                "Authorization".into(),
                RedactedSecret::new("Bearer gsk_top_secret"),
            )],
            extra_headers: vec![],
            initial_messages: vec!["{}".into()],
            timeout_secs: 5,
        };
        let (built, is_tls) = build_ws_handshake_request(&req).unwrap();
        assert!(is_tls);
        assert_eq!(
            built.headers().get("authorization").unwrap(),
            "Bearer gsk_top_secret"
        );
        assert!(!built.uri().to_string().contains("gsk_top_secret"));
    }
}
