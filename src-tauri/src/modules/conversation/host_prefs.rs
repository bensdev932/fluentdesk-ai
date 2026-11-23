//! Learner preferences synchronization helpers on `SpeakingSessionHost`.

use crate::database::repository::SessionRepository;
use crate::domain::{LearnerPreferences, SessionError, SessionId};
use crate::modules::conversation::{host::SpeakingSessionHost, manager::SessionManager};
use crate::modules::speech::intake::SpeechIntake;
use crate::preferences::LearnerPreferencesStore;

pub(crate) fn default_prefs_store(
    repository: Option<&SessionRepository>,
    user_id: &str,
) -> Option<LearnerPreferencesStore> {
    repository.map(|r| LearnerPreferencesStore::for_user(r.clone(), user_id))
}

impl SpeakingSessionHost {
    pub fn with_preferences_store(mut self, store: LearnerPreferencesStore) -> Self {
        self.prefs_store = Some(store);
        self
    }

    pub fn preferences_store(&self) -> Option<&LearnerPreferencesStore> {
        self.prefs_store.as_ref()
    }

    pub async fn apply_preferences_to_active(&self, prefs: &LearnerPreferences) {
        let mut guard = self.active.lock().await;
        if let Some(mgr) = guard.as_mut() {
            mgr.correction_mode = prefs.correction_mode;
            mgr.speaking_speed = prefs.speaking_speed;
            if mgr.scaffold.cefr_level != prefs.cefr_level {
                mgr.scaffold.cefr_level = prefs.cefr_level;
                mgr.speech_intake =
                    SpeechIntake::for_session_with_cefr(mgr.push_to_talk, prefs.cefr_level);
            }
        }
    }

    pub(crate) async fn build_manager(
        &self,
        session_id: SessionId,
    ) -> Result<SessionManager, SessionError> {
        let prefs = match self.preferences_store() {
            Some(store) => Some(store.get_preferences().await?),
            None => None,
        };
        let speed = if self.has_speed_override {
            self.speaking_speed
        } else {
            prefs
                .as_ref()
                .map(|p| p.speaking_speed)
                .unwrap_or(self.speaking_speed)
        };
        let mut mgr = match &self.repository {
            Some(repo) => {
                let mut m = SessionManager::with_persistence(
                    session_id,
                    self.max_context,
                    speed,
                    self.adapters.clone(),
                    repo.clone(),
                );
                if let Ok(facts) = repo.list_learner_facts(&self.user_id, 5).await {
                    m.scaffold.learner_facts = facts.into_iter().map(|f| f.fact_text).collect();
                }
                m
            }
            None => SessionManager::new(session_id, self.adapters.clone()),
        };
        if let Some(p) = prefs {
            mgr.scaffold.cefr_level = p.cefr_level;
        }
        Ok(mgr)
    }
}
