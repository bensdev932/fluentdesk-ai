//! Local Ollama environment resolution (`OLLAMA_HOST`) and `/api/tags` model discovery.

use crate::ai::{
    ollama::{OllamaAiProvider, OllamaConfig},
    ollama_http,
};
use crate::domain::{ProviderHealth, SessionError, SessionErrorKind};

pub const DEFAULT_OLLAMA_HOST: &str = "127.0.0.1";
pub const DEFAULT_OLLAMA_PORT: u16 = 11434;
pub const DEFAULT_OLLAMA_MODEL: &str = "llama3.2:3b";
const NO_MODELS_MSG: &str =
    "Local Ollama is running, but no models are installed. Pull a model (e.g. `ollama pull llama3.2:3b`) and retry.";

/// Builds a validated `OllamaConfig` from `OLLAMA_HOST` and `FLUENTDESK_OLLAMA_MODEL`.
pub fn ollama_config_from_env() -> Result<OllamaConfig, SessionError> {
    let raw_host = std::env::var("OLLAMA_HOST").ok();
    let (host, port) = parse_ollama_host_env(raw_host.as_deref())?;
    let model = env_model_or_default(std::env::var("FLUENTDESK_OLLAMA_MODEL").ok().as_deref());
    OllamaConfig::try_new(host, port, model)
}

/// Parses `OLLAMA_HOST` (e.g. `0.0.0.0:11434`, `http://127.0.0.1:11434`, `[::]:11434`) to a client endpoint.
pub fn parse_ollama_host_env(raw: Option<&str>) -> Result<(String, u16), SessionError> {
    let trimmed = raw.unwrap_or("").trim();
    if trimmed.is_empty() {
        return Ok((DEFAULT_OLLAMA_HOST.to_string(), DEFAULT_OLLAMA_PORT));
    }
    let without_scheme = trimmed.strip_prefix("http://").unwrap_or(trimmed);
    let cleaned = without_scheme.trim_end_matches('/');
    let (host, port) = split_host_and_port(cleaned)?;
    Ok((normalize_wildcard_host(host), port))
}

/// Selects the preferred model if installed, otherwise auto-selects the best chat model from `/api/tags`.
pub fn select_installed_model(models: &[String], preferred: &str) -> Option<String> {
    find_matching_model(models, preferred)
        .or_else(|| find_non_embedding_model(models))
        .or_else(|| models.first().cloned())
}

/// Discovers an available model from `/api/tags` and returns a ready `OllamaAiProvider`.
pub async fn discover_local_provider(
    base: &OllamaConfig,
) -> Result<OllamaAiProvider, SessionError> {
    let descriptors = ollama_http::list_model_descriptors(base).await?;
    let names: Vec<String> = descriptors.iter().map(|d| d.name.clone()).collect();
    let model = select_installed_model(&names, base.model_name()).ok_or_else(no_models_err)?;
    let supports_thinking = descriptors
        .iter()
        .find(|d| d.name == model)
        .is_some_and(|d| d.supports_thinking);
    let cfg = OllamaConfig::try_new(base.host(), base.port(), model)?;
    let disable_thinking = cfg.disable_thinking() || supports_thinking;
    Ok(OllamaAiProvider::new(
        cfg.with_disable_thinking(disable_thinking),
    ))
}

/// Checks local Ollama reachability and returns the active model name or explicit failure state.
pub async fn check_local_ollama_health(
    base: &OllamaConfig,
) -> Result<String, (ProviderHealth, String)> {
    match ollama_http::list_installed_models(base).await {
        Ok(models) => select_installed_model(&models, base.model_name())
            .ok_or_else(|| (ProviderHealth::Misconfigured, NO_MODELS_MSG.to_string())),
        Err(err) => Err((ProviderHealth::Unavailable, err.user_message)),
    }
}

fn env_model_or_default(raw: Option<&str>) -> String {
    let trimmed = raw.unwrap_or("").trim();
    if trimmed.is_empty() {
        DEFAULT_OLLAMA_MODEL.to_string()
    } else {
        trimmed.to_string()
    }
}

fn split_host_and_port(cleaned: &str) -> Result<(&str, u16), SessionError> {
    if let Some(rest) = cleaned.strip_prefix('[') {
        return split_bracketed_ipv6(rest);
    }
    if cleaned == "::1" || cleaned == "::" {
        return Ok((cleaned, DEFAULT_OLLAMA_PORT));
    }
    split_host_colon_port(cleaned)
}

fn split_bracketed_ipv6(rest: &str) -> Result<(&str, u16), SessionError> {
    let (host, tail) = rest.split_once(']').ok_or_else(invalid_host_err)?;
    if tail.is_empty() {
        return Ok((host, DEFAULT_OLLAMA_PORT));
    }
    let port_str = tail.strip_prefix(':').ok_or_else(invalid_host_err)?;
    Ok((host, parse_port_str(port_str)?))
}

fn split_host_colon_port(cleaned: &str) -> Result<(&str, u16), SessionError> {
    if cleaned.contains("://") {
        return Ok((cleaned, DEFAULT_OLLAMA_PORT));
    }
    match cleaned.rsplit_once(':') {
        Some((host, port_str)) if !host.contains(':') => Ok((host, parse_port_str(port_str)?)),
        _ => Ok((cleaned, DEFAULT_OLLAMA_PORT)),
    }
}

fn parse_port_str(raw: &str) -> Result<u16, SessionError> {
    let port = raw.parse::<u16>().map_err(|_| invalid_host_err())?;
    if port == 0 {
        return Err(invalid_host_err());
    }
    Ok(port)
}

fn normalize_wildcard_host(host: &str) -> String {
    match host.trim() {
        "" | "0.0.0.0" => DEFAULT_OLLAMA_HOST.to_string(),
        "::" => "::1".to_string(),
        other => other.to_string(),
    }
}

fn find_matching_model(models: &[String], preferred: &str) -> Option<String> {
    let tag_prefix = format!("{preferred}:");
    models
        .iter()
        .find(|m| m.as_str() == preferred)
        .or_else(|| models.iter().find(|m| m.starts_with(&tag_prefix)))
        .cloned()
}

fn find_non_embedding_model(models: &[String]) -> Option<String> {
    models.iter().find(|m| !is_embedding_model(m)).cloned()
}

fn is_embedding_model(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.contains("embed") || lower.contains("bge-") || lower.contains("minilm")
}

fn no_models_err() -> SessionError {
    SessionError::new(
        SessionErrorKind::AiProviderUnavailable,
        NO_MODELS_MSG,
        true,
        None,
    )
}

fn invalid_host_err() -> SessionError {
    SessionError::new(
        SessionErrorKind::AiProviderUnavailable,
        "Invalid OLLAMA_HOST port or host format.",
        true,
        None,
    )
}
