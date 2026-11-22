//! Private local TTS CLI subprocess runner (`T041`).

use crate::domain::{SessionError, SessionErrorKind};
use crate::tts::kokoro::KokoroModelConfig;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TtsProcessOutput {
    pub(super) success: bool,
    pub(super) stdout: Vec<u8>,
}

pub(super) trait TtsProcessRunner: Send + Sync {
    fn run(
        &self,
        executable_path: &Path,
        args: &[String],
    ) -> Result<TtsProcessOutput, SessionError>;
}

#[derive(Clone, Default)]
pub(super) struct SystemTtsProcessRunner;

impl TtsProcessRunner for SystemTtsProcessRunner {
    fn run(
        &self,
        executable_path: &Path,
        args: &[String],
    ) -> Result<TtsProcessOutput, SessionError> {
        let output = Command::new(executable_path)
            .args(args)
            .output()
            .map_err(|_| sidecar_exec_err("Failed to launch local TTS sidecar binary."))?;
        Ok(TtsProcessOutput {
            success: output.status.success(),
            stdout: output.stdout,
        })
    }
}

/// Synthesizes one sentence into raw WAV bytes via local Kokoro / Piper CLI in a blocking thread.
pub(super) async fn synthesize_sentence_wav(
    runner: Arc<dyn TtsProcessRunner>,
    config: KokoroModelConfig,
    sentence: String,
    speed: f32,
) -> Result<Vec<u8>, SessionError> {
    tokio::task::spawn_blocking(move || invoke_tts_process(&*runner, &config, &sentence, speed))
        .await
        .map_err(|_| sidecar_exec_err("Local TTS worker task failed."))?
}

fn invoke_tts_process(
    runner: &dyn TtsProcessRunner,
    config: &KokoroModelConfig,
    sentence: &str,
    speed: f32,
) -> Result<Vec<u8>, SessionError> {
    let args = build_tts_args(config, sentence, speed);
    let output = runner.run(config.executable_path(), &args)?;
    if !output.success {
        return Err(sidecar_exec_err(
            "Local TTS sidecar exited with non-zero status.",
        ));
    }
    Ok(output.stdout)
}

fn build_tts_args(config: &KokoroModelConfig, sentence: &str, speed: f32) -> Vec<String> {
    vec![
        "--model".to_string(),
        config.model_path().to_string_lossy().to_string(),
        "--voice".to_string(),
        config.voice_id().to_string(),
        "--speed".to_string(),
        format!("{speed:.2}"),
        "--text".to_string(),
        sentence.to_string(),
    ]
}

fn sidecar_exec_err(reason: &str) -> SessionError {
    SessionError::new(
        SessionErrorKind::TtsFailed,
        "Local voice synthesis failed.",
        true,
        Some(reason.to_string()),
    )
}
