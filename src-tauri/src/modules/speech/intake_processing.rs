//! Internal VAD pre-roll, silence trimming, and push-to-talk chunk helpers for `SpeechIntake`.

use crate::domain::{AudioChunk, DomainValidationError, SessionError, SessionErrorKind};
use crate::modules::speech::conversion::normalize_chunk_to_16khz_mono;
use crate::modules::speech::intake::{
    compute_rms_f32, SpeechIntake, TurnDetectionEvent, VadEndpointConfig,
};
use crate::modules::speech::utterance::CompletedUtterance;

impl SpeechIntake {
    pub(super) fn begin_ptt_press(&mut self) {
        if !self.ptt_held {
            self.buffer.clear();
            self.pre_roll.clear();
            self.ptt_held = true;
        }
    }

    pub(super) fn release_ptt(&mut self) -> Option<CompletedUtterance> {
        self.ptt_held = false;
        let out = self.buffer.take_utterance();
        self.reset();
        out
    }

    pub(super) fn process_ptt_chunk(
        &mut self,
        chunk: &AudioChunk,
    ) -> Result<TurnDetectionEvent, SessionError> {
        if !self.ptt_held {
            return Ok(TurnDetectionEvent::Silence);
        }
        self.push_to_bounded_buffer(chunk)
    }

    pub(super) fn process_vad_chunk(
        &mut self,
        vad: VadEndpointConfig,
        chunk: &AudioChunk,
    ) -> Result<TurnDetectionEvent, SessionError> {
        if compute_rms_f32(&chunk.samples_f32) >= vad.rms_threshold() {
            return self.handle_voiced_chunk(vad, chunk);
        }
        self.handle_silent_chunk(vad, chunk)
    }

    fn handle_voiced_chunk(
        &mut self,
        vad: VadEndpointConfig,
        chunk: &AudioChunk,
    ) -> Result<TurnDetectionEvent, SessionError> {
        if !self.in_speech && vad.pre_roll_ms() > 0 && !self.pre_roll.is_empty() {
            let prefix: Vec<f32> = self.pre_roll.drain(..).collect();
            let prefix_ms = u32::try_from(prefix.len() / 16).unwrap_or(0);
            let _ = self.buffer.push_normalized_samples(&prefix, prefix_ms);
        }
        self.in_speech = true;
        self.trailing_silence_ms = 0;
        self.speech_ms = self.speech_ms.saturating_add(chunk.duration_ms);
        self.push_to_bounded_buffer(chunk)
    }

    fn handle_silent_chunk(
        &mut self,
        vad: VadEndpointConfig,
        chunk: &AudioChunk,
    ) -> Result<TurnDetectionEvent, SessionError> {
        if !self.in_speech {
            self.record_pre_roll(vad, chunk)?;
            return Ok(TurnDetectionEvent::Silence);
        }
        self.trailing_silence_ms = self.trailing_silence_ms.saturating_add(chunk.duration_ms);
        if self.is_short_noise_burst(vad) {
            self.reset();
            return Ok(TurnDetectionEvent::Silence);
        }
        self.append_vad_silence_chunk(vad, chunk)
    }

    fn record_pre_roll(
        &mut self,
        vad: VadEndpointConfig,
        chunk: &AudioChunk,
    ) -> Result<(), SessionError> {
        let cap = (vad.pre_roll_ms() as usize) * 16;
        if cap == 0 {
            return Ok(());
        }
        let samples = normalize_chunk_to_16khz_mono(chunk)?;
        for s in samples {
            if self.pre_roll.len() >= cap {
                self.pre_roll.pop_front();
            }
            self.pre_roll.push_back(s);
        }
        Ok(())
    }

    fn is_short_noise_burst(&self, vad: VadEndpointConfig) -> bool {
        self.trailing_silence_ms >= vad.silence_hangover_ms()
            && self.speech_ms < vad.min_speech_ms()
    }

    fn append_vad_silence_chunk(
        &mut self,
        vad: VadEndpointConfig,
        chunk: &AudioChunk,
    ) -> Result<TurnDetectionEvent, SessionError> {
        let ev = self.push_to_bounded_buffer(chunk)?;
        if matches!(ev, TurnDetectionEvent::SpeechContinuing)
            && self.trailing_silence_ms >= vad.silence_hangover_ms()
        {
            if let Some(keep_ms) = vad.max_trailing_silence_ms() {
                self.buffer
                    .trim_trailing_silence(self.trailing_silence_ms, keep_ms);
            }
            return Ok(self.finish_turn());
        }
        Ok(ev)
    }

    fn push_to_bounded_buffer(
        &mut self,
        chunk: &AudioChunk,
    ) -> Result<TurnDetectionEvent, SessionError> {
        let reached_cap = self.buffer.push_chunk(chunk)?;
        if reached_cap || self.buffer.duration_ms() >= self.config.max_duration().as_u32() {
            return Ok(self.finish_turn());
        }
        Ok(TurnDetectionEvent::SpeechContinuing)
    }

    fn finish_turn(&mut self) -> TurnDetectionEvent {
        let utt = self.buffer.take_utterance();
        self.reset();
        utt.map_or(
            TurnDetectionEvent::Silence,
            TurnDetectionEvent::TurnCompleted,
        )
    }
}

pub(super) fn format_err(err: DomainValidationError) -> SessionError {
    SessionError::new(
        SessionErrorKind::AudioDeviceUnavailable,
        "Unsupported audio stream format.",
        true,
        Some(err.to_string()),
    )
}
