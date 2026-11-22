//! Private local whisper.cpp CLI subprocess runner (`T024`).

use crate::domain::{SessionError, SessionErrorKind};
use crate::modules::speech::conversion::encode_pcm16_wav;
use crate::stt::whisper::WhisperModelConfig;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

static NEXT_WAV_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct WhisperProcessOutput {
    pub(super) success: bool,
    pub(super) stdout: Vec<u8>,
}

pub(super) trait WhisperProcessRunner: Send + Sync {
    fn run(
        &self,
        binary_path: &Path,
        args: &[String],
    ) -> Result<WhisperProcessOutput, SessionError>;
}

#[derive(Clone, Default)]
pub(super) struct SystemWhisperProcessRunner;

impl WhisperProcessRunner for SystemWhisperProcessRunner {
    fn run(
        &self,
        binary_path: &Path,
        args: &[String],
    ) -> Result<WhisperProcessOutput, SessionError> {
        let output = Command::new(binary_path)
            .args(args)
            .output()
            .map_err(|e| stt_failed("Failed to spawn local whisper.cpp binary.", e))?;
        Ok(WhisperProcessOutput {
            success: output.status.success(),
            stdout: output.stdout,
        })
    }
}

struct TempWavGuard(PathBuf);

impl Drop for TempWavGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Runs `whisper-cli` on a temporary 16 kHz PCM16 WAV file inside a blocking worker thread.
pub(super) async fn run_whisper_cli(
    runner: Arc<dyn WhisperProcessRunner>,
    binary_path: PathBuf,
    config: WhisperModelConfig,
    samples_16khz: Vec<f32>,
    prompt: Option<String>,
) -> Result<String, SessionError> {
    tokio::task::spawn_blocking(move || {
        let wav_guard = write_temp_wav(&samples_16khz)?;
        invoke_whisper_process(
            &*runner,
            &binary_path,
            &config,
            &wav_guard.0,
            prompt.as_deref(),
        )
    })
    .await
    .map_err(|e| stt_failed("Whisper worker task failed.", e))?
}

fn write_temp_wav(samples_16khz: &[f32]) -> Result<TempWavGuard, SessionError> {
    let wav_bytes = encode_pcm16_wav(samples_16khz, 16_000);
    let seq = NEXT_WAV_ID.fetch_add(1, Ordering::Relaxed);
    let path =
        std::env::temp_dir().join(format!("fluentdesk_utt_{}_{}.wav", std::process::id(), seq));
    std::fs::write(&path, wav_bytes)
        .map_err(|e| stt_failed("Failed to write temporary WAV.", e))?;
    Ok(TempWavGuard(path))
}

fn invoke_whisper_process(
    runner: &dyn WhisperProcessRunner,
    binary_path: &Path,
    config: &WhisperModelConfig,
    wav_path: &Path,
    prompt: Option<&str>,
) -> Result<String, SessionError> {
    let args = build_cli_args(config, wav_path, prompt);
    let output = runner.run(binary_path, &args)?;
    if !output.success {
        return Err(stt_failed("Local whisper.cpp returned non-zero exit.", ""));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn build_cli_args(
    config: &WhisperModelConfig,
    wav_path: &Path,
    prompt: Option<&str>,
) -> Vec<String> {
    let mut args = vec![
        "-m".to_string(),
        config.model_path().to_string_lossy().to_string(),
        "-l".to_string(),
        config.language().to_string(),
        "-t".to_string(),
        config.threads().to_string(),
        "-nt".to_string(),
        "--no-speech-thold".to_string(),
        "0.60".to_string(),
        "--suppress-nst".to_string(),
        "-f".to_string(),
        wav_path.to_string_lossy().to_string(),
    ];
    if let Some(p) = prompt.map(str::trim).filter(|s| !s.is_empty()) {
        args.push("--prompt".to_string());
        args.push(p.to_string());
    }
    args
}

fn stt_failed(msg: &'static str, detail: impl std::fmt::Display) -> SessionError {
    SessionError::new(
        SessionErrorKind::SttFailed,
        msg,
        true,
        Some(detail.to_string()),
    )
}
