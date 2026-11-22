//! Minimal localhost HTTP/1.1 client for the local Ollama daemon (`T031`).

use crate::ai::{ollama::OllamaConfig, ollama_http_codec::parse_http_response};
use crate::domain::{SessionError, SessionErrorKind};
use crate::modules::tutor::prompts::TutorPrompt;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

const OLLAMA_CONNECT_TIMEOUT_SECS: u64 = 1;
const OLLAMA_PROBE_TIMEOUT_SECS: u64 = 2;
pub(crate) const OLLAMA_GENERATE_TIMEOUT_SECS: u64 = 60;

#[derive(Deserialize)]
struct OllamaTagsResponse {
    models: Vec<OllamaModelEntry>,
}

#[derive(Deserialize)]
struct OllamaModelEntry {
    name: String,
    #[serde(default)]
    capabilities: Vec<String>,
}

/// Discovered local Ollama model descriptor returned by `/api/tags`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OllamaModelDescriptor {
    pub name: String,
    pub supports_thinking: bool,
}

#[derive(Serialize)]
struct OllamaGenerateRequest<'a> {
    model: &'a str,
    system: &'a str,
    prompt: &'a str,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    think: Option<bool>,
}

#[derive(Deserialize)]
struct OllamaGenerateResponse {
    response: String,
}

/// Lists installed models from local `ollama serve` via `GET /api/tags` (`T031`).
pub(crate) async fn list_installed_models(cfg: &OllamaConfig) -> Result<Vec<String>, SessionError> {
    let descriptors = list_model_descriptors(cfg).await?;
    Ok(descriptors.into_iter().map(|m| m.name).collect())
}

/// Lists installed model descriptors including Ollama capabilities from `GET /api/tags`.
pub(crate) async fn list_model_descriptors(
    cfg: &OllamaConfig,
) -> Result<Vec<OllamaModelDescriptor>, SessionError> {
    let body = send_http_request(cfg, "GET", "/api/tags", "", OLLAMA_PROBE_TIMEOUT_SECS).await?;
    let parsed: OllamaTagsResponse =
        serde_json::from_str(&body).map_err(|e| parse_err(e.to_string()))?;
    Ok(parsed
        .models
        .into_iter()
        .map(map_model_descriptor)
        .collect())
}

fn map_model_descriptor(entry: OllamaModelEntry) -> OllamaModelDescriptor {
    let supports_thinking = entry
        .capabilities
        .iter()
        .any(|c| c.eq_ignore_ascii_case("thinking"));
    OllamaModelDescriptor {
        name: entry.name,
        supports_thinking,
    }
}

/// Generates a non-streaming completion from `ollama serve` via `POST /api/generate` (`T031`).
pub(crate) async fn generate_completion(
    cfg: &OllamaConfig,
    prompt: &TutorPrompt,
) -> Result<String, SessionError> {
    let payload = serialize_generate_request(cfg, prompt)?;
    let body = send_http_request(
        cfg,
        "POST",
        "/api/generate",
        &payload,
        OLLAMA_GENERATE_TIMEOUT_SECS,
    )
    .await?;
    let parsed: OllamaGenerateResponse =
        serde_json::from_str(&body).map_err(|e| parse_err(e.to_string()))?;
    Ok(parsed.response)
}

fn serialize_generate_request(
    cfg: &OllamaConfig,
    prompt: &TutorPrompt,
) -> Result<String, SessionError> {
    serialize_generate_request_with_stream(cfg, prompt, false)
}

pub(crate) fn serialize_generate_request_with_stream(
    cfg: &OllamaConfig,
    prompt: &TutorPrompt,
    stream: bool,
) -> Result<String, SessionError> {
    let req = OllamaGenerateRequest {
        model: cfg.model_name(),
        system: prompt.system_instruction(),
        prompt: prompt.user_prompt(),
        stream,
        think: cfg.disable_thinking().then_some(false),
    };
    serde_json::to_string(&req).map_err(|e| parse_err(e.to_string()))
}

async fn send_http_request(
    cfg: &OllamaConfig,
    method: &str,
    path: &str,
    body: &str,
    timeout_secs: u64,
) -> Result<String, SessionError> {
    let addr = cfg.socket_addr();
    let req = format_http_request(&addr, method, path, body);
    tokio::time::timeout(
        Duration::from_secs(timeout_secs),
        execute_http_exchange(&addr, &req),
    )
    .await
    .map_err(|_| timeout_err())?
}

pub(crate) fn format_http_request(addr: &str, method: &str, path: &str, body: &str) -> String {
    format!(
        "{method} {path} HTTP/1.1\r\nHost: {addr}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

async fn execute_http_exchange(addr: &str, req: &str) -> Result<String, SessionError> {
    let mut stream = connect_with_timeout(addr).await?;
    stream.write_all(req.as_bytes()).await.map_err(io_err)?;
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await.map_err(io_err)?;
    parse_http_response(&buf)
}

pub(crate) async fn connect_with_timeout(addr: &str) -> Result<TcpStream, SessionError> {
    tokio::time::timeout(
        Duration::from_secs(OLLAMA_CONNECT_TIMEOUT_SECS),
        TcpStream::connect(addr),
    )
    .await
    .map_err(|_| timeout_err())?
    .map_err(io_err)
}

pub(crate) fn timeout_err() -> SessionError {
    SessionError::new(
        SessionErrorKind::AiProviderUnavailable,
        "Timed out waiting for local Ollama service.",
        true,
        None,
    )
}

pub(crate) fn io_err(err: std::io::Error) -> SessionError {
    SessionError::new(
        SessionErrorKind::AiProviderUnavailable,
        "Could not connect to local Ollama service on localhost.",
        true,
        Some(err.to_string()),
    )
}

pub(crate) fn parse_err(detail: String) -> SessionError {
    SessionError::new(
        SessionErrorKind::AiInferenceFailed,
        "Unreadable response from local Ollama service.",
        true,
        Some(detail),
    )
}
