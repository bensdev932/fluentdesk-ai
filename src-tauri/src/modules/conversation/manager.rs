//! Deep single-owner speaking session module (`T009`–`T011`, `T034`–`T035`, `T048`–`T051`).

use crate::ai::provider::AiProvider;
use crate::database::repository::SessionRepository;
use crate::domain::{
    AudioChunk, CorrectionMode, PedagogicalScaffold, SessionError, SessionId, SessionReview,
    SessionState,
};
use crate::modules::conversation::{
    cancellation::SessionCancellationToken,
    context::{BoundedConversationContext, MaxContextTurns},
    persistence::{current_unix_ms, invalid_transition_err},
    types::{SessionStatusSnapshot, StartSpeakingSessionConfig},
};
use crate::modules::speech::{
    audio_capture::{AudioDeviceId, AudioInput},
    intake::SpeechIntake,
    playback::AudioOutput,
};
use crate::stt::provider::SpeechToText;
use crate::tts::provider::{SpeakingSpeed, TextToSpeech};
use std::sync::Arc;
use tokio::sync::{mpsc, Notify};

/// Pipeline dependencies encapsulated by `SessionManager`.
#[derive(Clone)]
pub struct SessionPipelineAdapters {
    pub audio_input: Arc<dyn AudioInput>,
    pub stt: Arc<dyn SpeechToText>,
    pub ai: Arc<dyn AiProvider>,
    pub tts: Arc<dyn TextToSpeech>,
    pub audio_output: Arc<dyn AudioOutput>,
}

/// Deep module owning speaking session state, capture stream, speech intake, and persistence.
pub struct SessionManager {
    pub(crate) session_id: SessionId,
    pub(crate) user_id: String,
    pub(crate) state: SessionState,
    pub(crate) active_input_device: Option<AudioDeviceId>,
    pub(crate) correction_mode: CorrectionMode,
    pub(crate) push_to_talk: bool,
    pub(crate) speaking_speed: SpeakingSpeed,
    pub(crate) scaffold: PedagogicalScaffold,
    pub(crate) latest_review: Option<SessionReview>,
    pub(crate) context: BoundedConversationContext,
    pub(crate) repository: Option<SessionRepository>,
    pub(crate) conversation_persisted: bool,
    pub(crate) context_loaded: bool,
    pub(crate) next_sequence_number: u32,
    pub(crate) capture_rx: Option<mpsc::Receiver<AudioChunk>>,
    pub(crate) speech_intake: SpeechIntake,
    pub(crate) cancellation: SessionCancellationToken,
    pub(crate) capture_cancel: SessionCancellationToken,
    pub(crate) capture_pump_active: bool,
    pub(crate) pending_ptt_release: bool,
    pub(crate) ptt_notify: Arc<Notify>,
    pub(crate) adapters: SessionPipelineAdapters,
}

impl SessionManager {
    pub fn new(session_id: SessionId, adapters: SessionPipelineAdapters) -> Self {
        Self {
            session_id,
            user_id: "default-user".to_string(),
            state: SessionState::Idle,
            active_input_device: None,
            correction_mode: CorrectionMode::Natural,
            push_to_talk: false,
            speaking_speed: SpeakingSpeed::try_new(1.0).expect("default speaking speed"),
            scaffold: PedagogicalScaffold::default(),
            latest_review: None,
            context: BoundedConversationContext::new(MaxContextTurns::try_new(6).expect("bound")),
            repository: None,
            conversation_persisted: false,
            context_loaded: false,
            next_sequence_number: 0,
            capture_rx: None,
            speech_intake: SpeechIntake::for_session(false),
            cancellation: SessionCancellationToken::new(),
            capture_cancel: SessionCancellationToken::new(),
            capture_pump_active: false,
            pending_ptt_release: false,
            ptt_notify: Arc::new(Notify::new()),
            adapters,
        }
    }

