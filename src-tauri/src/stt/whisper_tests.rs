//! Tests for encapsulated `LocalWhisperAdapter` execution through `SpeechToText`.

use crate::domain::{ProviderHealth, SessionError, SessionErrorKind};
use crate::modules::speech::utterance::CompletedUtterance;
use crate::stt::{
    provider::SpeechToText,
    sidecar::{WhisperProcessOutput, WhisperProcessRunner},
    whisper::{LocalWhisperAdapter, WhisperModelConfig},
};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

struct TempFileGuard(PathBuf);
impl Drop for TempFileGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

type WhisperCall = (PathBuf, Vec<String>, Vec<u8>);

#[derive(Clone, Default)]
struct FakeWhisperRunner {
    outcomes: Arc<Mutex<VecDeque<Result<WhisperProcessOutput, SessionError>>>>,
    calls: Arc<Mutex<Vec<WhisperCall>>>,
}

impl FakeWhisperRunner {
    fn with_outcomes(outcomes: Vec<Result<WhisperProcessOutput, SessionError>>) -> Self {
        Self {
            outcomes: Arc::new(Mutex::new(VecDeque::from(outcomes))),
            calls: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn calls(&self) -> Vec<WhisperCall> {
        self.calls.lock().expect("lock calls").clone()
    }
}

impl WhisperProcessRunner for FakeWhisperRunner {
    fn run(&self, bin: &Path, args: &[String]) -> Result<WhisperProcessOutput, SessionError> {
        let wav = args
            .last()
            .and_then(|p| std::fs::read(p).ok())
            .unwrap_or_default();
        self.calls
            .lock()
            .expect("lock calls")
            .push((bin.to_path_buf(), args.to_vec(), wav));
        self.outcomes
            .lock()
            .expect("lock outcomes")
            .pop_front()
            .expect("queued whisper outcome")
    }
}

fn create_temp_model_file(name: &str) -> (PathBuf, TempFileGuard) {
    let path = std::env::temp_dir().join(format!("{name}_{}.bin", std::process::id()));
    std::fs::write(&path, b"MOCK_GGML_DATA").unwrap();
    (path.clone(), TempFileGuard(path))
}

#[tokio::test]
async fn local_whisper_adapter_runs_cli_with_wav_fixture_and_cleans_temp_file() {
    let (model_path, _guard) = create_temp_model_file("whisper_m");
    let cfg = WhisperModelConfig::try_new(&model_path, "en", 4).unwrap();
    let runner = Arc::new(FakeWhisperRunner::with_outcomes(vec![Ok(
        WhisperProcessOutput {
            success: true,
            stdout: b"  Hello from local Whisper. \n".to_vec(),
        },
    )]));
    let exe = std::env::current_exe().unwrap();
    let adapter = LocalWhisperAdapter::new_with_runner(&exe, cfg, runner.clone());
    assert_eq!(adapter.health_check().await, ProviderHealth::Available);

    let utt = CompletedUtterance::try_new(vec![0.05; 320], 20, false).unwrap();
    let tx = adapter.transcribe(&utt).await.expect("transcribe");
    assert_eq!(
        (tx.text.as_str(), tx.language.as_str(), tx.duration_ms),
        ("Hello from local Whisper.", "en", 20)
    );
    let calls = runner.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0, exe);
    assert_eq!(
        &calls[0].1[..11],
        [
            "-m",
            &model_path.to_string_lossy(),
            "-l",
            "en",
            "-t",
            "4",
            "-nt",
            "--no-speech-thold",
            "0.60",
            "--suppress-nst",
            "-f"
        ]
    );
    assert_eq!(
        (&calls[0].2[0..4], &calls[0].2[8..12]),
        (b"RIFF".as_slice(), b"WAVE".as_slice())
    );
    assert!(!Path::new(&calls[0].1[11]).exists());
}

#[tokio::test]
async fn local_whisper_adapter_maps_exit_spawn_and_missing_model_errors() {
    let (model_path, _guard) = create_temp_model_file("whisper_err");
    let cfg = WhisperModelConfig::try_new(&model_path, "en", 2).unwrap();
    let runner = Arc::new(FakeWhisperRunner::with_outcomes(vec![
        Ok(WhisperProcessOutput {
            success: false,
            stdout: Vec::new(),
        }),
        Err(SessionError::new(
            SessionErrorKind::SttFailed,
            "spawn failed",
            true,
            None,
        )),
    ]));
    let exe = std::env::current_exe().unwrap();
    let adapter = LocalWhisperAdapter::new_with_runner(&exe, cfg, runner.clone());
    let utt = CompletedUtterance::try_new(vec![0.05; 320], 20, false).unwrap();
    assert_eq!(
        adapter.transcribe(&utt).await.unwrap_err().kind,
        SessionErrorKind::SttFailed
    );
    assert_eq!(
        adapter.transcribe(&utt).await.unwrap_err().kind,
        SessionErrorKind::SttFailed
    );

    let missing_cfg = WhisperModelConfig::try_new("missing_whisper_model.bin", "en", 2).unwrap();
    let missing_adapter = LocalWhisperAdapter::new_with_runner(&exe, missing_cfg, runner.clone());
    assert_eq!(
        missing_adapter.health_check().await,
        ProviderHealth::Unavailable
    );
    assert_eq!(
        missing_adapter.transcribe(&utt).await.unwrap_err().kind,
        SessionErrorKind::SttUnavailable
    );
    assert_eq!(runner.calls().len(), 2);

    let unconfigured = LocalWhisperAdapter::new(
        std::env::temp_dir().join("missing-whisper-cli.exe"),
        WhisperModelConfig::try_new(&model_path, "en", 2).unwrap(),
    );
    assert_eq!(
        unconfigured.health_check().await,
        ProviderHealth::Unavailable
    );
    assert_eq!(
        unconfigured.transcribe(&utt).await.unwrap_err().kind,
        SessionErrorKind::SttFailed
    );
}

#[tokio::test]
#[ignore = "Requires local whisper-cli binary (FLUENTDESK_WHISPER_BIN) and model (FLUENTDESK_WHISPER_MODEL)"]
async fn real_whisper_cli_transcribes_wav_fixture_when_configured() {
    let bin = std::env::var("FLUENTDESK_WHISPER_BIN").expect("FLUENTDESK_WHISPER_BIN not set");
    let model =
        std::env::var("FLUENTDESK_WHISPER_MODEL").expect("FLUENTDESK_WHISPER_MODEL not set");
    let cfg = WhisperModelConfig::try_new(model, "en", 4).unwrap();
    let adapter = LocalWhisperAdapter::new(bin, cfg);
    assert_eq!(adapter.health_check().await, ProviderHealth::Available);
}
