//! Unified Speech Intake and Turn Detector (`T018`–`T020`, `T023`, `T026`).

pub use crate::modules::speech::intake_config::{
    SpeechIntakeConfig, TurnDetectionEvent, VadEndpointConfig,
};
pub use crate::modules::speech::transcription::{
    is_noise_or_hallucination, transcribe_completed_utterance,
    transcribe_completed_utterance_with_context, UserTranscriptReadyEvent,
};
pub use crate::modules::speech::utterance::{
    CompletedUtterance, MaxUtteranceDurationMs, DEFAULT_MAX_UTTERANCE_MS,
    MAX_UTTERANCE_SAMPLES_16KHZ,
};

use crate::domain::{AudioChunk, CefrLevel, SessionError};
use crate::modules::speech::intake_config::invalid_cfg_err;
use crate::modules::speech::intake_processing::format_err;
use crate::modules::speech::utterance::BoundedUtteranceBuffer;
use crate::stt::provider::SpeechToText;
use std::collections::VecDeque;

/// Deep unified Speech Intake / Turn Detector combining VAD hangover, push-to-talk,
/// single max-duration cap, bounded 16 kHz mono accumulation, and STT noise filtering.
#[derive(Debug)]
pub struct SpeechIntake {
    pub(super) config: SpeechIntakeConfig,
    pub(super) buffer: BoundedUtteranceBuffer,
    pub(super) pre_roll: VecDeque<f32>,
    pub(super) speech_ms: u32,
    pub(super) trailing_silence_ms: u32,
    pub(super) in_speech: bool,
    pub(super) ptt_held: bool,
}

impl SpeechIntake {
    pub fn new(config: SpeechIntakeConfig) -> Self {
        let max_samples = config.max_duration().max_samples_at_16khz();
        Self {
            config,
            buffer: BoundedUtteranceBuffer::new(max_samples),
            pre_roll: VecDeque::new(),
            speech_ms: 0,
            trailing_silence_ms: 0,
            in_speech: false,
            ptt_held: false,
        }
    }

    pub fn for_session(push_to_talk: bool) -> Self {
        Self::new(SpeechIntakeConfig::for_session(push_to_talk))
    }

    pub fn for_session_with_cefr(push_to_talk: bool, level: CefrLevel) -> Self {
        Self::new(SpeechIntakeConfig::for_session_with_cefr(
            push_to_talk,
            level,
        ))
    }

    pub fn is_push_to_talk(&self) -> bool {
        self.config.is_push_to_talk()
    }

    pub fn is_ptt_held(&self) -> bool {
        self.ptt_held
    }

    pub fn set_ptt_held(&mut self, held: bool) -> Result<Option<CompletedUtterance>, SessionError> {
        if !self.is_push_to_talk() {
            return Err(invalid_cfg_err(
                "Push-to-talk is not enabled for this session.",
            ));
        }
        if held {
            self.begin_ptt_press();
            return Ok(None);
        }
        Ok(self.release_ptt())
    }

    pub fn process_chunk(
        &mut self,
        chunk: &AudioChunk,
    ) -> Result<TurnDetectionEvent, SessionError> {
        chunk.format.validate().map_err(format_err)?;
        match self.config {
            SpeechIntakeConfig::PushToTalk(_) => self.process_ptt_chunk(chunk),
            SpeechIntakeConfig::VoiceActivity(vad) => self.process_vad_chunk(vad, chunk),
        }
    }

    pub fn flush_on_stop(&mut self) -> Option<CompletedUtterance> {
        let out = self
            .should_flush_on_stop()
            .then(|| self.buffer.take_utterance())
            .flatten();
        self.reset();
        out
    }

    pub fn reset(&mut self) {
        self.buffer.clear();
        self.pre_roll.clear();
        self.speech_ms = 0;
        self.trailing_silence_ms = 0;
        self.in_speech = false;
        self.ptt_held = false;
    }

    pub async fn transcribe(
        &self,
        stt: &dyn SpeechToText,
        utterance: &CompletedUtterance,
    ) -> Result<UserTranscriptReadyEvent, SessionError> {
        self.transcribe_with_context(stt, utterance, None).await
    }

    pub async fn transcribe_with_context(
        &self,
        stt: &dyn SpeechToText,
        utterance: &CompletedUtterance,
        context: Option<&crate::stt::provider::SttPromptContext>,
    ) -> Result<UserTranscriptReadyEvent, SessionError> {
        crate::modules::speech::transcription::transcribe_completed_utterance_with_context(
            stt, utterance, context,
        )
        .await
    }

    fn should_flush_on_stop(&self) -> bool {
        match self.config {
            SpeechIntakeConfig::PushToTalk(_) => self.ptt_held,
            SpeechIntakeConfig::VoiceActivity(vad) => {
                self.in_speech && self.speech_ms >= vad.min_speech_ms()
            }
        }
    }
}

/// Computes Root-Mean-Square (RMS) amplitude of a PCM `f32` frame.
pub fn compute_rms_f32(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_sq: f32 = samples.iter().map(|&s| s * s).sum();
    (sum_sq / (samples.len() as f32)).sqrt()
}
