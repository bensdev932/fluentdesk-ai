//! Shared test doubles for streaming TTS playback and cancellation tests.

use crate::domain::{AudioChunk, SessionError, SessionErrorKind};
use crate::modules::speech::{conversion::encode_pcm16_wav, playback::AudioOutput};
use crate::tts::{
    kokoro::KokoroModelConfig,
    sidecar::{TtsProcessOutput, TtsProcessRunner},
};
use async_trait::async_trait;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{mpsc as std_mpsc, Arc, Mutex};
use tokio::sync::{mpsc, Notify};

pub(super) type TtsCall = (PathBuf, Vec<String>);

#[derive(Clone, Default)]
pub(super) struct ScriptedTtsRunner {
    outcomes: Arc<Mutex<VecDeque<Result<TtsProcessOutput, SessionError>>>>,
    calls: Arc<Mutex<Vec<TtsCall>>>,
}

impl ScriptedTtsRunner {
    pub(super) fn with_outcomes(outcomes: Vec<Result<TtsProcessOutput, SessionError>>) -> Self {
        Self {
            outcomes: Arc::new(Mutex::new(VecDeque::from(outcomes))),
            calls: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub(super) fn calls(&self) -> Vec<TtsCall> {
        self.calls.lock().expect("lock tts calls").clone()
    }
}

impl TtsProcessRunner for ScriptedTtsRunner {
    fn run(&self, exe: &Path, args: &[String]) -> Result<TtsProcessOutput, SessionError> {
        self.calls
            .lock()
            .expect("lock tts calls")
            .push((exe.to_path_buf(), args.to_vec()));
        self.outcomes
            .lock()
            .expect("lock tts outcomes")
            .pop_front()
            .expect("queued tts outcome")
    }
}

pub(super) struct GatedRunner {
    pub(super) calls: Arc<Mutex<Vec<String>>>,
    pub(super) gate_sentence_idx: Option<usize>,
    pub(super) gate_rx: Mutex<Option<std_mpsc::Receiver<()>>>,
    pub(super) fail_sentence_idx: Option<usize>,
    pub(super) started_tx: Option<mpsc::UnboundedSender<usize>>,
}

impl TtsProcessRunner for GatedRunner {
    fn run(&self, _exe: &Path, args: &[String]) -> Result<TtsProcessOutput, SessionError> {
        let idx = record_sentence_call(&self.calls, args, &self.started_tx);
        wait_on_sentence_gate(self.gate_sentence_idx, idx, &self.gate_rx);
        if self.fail_sentence_idx == Some(idx) {
            return Ok(TtsProcessOutput {
                success: false,
                stdout: Vec::new(),
            });
        }
        Ok(TtsProcessOutput {
            success: true,
            stdout: encode_pcm16_wav(&vec![0.1; 2_400], 24_000),
        })
    }
}

fn record_sentence_call(
    calls: &Arc<Mutex<Vec<String>>>,
    args: &[String],
    started_tx: &Option<mpsc::UnboundedSender<usize>>,
) -> usize {
    let sentence = args.last().cloned().unwrap_or_default();
    let idx = {
        let mut guard = calls.lock().expect("lock calls");
        guard.push(sentence);
        guard.len()
    };
    if let Some(tx) = started_tx {
        let _ = tx.send(idx);
    }
    idx
}

fn wait_on_sentence_gate(
    gate_idx: Option<usize>,
    idx: usize,
    gate_rx: &Mutex<Option<std_mpsc::Receiver<()>>>,
) {
    if gate_idx == Some(idx) {
        if let Some(rx) = gate_rx.lock().expect("lock gate").take() {
            let _ = rx.recv();
        }
    }
}

pub(super) struct SpyAudioOutput {
    pub(super) played: Arc<Mutex<Vec<AudioChunk>>>,
    pub(super) stopped: Arc<Mutex<usize>>,
    pub(super) on_play_tx: Option<mpsc::UnboundedSender<usize>>,
    pub(super) block_chunk_idx: Option<usize>,
    pub(super) block_notify: Option<Arc<Notify>>,
    pub(super) fail_chunk_idx: Option<usize>,
}

#[async_trait]
impl AudioOutput for SpyAudioOutput {
    async fn play_audio(&self, audio: &AudioChunk) -> Result<(), SessionError> {
        let idx = record_played_chunk(&self.played, audio, &self.on_play_tx);
        if self.fail_chunk_idx == Some(idx) {
            return Err(SessionError::new(
                SessionErrorKind::PlaybackFailed,
                "speaker unplugged",
                true,
                None,
            ));
        }
        if self.block_chunk_idx == Some(idx) {
            if let Some(n) = &self.block_notify {
                n.notified().await;
            }
        }
        Ok(())
    }

    async fn stop_playback(&self) -> Result<(), SessionError> {
        *self.stopped.lock().expect("lock stopped") += 1;
        if let Some(n) = &self.block_notify {
            n.notify_waiters();
        }
        Ok(())
    }

    fn is_playing(&self) -> bool {
        false
    }
}

fn record_played_chunk(
    played: &Arc<Mutex<Vec<AudioChunk>>>,
    audio: &AudioChunk,
    on_play_tx: &Option<mpsc::UnboundedSender<usize>>,
) -> usize {
    let mut guard = played.lock().expect("lock played");
    guard.push(audio.clone());
    let idx = guard.len();
    if let Some(tx) = on_play_tx {
        let _ = tx.send(idx);
    }
    idx
}

pub(super) fn test_kokoro_config() -> KokoroModelConfig {
    KokoroModelConfig::try_new("kokoro.exe", "model.onnx", "af_bella").unwrap()
}
