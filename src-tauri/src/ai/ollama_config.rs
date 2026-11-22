//! Validated localhost configuration value object for the local Ollama service (`T031`).

use crate::domain::{SessionError, SessionErrorKind};

/// Validated localhost configuration for the local Ollama service (`T031`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OllamaConfig {
    host: String,
    port: u16,
    model_name: String,
    disable_thinking: bool,
}

impl OllamaConfig {
    pub fn try_new(
        host: impl Into<String>,
        port: u16,
        model_name: impl Into<String>,
    ) -> Result<Self, SessionError> {
        let cleaned_host = validate_localhost(host.into())?;
        let cleaned_port = validate_port(port)?;
        let cleaned_model = validate_model_name(model_name.into())?;
        let disable_thinking = is_thinking_model_name(&cleaned_model);
        Ok(Self {
            host: cleaned_host,
            port: cleaned_port,
            model_name: cleaned_model,
            disable_thinking,
        })
    }

    pub fn with_disable_thinking(mut self, disable: bool) -> Self {
        self.disable_thinking = disable;
        self
    }

    pub fn disable_thinking(&self) -> bool {
        self.disable_thinking
    }

    pub fn host(&self) -> &str {
        &self.host
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn model_name(&self) -> &str {
        &self.model_name
    }

    /// Formats the validated localhost endpoint as `host:port` (bracketing IPv6 `::1`).
    pub fn socket_addr(&self) -> String {
        if self.host.contains(':') && !self.host.starts_with('[') {
            format!("[{}]:{}", self.host, self.port)
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }
}

fn validate_localhost(raw_host: String) -> Result<String, SessionError> {
    let trimmed = raw_host.trim().to_ascii_lowercase();
    if trimmed == "127.0.0.1" || trimmed == "localhost" || trimmed == "::1" {
        return Ok(trimmed);
    }
    Err(config_err(
        "Ollama host must be localhost (127.0.0.1, localhost, or ::1).",
    ))
}

fn validate_port(port: u16) -> Result<u16, SessionError> {
    if port == 0 {
        return Err(config_err("Ollama port must be greater than 0."));
    }
    Ok(port)
}

fn validate_model_name(raw_model: String) -> Result<String, SessionError> {
    let trimmed = raw_model.trim();
    if trimmed.is_empty() {
        return Err(config_err("Ollama model name must not be empty."));
    }
    Ok(trimmed.to_string())
}

fn is_thinking_model_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.contains("qwen3") || lower.contains("deepseek-r1") || lower.contains("qwq")
}

fn config_err(reason: &str) -> SessionError {
    SessionError::new(SessionErrorKind::AiProviderUnavailable, reason, true, None)
}
