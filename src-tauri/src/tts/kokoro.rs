//! Local Kokoro / ONNX TTS adapter and streaming sentence synthesizer (`T040`, `T041`).

use crate::domain::{AudioChunk, AudioFormat, ProviderHealth, SessionError};
use crate::modules::speech::{
    conversion::decode_wav_bytes_to_audio_chunk, spoken_text::split_spoken_sentences,
};
use crate::tts::kokoro_config::tts_config_err;
pub use crate::tts::kokoro_config::KokoroModelConfig;
use crate::tts::{
    provider::{SpeakingSpeed, TextToSpeech, TtsAudioStream},
    sidecar::{synthesize_sentence_wav, SystemTtsProcessRunner, TtsProcessRunner},
};
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::mpsc;

/// Local neural TTS adapter implementing `TextToSpeech` (`T041`).
#[derive(Clone)]
pub struct LocalKokoroTtsAdapter {
    config: KokoroModelConfig,
    runner: Arc<dyn TtsProcessRunner>,
}

impl LocalKokoroTtsAdapter {
    pub fn new(config: KokoroModelConfig) -> Self {
        Self::new_with_runner(config, Arc::new(SystemTtsProcessRunner))
    }

    pub(super) fn new_with_runner(
        config: KokoroModelConfig,
        runner: Arc<dyn TtsProcessRunner>,
    ) -> Self {
        Self { config, runner }
    }

    pub fn config(&self) -> &KokoroModelConfig {
        &self.config
    }
}

#[async_trait]
impl TextToSpeech for LocalKokoroTtsAdapter {
    async fn health_check(&self) -> ProviderHealth {
        self.config.verify_files_exist()
    }

    async fn synthesize(
        &self,
        spoken_text: &str,
        speaking_speed: f32,
    ) -> Result<AudioChunk, SessionError> {
        let mut rx = self.synthesize_stream(spoken_text, speaking_speed).await?;
        let mut chunks = Vec::new();
        while let Some(item) = rx.recv().await {
            chunks.push(item?);
        }
        assemble_synthesized_chunks(&chunks)
    }

    async fn synthesize_stream(
        &self,
        spoken_text: &str,
        speaking_speed: f32,
    ) -> Result<TtsAudioStream, SessionError> {
        let (speed, sentences) = prepare_sentences(spoken_text, speaking_speed)?;
        let (tx, rx) = mpsc::channel(1);
        tokio::spawn(stream_sentences(
            tx,
            self.runner.clone(),
            self.config.clone(),
            sentences,
            speed.as_f32(),
        ));
        Ok(rx)
    }
}

fn prepare_sentences(
    spoken_text: &str,
    speaking_speed: f32,
) -> Result<(SpeakingSpeed, Vec<String>), SessionError> {
    let speed = SpeakingSpeed::try_new(speaking_speed)?;
    let sentences = split_spoken_sentences(spoken_text);
    if sentences.is_empty() {
        return Err(tts_config_err("Cannot synthesize empty spoken text."));
    }
    Ok((speed, sentences))
}

async fn synthesize_sentence_chunk(
    runner: Arc<dyn TtsProcessRunner>,
    config: KokoroModelConfig,
    sentence: String,
    speed: f32,
) -> Result<AudioChunk, SessionError> {
    let wav = synthesize_sentence_wav(runner, config, sentence, speed).await?;
    decode_wav_bytes_to_audio_chunk(&wav)
}

async fn stream_sentences(
    tx: mpsc::Sender<Result<AudioChunk, SessionError>>,
    runner: Arc<dyn TtsProcessRunner>,
    config: KokoroModelConfig,
    sentences: Vec<String>,
    speed: f32,
) {
    for sentence in sentences {
        let Ok(permit) = tx.reserve().await else {
            break;
        };
        if !send_next_sentence(&tx, permit, &runner, &config, sentence, speed).await {
            break;
        }
    }
}

async fn send_next_sentence(
    tx: &mpsc::Sender<Result<AudioChunk, SessionError>>,
    permit: mpsc::Permit<'_, Result<AudioChunk, SessionError>>,
    runner: &Arc<dyn TtsProcessRunner>,
    config: &KokoroModelConfig,
    sentence: String,
    speed: f32,
) -> bool {
    let chunk = tokio::select! {
        biased;
        _ = tx.closed() => return false,
        res = synthesize_sentence_chunk(runner.clone(), config.clone(), sentence, speed) => res,
    };
    let ok = chunk.is_ok();
    permit.send(chunk);
    ok
}

fn assemble_synthesized_chunks(chunks: &[AudioChunk]) -> Result<AudioChunk, SessionError> {
    let first = chunks
        .first()
        .ok_or_else(|| tts_config_err("Cannot assemble empty audio chunks."))?;
    let combined = combine_matching_chunks(chunks, first.format)?;
    let frames = (combined.len() as u64) / u64::from(first.format.channels.max(1));
    let duration_ms = ((frames * 1000) / u64::from(first.format.sample_rate_hz)).max(1) as u32;
    AudioChunk::new_synthesized(first.format, combined, 0, duration_ms)
        .map_err(|e| tts_config_err(&e.to_string()))
}

fn combine_matching_chunks(
    chunks: &[AudioChunk],
    expected: AudioFormat,
) -> Result<Vec<f32>, SessionError> {
    let mut combined = Vec::new();
    for chunk in chunks {
        if chunk.format != expected {
            return Err(tts_config_err("Synthesized sentence format mismatch."));
        }
        combined.extend_from_slice(&chunk.samples_f32);
    }
    Ok(combined)
}