    pub fn with_persistence(
        session_id: SessionId,
        max_context_turns: MaxContextTurns,
        speaking_speed: SpeakingSpeed,
        adapters: SessionPipelineAdapters,
        repository: SessionRepository,
    ) -> Self {
        let mut mgr = Self::new(session_id, adapters);
        mgr.context = BoundedConversationContext::new(max_context_turns);
        mgr.speaking_speed = speaking_speed;
        mgr.repository = Some(repository);
        mgr
    }

    pub fn session_id(&self) -> &SessionId {
        &self.session_id
    }
    pub fn state(&self) -> SessionState {
        self.state
    }
    pub fn is_cancelled(&self) -> bool {
        self.cancellation.is_cancelled()
    }
    pub fn is_capture_stream_open(&self) -> bool {
        self.capture_rx.is_some()
    }
    pub fn is_push_to_talk(&self) -> bool {
        self.push_to_talk
    }
    pub fn selected_input_device(&self) -> Option<&AudioDeviceId> {
        self.active_input_device.as_ref()
    }
    pub fn cancellation_token(&self) -> SessionCancellationToken {
        self.cancellation.clone()
    }
    pub fn status_snapshot(&self) -> SessionStatusSnapshot {
        SessionStatusSnapshot::from_manager(
            self.session_id.clone(),
            self.state,
            self.correction_mode,
            self.push_to_talk,
            &self.scaffold,
            self.latest_review.clone(),
        )
    }

    pub fn transition_to(&mut self, target: SessionState) -> Result<SessionState, SessionError> {
        if !self.state.can_transition_to(target) {
            return Err(invalid_transition_err(self.state, target));
        }
        self.state = target;
        Ok(self.state)
    }

    pub async fn start(
        &mut self,
        cfg: &StartSpeakingSessionConfig,
    ) -> Result<SessionStatusSnapshot, SessionError> {
        self.start_session(cfg, "default-user", current_unix_ms())
            .await
    }

    pub async fn start_session(
        &mut self,
        cfg: &StartSpeakingSessionConfig,
        user_id: &str,
        now_unix_ms: i64,
    ) -> Result<SessionStatusSnapshot, SessionError> {
        self.user_id = user_id.to_string();
        self.latest_review = None;
        self.reset_intake_for_start(cfg);
        self.configure_scaffold(cfg)?;
        self.transition_to(SessionState::Initializing)?;
        self.verify_required_providers().await?;
        self.start_microphone_stream_for(cfg.input_device_id.as_deref())
            .await?;
        self.ensure_session_persisted(user_id, now_unix_ms).await?;
        self.transition_to(SessionState::Listening)?;
        Ok(self.status_snapshot())
    }

    pub async fn pause(&mut self) -> Result<SessionStatusSnapshot, SessionError> {
        self.transition_to(SessionState::Paused)?;
        self.clear_capture_state(true);
        self.adapters.audio_input.stop_capture().await?;
        self.adapters.audio_output.stop_playback().await?;
        Ok(self.status_snapshot())
    }

    pub async fn resume(&mut self) -> Result<SessionStatusSnapshot, SessionError> {
        self.transition_to(SessionState::Listening)?;
        self.clear_capture_state(false);
        self.start_microphone_stream().await?;
        Ok(self.status_snapshot())
    }

    pub async fn stop(&mut self) -> Result<SessionStatusSnapshot, SessionError> {
        self.stop_at(current_unix_ms()).await
    }

    pub async fn stop_at(
        &mut self,
        ended_at_unix_ms: i64,
    ) -> Result<SessionStatusSnapshot, SessionError> {
        self.cancellation.cancel();
        self.clear_capture_state(true);
        let _ = self.adapters.audio_input.stop_capture().await;
        let _ = self.adapters.audio_output.stop_playback().await;
        if self.state != SessionState::Ended {
            self.transition_to(SessionState::Ended)?;
            self.persist_session_ended(ended_at_unix_ms).await?;
        }
        Ok(self.status_snapshot())
    }
}
