//! Validated configuration for the local Kokoro / ONNX TTS engine (`T041`).

use crate::domain::{ProviderHealth, SessionError, SessionErrorKind};
use std::path::{Path, PathBuf};

/// Validated configuration for the local Kokoro / ONNX TTS engine (`T041`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KokoroModelConfig {
    executable_path: PathBuf,
    model_path: PathBuf,
    voice_id: String,
}

impl KokoroModelConfig {
    pub fn try_new(
        executable_path: impl Into<PathBuf>,
        model_path: impl Into<PathBuf>,
        voice_id: impl Into<String>,
    ) -> Result<Self, SessionError> {
        let exe = validate_non_empty_path(executable_path.into(), "TTS executable path")?;
        let model = validate_non_empty_path(model_path.into(), "TTS model path")?;
        let voice = validate_voice_id(voice_id.into())?;
        Ok(Self {
            executable_path: exe,
            model_path: model,
            voice_id: voice,
        })
    }

    pub fn executable_path(&self) -> &Path {
        &self.executable_path
    }

    pub fn model_path(&self) -> &Path {
        &self.model_path
    }

    pub fn voice_id(&self) -> &str {
        &self.voice_id
    }

    pub fn verify_files_exist(&self) -> ProviderHealth {
        if self.executable_path.is_file() && self.model_path.is_file() {
            ProviderHealth::Available
        } else {
            ProviderHealth::Misconfigured
        }
    }
}

fn validate_non_empty_path(path: PathBuf, label: &str) -> Result<PathBuf, SessionError> {
    if path.as_os_str().is_empty() {
        return Err(tts_config_err(&format!("{label} must not be empty.")));
    }
    Ok(path)
}

fn validate_voice_id(voice: String) -> Result<String, SessionError> {
    let trimmed = voice.trim();
    if trimmed.is_empty() {
        return Err(tts_config_err("TTS voice ID must not be empty."));
    }
    Ok(trimmed.to_string())
}

pub(super) fn tts_config_err(detail: &str) -> SessionError {
    SessionError::new(
        SessionErrorKind::TtsUnavailable,
        "Local TTS model configuration is invalid.",
        true,
        Some(detail.to_string()),
    )
}
