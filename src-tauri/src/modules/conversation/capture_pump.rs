//! Live capture pump, device selection, and push-to-talk helpers on `SessionManager`.

use crate::domain::{AudioChunk, SessionError, SessionState};
use crate::modules::conversation::{
    host_turns::{cancelled_turn_err, require_state},
    manager::SessionManager,
    persistence::invalid_transition_err,
    turn::EndToEndTurnResult,
    types::StartSpeakingSessionConfig,
};
use crate::modules::speech::{
    audio_capture::{validate_audio_input_device, AudioDeviceId},
    intake::{CompletedUtterance, TurnDetectionEvent},
};
use tokio::sync::mpsc;

impl SessionManager {
    pub(crate) fn reset_intake_for_start(&mut self, cfg: &StartSpeakingSessionConfig) {
        self.cancellation.reset();
        self.clear_capture_state(false);
        self.correction_mode = cfg.correction_mode;
        self.push_to_talk = cfg.push_to_talk;
    }

    pub(crate) fn clear_capture_state(&mut self, stop_stream: bool) {
        if stop_stream {
            self.capture_cancel.cancel();
            self.capture_rx = None;
        } else {
            self.capture_cancel.reset();
        }
        self.capture_pump_active = false;
        self.pending_ptt_release = false;
        self.speech_intake.reset();
    }

    pub fn process_captured_audio_chunk(
        &mut self,
        chunk: &AudioChunk,
    ) -> Result<TurnDetectionEvent, SessionError> {
        self.speech_intake.process_chunk(chunk)
    }

    pub fn set_ptt_held(&mut self, held: bool) -> Result<Option<CompletedUtterance>, SessionError> {
        require_state(self, SessionState::Listening, SessionState::Processing)?;
        if self.capture_pump_active {
            return self.queue_ptt_for_active_pump(held);
        }
        self.speech_intake.set_ptt_held(held)
    }

    fn queue_ptt_for_active_pump(
        &mut self,
        held: bool,
    ) -> Result<Option<CompletedUtterance>, SessionError> {
        if held || !self.speech_intake.is_push_to_talk() {
            self.pending_ptt_release = false;
            return self.speech_intake.set_ptt_held(held);
        }
        if self.speech_intake.is_ptt_held() {
            self.pending_ptt_release = true;
            self.ptt_notify.notify_one();
        }
        Ok(None)
    }

    pub(crate) async fn start_microphone_stream_for(
        &mut self,
        requested_id: Option<&str>,
    ) -> Result<(), SessionError> {
        if let Some(raw) = requested_id {
            let device_id = validate_audio_input_device(self.adapters.audio_input.as_ref(), raw)
                .await
                .inspect_err(|_| {
                    let _ = self.transition_to(SessionState::Error);
                })?;
            self.active_input_device = Some(device_id);
        }
        self.start_microphone_stream().await
    }

    pub(crate) async fn set_active_input_device(
        &mut self,
        device_id: AudioDeviceId,
    ) -> Result<(), SessionError> {
        if self.capture_pump_active {
            return Err(invalid_transition_err(self.state, self.state));
        }
        self.active_input_device = Some(device_id);
        if self.state == SessionState::Listening {
            self.adapters.audio_input.stop_capture().await?;
            self.start_microphone_stream().await?;
        }
        Ok(())
    }

    pub async fn pump_capture_turn(
        &mut self,
        now_unix_ms: i64,
    ) -> Result<Option<EndToEndTurnResult>, SessionError> {
        require_state(self, SessionState::Listening, SessionState::Processing)?;
        if self.is_cancelled() {
            return Err(cancelled_turn_err());
        }
        let Some(mut rx) = self.capture_rx.take() else {
            return Ok(None);
        };
        let completed = self.drain_capture_until_utterance(&mut rx).await?;
        match completed {
            Some(utt) => {
                self.capture_rx = Some(rx);
                self.process_spoken_utterance(&utt, now_unix_ms)
                    .await
                    .map(Some)
            }
            None => Ok(None),
        }
    }

    async fn drain_capture_until_utterance(
        &mut self,
        rx: &mut mpsc::Receiver<AudioChunk>,
    ) -> Result<Option<CompletedUtterance>, SessionError> {
        let mut cancel = self.cancellation.clone();
        let mut capture_cancel = self.capture_cancel.clone();
        loop {
            tokio::select! {
                biased;
                _ = cancel.wait_for_cancellation() => {
                    self.speech_intake.reset();
                    return Err(cancelled_turn_err());
                }
                _ = capture_cancel.wait_for_cancellation() => {
                    self.speech_intake.reset();
                    return Ok(None);
                }
                maybe_chunk = rx.recv() => match maybe_chunk {
                    Some(chunk) => if let Some(utt) = self.handle_pumped_chunk(&chunk)? {
                        return Ok(Some(utt));
                    },
                    None => return Ok(None),
                }
            }
        }
    }

    pub(crate) fn handle_pumped_chunk(
        &mut self,
        chunk: &AudioChunk,
    ) -> Result<Option<CompletedUtterance>, SessionError> {
        let ev = match self.speech_intake.process_chunk(chunk) {
            Ok(ev) => ev,
            Err(err) => {
                let _ = self.transition_to(SessionState::Error);
                return Err(err);
            }
        };
        Ok(match ev {
            TurnDetectionEvent::TurnCompleted(utt) => Some(utt),
            TurnDetectionEvent::Silence | TurnDetectionEvent::SpeechContinuing => None,
        })
    }
}
