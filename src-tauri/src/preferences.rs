//! Unified SQLite-backed Learner Preferences module (`get_preferences`, `patch_preferences`).

use crate::database::repository::SessionRepository;
use crate::domain::{LearnerPreferences, LearnerPreferencesPatch, SessionError};
use std::sync::{Arc, RwLock};

pub const DEFAULT_PREFERENCES_USER_ID: &str = "default-user";

/// Single backend seam for reading and patching non-secret learner & routing preferences.
#[derive(Clone)]
pub struct LearnerPreferencesStore {
    repository: SessionRepository,
    user_id: String,
    cache: Arc<RwLock<Option<LearnerPreferences>>>,
}

impl LearnerPreferencesStore {
    pub fn new(repository: SessionRepository) -> Self {
        Self::for_user(repository, DEFAULT_PREFERENCES_USER_ID)
    }

    pub fn for_user(repository: SessionRepository, user_id: impl Into<String>) -> Self {
        let cleaned = user_id.into().trim().to_string();
        let resolved = if cleaned.is_empty() {
            DEFAULT_PREFERENCES_USER_ID.to_string()
        } else {
            cleaned
        };
        Self {
            repository,
            user_id: resolved,
            cache: Arc::new(RwLock::new(None)),
        }
    }

    pub fn repository(&self) -> &SessionRepository {
        &self.repository
    }

    pub fn user_id(&self) -> &str {
        &self.user_id
    }

    pub async fn get_preferences(&self) -> Result<LearnerPreferences, SessionError> {
        if let Some(p) = self.cache.read().expect("lock").as_ref() {
            return Ok(p.clone());
        }
        let prefs = self
            .repository
            .get_or_create_preferences(&self.user_id, current_unix_ms())
            .await?;
        *self.cache.write().expect("lock") = Some(prefs.clone());
        Ok(prefs)
    }

    pub async fn patch_preferences(
        &self,
        patch: LearnerPreferencesPatch,
    ) -> Result<LearnerPreferences, SessionError> {
        let mut clean = self.get_preferences().await?;
        clean.apply_patch(patch)?;
        clean.enforce_cloud_consent_invariant();
        self.repository
            .save_preferences(&self.user_id, &clean, current_unix_ms())
            .await?;
        *self.cache.write().expect("lock") = Some(clean.clone());
        Ok(clean)
    }
}

fn current_unix_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::connection::init_sqlite_pool;
    use crate::domain::{CorrectionMode, ProviderPreference};

    #[tokio::test]
    async fn preferences_store_caches_reads_and_updates_cache_with_invariant_on_patch() {
        let pool = init_sqlite_pool("sqlite::memory:").await.unwrap();
        let store = LearnerPreferencesStore::new(SessionRepository::new(pool.clone()));
        assert_eq!(
            store.get_preferences().await.unwrap().correction_mode,
            CorrectionMode::Gentle
        );
        sqlx::query("UPDATE session_settings SET correction_mode = 'Off' WHERE user_id = ?1")
            .bind(DEFAULT_PREFERENCES_USER_ID)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(
            store.get_preferences().await.unwrap().correction_mode,
            CorrectionMode::Gentle
        );
        let patched = store
            .patch_preferences(LearnerPreferencesPatch {
                cloud_opt_in: Some(false),
                preferred_provider: Some(ProviderPreference::Groq),
                correction_mode: Some(CorrectionMode::Detailed),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(patched.preferred_provider, ProviderPreference::Auto);
        assert_eq!(
            store.get_preferences().await.unwrap().correction_mode,
            CorrectionMode::Detailed
        );
    }
}
