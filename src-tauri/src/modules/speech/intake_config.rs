//! Configuration and event types for unified `SpeechIntake` (`T018`–`T020`, `T026`).

use crate::domain::{CefrLevel, SessionError, SessionErrorKind};
use crate::modules::speech::utterance::{
    CompletedUtterance, MaxUtteranceDurationMs, DEFAULT_MAX_UTTERANCE_MS,
};

const DEFAULT_VAD_RMS_THRESHOLD: f32 = 0.03;
const DEFAULT_VAD_HANGOVER_MS: u32 = 800;
const DEFAULT_VAD_MIN_SPEECH_MS: u32 = 200;
const DEFAULT_PRE_ROLL_MS: u32 = 200;
const DEFAULT_TRAILING_KEEP_MS: u32 = 200;

/// Returns CEFR-scaled VAD silence hangover in milliseconds for non-native hesitation pacing.
pub fn hangover_ms_for_cefr(level: CefrLevel) -> u32 {
    match level {
        CefrLevel::A1 | CefrLevel::A2 => 1_400,
        CefrLevel::B1 | CefrLevel::B2 => 1_050,
        CefrLevel::C1 | CefrLevel::C2 => 800,
    }
}

/// Outcome of feeding one captured `AudioChunk` into `SpeechIntake`.
#[derive(Debug, Clone, PartialEq)]
pub enum TurnDetectionEvent {
    Silence,
    SpeechContinuing,
    TurnCompleted(CompletedUtterance),
}

/// Validated configuration for energy-based VAD and learner-friendly endpointing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VadEndpointConfig {
    rms_threshold: f32,
    silence_hangover_ms: u32,
    min_speech_ms: u32,
    max_utterance: MaxUtteranceDurationMs,
    pre_roll_ms: u32,
    max_trailing_silence_ms: Option<u32>,
}

impl VadEndpointConfig {
    pub fn try_new(
        rms_threshold: f32,
        silence_hangover_ms: u32,
        min_speech_ms: u32,
        max_utterance_ms: u32,
    ) -> Result<Self, SessionError> {
        let max_utterance = MaxUtteranceDurationMs::try_new(max_utterance_ms)?;
        if !is_valid_vad_params(
            rms_threshold,
            silence_hangover_ms,
            min_speech_ms,
            max_utterance_ms,
        ) {
            return Err(invalid_cfg_err(
                "Invalid voice activity detector configuration.",
            ));
        }
        Ok(Self {
            rms_threshold,
            silence_hangover_ms,
            min_speech_ms,
            max_utterance,
            pre_roll_ms: 0,
            max_trailing_silence_ms: None,
        })
    }

    pub fn for_cefr(level: CefrLevel) -> Self {
        let mut cfg = Self::try_new(
            DEFAULT_VAD_RMS_THRESHOLD,
            hangover_ms_for_cefr(level),
            DEFAULT_VAD_MIN_SPEECH_MS,
            DEFAULT_MAX_UTTERANCE_MS,
        )
        .expect("valid CEFR VAD configuration");
        cfg.pre_roll_ms = DEFAULT_PRE_ROLL_MS;
        cfg.max_trailing_silence_ms = Some(DEFAULT_TRAILING_KEEP_MS);
        cfg
    }

    pub fn rms_threshold(&self) -> f32 {
        self.rms_threshold
    }
    pub fn silence_hangover_ms(&self) -> u32 {
        self.silence_hangover_ms
    }
    pub fn min_speech_ms(&self) -> u32 {
        self.min_speech_ms
    }
    pub fn max_utterance(&self) -> MaxUtteranceDurationMs {
        self.max_utterance
    }
    pub fn pre_roll_ms(&self) -> u32 {
        self.pre_roll_ms
    }
    pub fn max_trailing_silence_ms(&self) -> Option<u32> {
        self.max_trailing_silence_ms
    }
}

impl Default for VadEndpointConfig {
    fn default() -> Self {
        Self::try_new(
            DEFAULT_VAD_RMS_THRESHOLD,
            DEFAULT_VAD_HANGOVER_MS,
            DEFAULT_VAD_MIN_SPEECH_MS,
            DEFAULT_MAX_UTTERANCE_MS,
        )
        .expect("valid default VAD configuration")
    }
}

/// Active endpointing policy for `SpeechIntake`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SpeechIntakeConfig {
    VoiceActivity(VadEndpointConfig),
    PushToTalk(MaxUtteranceDurationMs),
}

impl SpeechIntakeConfig {
    pub fn for_session(push_to_talk: bool) -> Self {
        if push_to_talk {
            Self::PushToTalk(MaxUtteranceDurationMs::default_cap())
        } else {
            Self::VoiceActivity(VadEndpointConfig::default())
        }
    }

    pub fn for_session_with_cefr(push_to_talk: bool, level: CefrLevel) -> Self {
        if push_to_talk {
            Self::PushToTalk(MaxUtteranceDurationMs::default_cap())
        } else {
            Self::VoiceActivity(VadEndpointConfig::for_cefr(level))
        }
    }

    pub fn max_duration(&self) -> MaxUtteranceDurationMs {
        match self {
            Self::VoiceActivity(cfg) => cfg.max_utterance(),
            Self::PushToTalk(max) => *max,
        }
    }

    pub fn is_push_to_talk(&self) -> bool {
        matches!(self, Self::PushToTalk(_))
    }
}

fn is_valid_vad_params(
    rms_threshold: f32,
    silence_hangover_ms: u32,
    min_speech_ms: u32,
    max_utterance_ms: u32,
) -> bool {
    (0.001..=0.5).contains(&rms_threshold)
        && silence_hangover_ms >= 100
        && min_speech_ms >= 50
        && max_utterance_ms > silence_hangover_ms
}

pub(super) fn invalid_cfg_err(msg: &'static str) -> SessionError {
    SessionError::new(SessionErrorKind::InvalidStateTransition, msg, true, None)
}
