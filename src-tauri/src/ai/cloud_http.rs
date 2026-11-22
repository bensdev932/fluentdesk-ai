//! Shared pooled `reqwest::Client` HTTP/HTTPS POST transport for cloud AI adapters.

use crate::telemetry::{redact_sensitive_log_text, RedactedSecret};
use reqwest::{header::CONTENT_TYPE, Client, Request, RequestBuilder, Response, Url};
use std::sync::OnceLock;
use std::time::Duration;

const CLOUD_CONNECT_TIMEOUT_SECS: u64 = 4;
static HTTPS_CLIENT: OnceLock<Client> = OnceLock::new();
static LOOPBACK_CLIENT: OnceLock<Client> = OnceLock::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CloudFailure {
    Cooldown,
    AuthInvalid,
    None,
}

pub(crate) struct CloudHttpRequest {
    pub url: String,
    pub secret_headers: Vec<(String, RedactedSecret)>,
    pub extra_headers: Vec<(String, String)>,
    pub body: String,
    pub timeout_secs: u64,
}

#[derive(Debug)]
pub(crate) struct CloudHttpError {
    pub status: Option<u16>,
    pub message: String,
    pub is_network: bool,
}

pub(crate) async fn post_json(req: CloudHttpRequest) -> Result<String, CloudHttpError> {
    let (client, request) = build_reqwest(req, 5)?;
    let resp = client.execute(request).await.map_err(map_reqwest_err)?;
    read_json_response(resp).await
}

async fn read_json_response(resp: Response) -> Result<String, CloudHttpError> {
    let status = resp.status().as_u16();
    let text = resp
        .text()
        .await
        .map_err(|e| net_err(format!("HTTP read error: {e}")))?;
    if !(200..=299).contains(&status) {
        return Err(err_status(status, &text));
    }
    Ok(text.trim().to_string())
}

pub(crate) fn build_reqwest(
    req: CloudHttpRequest,
    min_timeout_secs: u64,
) -> Result<(&'static Client, Request), CloudHttpError> {
    if !req.url.starts_with("http://") && !req.url.starts_with("https://") {
        return Err(err_msg(None, "Unsupported AI endpoint scheme.", false));
    }
    let client = client_for_url(&req.url);
    let rb = apply_headers(client.post(&req.url), &req, min_timeout_secs);
    let built = rb
        .body(req.body)
        .build()
        .map_err(|e| err_msg(None, format!("Invalid HTTP request: {e}"), false))?;
    Ok((client, built))
}

fn apply_headers(mut rb: RequestBuilder, req: &CloudHttpRequest, min_secs: u64) -> RequestBuilder {
    rb = rb
        .timeout(Duration::from_secs(req.timeout_secs.max(min_secs)))
        .header(CONTENT_TYPE, "application/json");
    for (k, v) in &req.extra_headers {
        rb = rb.header(k.as_str(), v.as_str());
    }
    for (k, secret) in &req.secret_headers {
        rb = rb.header(k.as_str(), secret.expose_secret());
    }
    rb
}

fn client_for_url(url: &str) -> &'static Client {
    if is_loopback_url(url) {
        LOOPBACK_CLIENT.get_or_init(|| build_client(true))
    } else {
        HTTPS_CLIENT.get_or_init(|| build_client(false))
    }
}

fn build_client(no_proxy: bool) -> Client {
    let builder = Client::builder()
        .pool_max_idle_per_host(4)
        .pool_idle_timeout(Duration::from_secs(90))
        .tcp_keepalive(Duration::from_secs(30))
        .tcp_nodelay(true)
        .connect_timeout(Duration::from_secs(CLOUD_CONNECT_TIMEOUT_SECS));
    let builder = if no_proxy {
        builder.no_proxy()
    } else {
        builder
    };
    builder.build().expect("reqwest client pool")
}

fn is_loopback_url(url: &str) -> bool {
    Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(is_loopback_host))
        .unwrap_or(false)
}

fn is_loopback_host(host: &str) -> bool {
    matches!(
        host.to_ascii_lowercase().as_str(),
        "127.0.0.1" | "localhost" | "[::1]" | "::1"
    )
}

pub(crate) fn map_reqwest_err(err: reqwest::Error) -> CloudHttpError {
    net_err(format!("HTTP request failed: {}", err.without_url()))
}

pub(crate) fn err_status(status: u16, body: &str) -> CloudHttpError {
    let clean = redact_sensitive_log_text(body);
    let snippet: String = clean.chars().take(180).collect();
    err_msg(Some(status), format!("HTTP {status}: {snippet}"), false)
}

pub(crate) fn net_err(raw: impl Into<String>) -> CloudHttpError {
    err_msg(None, raw, true)
}

pub(crate) fn err_msg(
    status: Option<u16>,
    raw: impl Into<String>,
    is_network: bool,
) -> CloudHttpError {
    CloudHttpError {
        status,
        message: redact_sensitive_log_text(&raw.into()),
        is_network,
    }
}

#[cfg(test)]
mod tests {
    use crate::ai::gemini::{extract_gemini_text, GeminiProvider};
    use crate::ai::provider::AiProvider;
    use crate::ai::router_tests::{ok_json, stub, tx};
    use crate::domain::CorrectionMode;
    use crate::telemetry::RedactedSecret;

    #[tokio::test]
    async fn gemini_thinking_modes_step_down_cache_and_skip_thought_parts() {
        let e400 = "HTTP/1.1 400 Bad Request\r\nContent-Length: 3\r\n\r\nbad".to_string();
        let ok = ok_json(
            r#"{"candidates":[{"content":{"parts":[{"thought":true,"text":"secret"},{"text":"{\"spokenReply\":\"Hi!\"}"}]}}]}"#,
        );
        let (p35, s35) = stub(vec![e400.clone(), e400, ok.clone(), ok.clone()]).await;
        let g35 = GeminiProvider::new(
            format!("http://127.0.0.1:{p35}/models"),
            "gemini-3.5-flash-lite",
            RedactedSecret::new("AIzaTest"),
        );
        let r = g35
            .generate_tutor_reply(&[], &tx("Hi"), CorrectionMode::Off)
            .await
            .unwrap();
        assert_eq!(r.spoken_reply, "Hi!");
        g35.generate_tutor_reply(&[], &tx("Again"), CorrectionMode::Off)
            .await
            .unwrap();
        {
            let reqs = s35.lock().unwrap();
            assert_eq!(reqs.len(), 4);
            assert!(reqs[0].contains("\"thinkingLevel\":\"MINIMAL\""));
            assert!(reqs[1].contains("\"thinkingLevel\":\"LOW\""));
            assert!(!reqs[2].contains("thinkingConfig") && !reqs[3].contains("thinkingConfig"));
        }
        let (p25, s25) = stub(vec![ok]).await;
        let g25 = GeminiProvider::new(
            format!("http://127.0.0.1:{p25}/models"),
            "gemini-2.5-flash",
            RedactedSecret::new("AIzaTest"),
        );
        g25.generate_tutor_reply(&[], &tx("Hi"), CorrectionMode::Off)
            .await
            .unwrap();
        assert!(s25.lock().unwrap()[0].contains("\"thinkingBudget\":0"));
        assert!(extract_gemini_text(
            r#"{"candidates":[{"content":{"parts":[{"thought":true,"text":"only"}]}}]}"#
        )
        .is_err());
    }
}
