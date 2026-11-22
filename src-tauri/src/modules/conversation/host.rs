//! Authoritative single-owner Speaking Session host coordinating lifecycle,
//! active `SessionId` verification, SQLite history queries, and turn execution.

use crate::database::repository::SessionRepository;
use crate::domain::{
    SessionError, SessionErrorKind, SessionHistoryView, SessionId, SessionSummary,
};
use crate::modules::conversation::{
    context::MaxContextTurns,
    host_prefs::default_prefs_store,
    host_turns::cancel_mgr_turn,
    manager::{SessionManager, SessionPipelineAdapters},
    persistence::current_unix_ms,
    types::{SessionStatusSnapshot, StartSpeakingSessionConfig},
};
use crate::preferences::LearnerPreferencesStore;
use crate::tts::provider::SpeakingSpeed;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;

const DEFAULT_USER_ID: &str = "default-user";
const DEFAULT_MAX_CONTEXT_TURNS: usize = 20;

/// Single-owner host coordinating active speaking sessions and SQLite persistence.
pub struct SpeakingSessionHost {
    pub(crate) adapters: SessionPipelineAdapters,
    pub(crate) repository: Option<SessionRepository>,
    pub(crate) prefs_store: Option<LearnerPreferencesStore>,
    pub(crate) user_id: String,
    pub(crate) max_context: MaxContextTurns,
    pub(crate) speaking_speed: SpeakingSpeed,
    pub(crate) has_speed_override: bool,
    pub(crate) active: Arc<Mutex<Option<SessionManager>>>,
    session_seq: AtomicU64,
}

impl SpeakingSessionHost {
    pub fn new(adapters: SessionPipelineAdapters, repository: Option<SessionRepository>) -> Self {
        let prefs_store = default_prefs_store(repository.as_ref(), DEFAULT_USER_ID);
        Self {
            adapters,
            repository,
            prefs_store,
            user_id: DEFAULT_USER_ID.to_string(),
            max_context: MaxContextTurns::try_new(DEFAULT_MAX_CONTEXT_TURNS).expect("valid bound"),
            speaking_speed: SpeakingSpeed::try_new(1.0).expect("valid speed"),
            has_speed_override: false,
            active: Arc::new(Mutex::new(None)),
            session_seq: AtomicU64::new(1),
        }
    }

    pub fn with_max_context(mut self, max_context: MaxContextTurns) -> Self {
        self.max_context = max_context;
        self
    }

    pub fn with_speaking_speed(mut self, speed: SpeakingSpeed) -> Self {
        self.speaking_speed = speed;
        self.has_speed_override = true;
        self
    }

    pub async fn start_session(
        &self,
        config: StartSpeakingSessionConfig,
    ) -> Result<SessionStatusSnapshot, SessionError> {
        let now_ms = current_unix_ms();
        let seq = self.session_seq.fetch_add(1, Ordering::Relaxed);
        let session_id = SessionId::new(format!("sess-{now_ms}-{seq}"))?;
        self.start_session_with_id(session_id, config, now_ms).await
    }

    pub async fn start_session_with_id(
        &self,
        session_id: SessionId,
        config: StartSpeakingSessionConfig,
        now_unix_ms: i64,
    ) -> Result<SessionStatusSnapshot, SessionError> {
        self.stop_previous_if_active(now_unix_ms).await;
        let mut mgr = self.build_manager(session_id).await?;
        let snapshot = mgr
            .start_session(&config, &self.user_id, now_unix_ms)
            .await?;
        *self.active.lock().await = Some(mgr);
        Ok(snapshot)
    }

    pub async fn pause_session(
        &self,
        session_id: &SessionId,
    ) -> Result<SessionStatusSnapshot, SessionError> {
        let mut guard = self.active.lock().await;
        verify_active_mut(guard.as_mut(), session_id)?.pause().await
    }

    pub async fn resume_session(
        &self,
        session_id: &SessionId,
    ) -> Result<SessionStatusSnapshot, SessionError> {
        let mut guard = self.active.lock().await;
        verify_active_mut(guard.as_mut(), session_id)?
            .resume()
            .await
    }

    pub async fn stop_session(
        &self,
        session_id: &SessionId,
    ) -> Result<SessionStatusSnapshot, SessionError> {
        self.stop_session_at(session_id, current_unix_ms()).await
    }

    pub async fn stop_session_at(
        &self,
        session_id: &SessionId,
        ended_at_unix_ms: i64,
    ) -> Result<SessionStatusSnapshot, SessionError> {
        let mut guard = self.active.lock().await;
        verify_active_mut(guard.as_mut(), session_id)?
            .stop_at(ended_at_unix_ms)
            .await
    }

    pub async fn get_status(
        &self,
        session_id: &SessionId,
    ) -> Result<SessionStatusSnapshot, SessionError> {
        let guard = self.active.lock().await;
        Ok(verify_active_ref(guard.as_ref(), session_id)?.status_snapshot())
    }

    pub async fn is_cancelled(&self, session_id: &SessionId) -> Result<bool, SessionError> {
        let guard = self.active.lock().await;
        Ok(verify_active_ref(guard.as_ref(), session_id)?.is_cancelled())
    }

    pub async fn cancel_active_turn(&self, session_id: &SessionId) -> bool {
        let mut guard = self.active.lock().await;
        match guard.as_mut().filter(|m| m.session_id() == session_id) {
            Some(mgr) => cancel_mgr_turn(mgr).await,
            None => false,
        }
    }

    pub async fn list_sessions(&self) -> Result<Vec<SessionSummary>, SessionError> {
        match &self.repository {
            Some(repo) => repo.list_sessions_for_user(&self.user_id).await,
            None => Ok(Vec::new()),
        }
    }

    pub async fn find_session_history(
        &self,
        session_id: &SessionId,
    ) -> Result<Option<SessionHistoryView>, SessionError> {
        match &self.repository {
            Some(repo) => repo.find_session_history(session_id).await,
            None => Ok(None),
        }
    }

    async fn stop_previous_if_active(&self, now_unix_ms: i64) {
        if let Some(mut mgr) = self.active.lock().await.take() {
            if mgr.state().is_active() {
                let _ = mgr.stop_at(now_unix_ms).await;
            }
        }
    }
}

pub(crate) fn verify_active_mut<'a>(
    mgr: Option<&'a mut SessionManager>,
    expected: &SessionId,
) -> Result<&'a mut SessionManager, SessionError> {
    mgr.filter(|m| m.session_id() == expected)
        .ok_or_else(mismatched_session_err)
}

pub(crate) fn verify_active_ref<'a>(
    mgr: Option<&'a SessionManager>,
    expected: &SessionId,
) -> Result<&'a SessionManager, SessionError> {
    mgr.filter(|m| m.session_id() == expected)
        .ok_or_else(mismatched_session_err)
}

fn mismatched_session_err() -> SessionError {
    let msg = "Requested speaking session does not match the active session.";
    SessionError::new(SessionErrorKind::InvalidStateTransition, msg, true, None)
}
