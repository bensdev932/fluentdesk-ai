//! Local whisper.cpp engine adapter and model configuration (`T024`, `T025`).

use crate::domain::{ProviderHealth, SessionError, SessionErrorKind, Transcript};
use crate::modules::speech::utterance::CompletedUtterance;
use crate::stt::provider::SpeechToText;
use crate::stt::sidecar::{run_whisper_cli, SystemWhisperProcessRunner, WhisperProcessRunner};
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Validated model configuration for local Whisper engine (`T025`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WhisperModelConfig {
    model_path: PathBuf,
    language: String,
    threads: u8,
}

impl WhisperModelConfig {
    pub fn try_new(
        model_path: impl Into<PathBuf>,
        language: impl Into<String>,
        threads: u8,
    ) -> Result<Self, SessionError> {
        let lang = language.into().trim().to_string();
        if lang.is_empty() || !(1..=32).contains(&threads) {
            return Err(unavailable_err("Invalid Whisper language or thread count."));
        }
        Ok(Self {
            model_path: model_path.into(),
            language: lang,
            threads,
        })
    }

    pub fn model_path(&self) -> &Path {
        &self.model_path
    }

    pub fn language(&self) -> &str {
        &self.language
    }

    pub fn threads(&self) -> u8 {
        self.threads
    }

    pub fn validate_file_exists(&self) -> Result<(), SessionError> {
        let meta = std::fs::metadata(&self.model_path)
            .map_err(|_| unavailable_err("Whisper model file was not found."))?;
        if !meta.is_file() || meta.len() == 0 {
            return Err(unavailable_err("Whisper model file is empty or invalid."));
        }
        Ok(())
    }
}

/// Local Whisper adapter running via local `whisper-cli` binary and GGML model (`T024`).
#[derive(Clone)]
pub struct LocalWhisperAdapter {
    binary_path: PathBuf,
    config: WhisperModelConfig,
    runner: Arc<dyn WhisperProcessRunner>,
}

impl LocalWhisperAdapter {
    pub fn new(binary_path: impl Into<PathBuf>, config: WhisperModelConfig) -> Self {
        Self::new_with_runner(binary_path, config, Arc::new(SystemWhisperProcessRunner))
    }

    pub(super) fn new_with_runner(
        binary_path: impl Into<PathBuf>,
        config: WhisperModelConfig,
        runner: Arc<dyn WhisperProcessRunner>,
    ) -> Self {
        Self {
            binary_path: binary_path.into(),
            config,
            runner,
        }
    }

    pub fn binary_path(&self) -> &Path {
        &self.binary_path
    }

    pub fn config(&self) -> &WhisperModelConfig {
        &self.config
    }
}

#[async_trait]
impl SpeechToText for LocalWhisperAdapter {
    async fn health_check(&self) -> ProviderHealth {
        if self.config.validate_file_exists().is_ok() && binary_is_configured(&self.binary_path) {
            ProviderHealth::Available
        } else {
            ProviderHealth::Unavailable
        }
    }

    async fn transcribe(&self, utterance: &CompletedUtterance) -> Result<Transcript, SessionError> {
        self.transcribe_with_prompt(utterance, None).await
    }

    async fn transcribe_with_context(
        &self,
        utterance: &CompletedUtterance,
        context: Option<&crate::stt::provider::SttPromptContext>,
    ) -> Result<Transcript, SessionError> {
        let prompt =
            context.and_then(crate::stt::provider::SttPromptContext::format_whisper_prompt);
        self.transcribe_with_prompt(utterance, prompt).await
    }
}

impl LocalWhisperAdapter {
    async fn transcribe_with_prompt(
        &self,
        utterance: &CompletedUtterance,
        prompt: Option<String>,
    ) -> Result<Transcript, SessionError> {
        self.config.validate_file_exists()?;
        let samples_16k = utterance.samples_16khz_mono().to_vec();
        let text = run_whisper_cli(
            self.runner.clone(),
            self.binary_path.clone(),
            self.config.clone(),
            samples_16k,
            prompt,
        )
        .await?;
        Transcript::new(
            text,
            self.config.language(),
            utterance.duration_ms(),
            Some(0.92),
        )
        .map_err(Into::into)
    }
}

fn binary_is_configured(binary_path: &Path) -> bool {
    if binary_path.as_os_str().is_empty() {
        return false;
    }
    if binary_path.components().count() > 1 {
        return binary_path.is_file();
    }
    true
}

fn unavailable_err(msg: &'static str) -> SessionError {
    SessionError::new(SessionErrorKind::SttUnavailable, msg, true, None)
}
