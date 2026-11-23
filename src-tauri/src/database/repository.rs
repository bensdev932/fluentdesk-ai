//! Session, user, preferences, and turn repository (`T005`, `T052`, `T053`).

use crate::database::mappers::{
    format_correction_mode, format_model_overrides, format_role, map_preferences_row,
    persistence_err,
};
use crate::domain::{
    ConversationTurn, CorrectionMode, LearnerPreferences, SessionError, SessionId, SpeakingSpeed,
};
use sqlx::SqlitePool;

/// SQLite repository for users, learner preferences, conversations, and turns.
#[derive(Clone)]
pub struct SessionRepository {
    pool: SqlitePool,
}

impl SessionRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub async fn ensure_default_user(
        &self,
        user_id: &str,
        display_name: &str,
        now_unix_ms: i64,
    ) -> Result<(), SessionError> {
        self.ensure_user_exists(user_id, display_name, now_unix_ms)
            .await
    }

    pub async fn ensure_user_exists(
        &self,
        uid: &str,
        name: &str,
        now_ms: i64,
    ) -> Result<(), SessionError> {
        sqlx::query(
            "INSERT OR IGNORE INTO users (id, display_name, proficiency_level, target_goal, created_at_unix_ms, updated_at_unix_ms)
             VALUES (?1, ?2, 'B1', 'general_conversation', ?3, ?3)",
        )
        .bind(uid).bind(name).bind(now_ms).execute(&self.pool).await
        .map_err(|e| persistence_err("Failed to initialize learner profile.", e))?;
        Ok(())
    }

    pub async fn get_or_create_preferences(
        &self,
        user_id: &str,
        now_unix_ms: i64,
    ) -> Result<LearnerPreferences, SessionError> {
        if let Some(row) = self.fetch_preferences_row(user_id).await? {
            return map_preferences_row(&row);
        }
        self.seed_default_preferences(user_id, now_unix_ms).await?;
        let row = self.fetch_preferences_row(user_id).await?;
        let row = row
            .ok_or_else(|| persistence_err("Failed to load learner preferences.", "Missing row"))?;
        map_preferences_row(&row)
    }

    async fn fetch_preferences_row(
        &self,
        user_id: &str,
    ) -> Result<Option<sqlx::sqlite::SqliteRow>, SessionError> {
        sqlx::query(
            "SELECT correction_mode, speaking_speed, push_to_talk_enabled, cloud_opt_in,
                    raw_audio_recording_opt_in, preferred_provider, model_overrides_json,
                    cefr_level, english_voice_id, pre_conversation_script_enabled
             FROM session_settings WHERE user_id = ?1",
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| persistence_err("Failed to load learner preferences.", e))
    }

    async fn seed_default_preferences(
        &self,
        user_id: &str,
        now_ms: i64,
    ) -> Result<(), SessionError> {
        self.ensure_user_exists(user_id, "Learner", now_ms).await?;
        sqlx::query(
            "INSERT INTO session_settings (
                user_id, correction_mode, speaking_speed, push_to_talk_enabled, cloud_opt_in,
                raw_audio_recording_opt_in, preferred_provider, model_overrides_json, cefr_level,
                english_voice_id, pre_conversation_script_enabled, updated_at_unix_ms
             ) VALUES (?1, 'Gentle', 1.0, 0, 0, 0, 'auto', '{}', 'B1', 'en-US', 0, ?2) ON CONFLICT(user_id) DO NOTHING",
        )
        .bind(user_id).bind(now_ms).execute(&self.pool).await
        .map_err(|e| persistence_err("Failed to initialize learner preferences.", e))?;
        Ok(())
    }

    pub async fn save_preferences(
        &self,
        user_id: &str,
        prefs: &LearnerPreferences,
        now_unix_ms: i64,
    ) -> Result<(), SessionError> {
        let speed = SpeakingSpeed::try_new(prefs.speaking_speed.as_f32())?;
        let voice_id = crate::domain::validate_english_voice_id(&prefs.english_voice_id)?;
        let mut clean = prefs.clone();
        clean.enforce_cloud_consent_invariant();
        let overrides_json = format_model_overrides(&clean.model_overrides);
        sqlx::query(
            "INSERT INTO session_settings (
                user_id, correction_mode, speaking_speed, push_to_talk_enabled, cloud_opt_in,
                raw_audio_recording_opt_in, preferred_provider, model_overrides_json, cefr_level,
                english_voice_id, pre_conversation_script_enabled, updated_at_unix_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
             ON CONFLICT(user_id) DO UPDATE SET
               correction_mode = excluded.correction_mode, speaking_speed = excluded.speaking_speed,
               push_to_talk_enabled = excluded.push_to_talk_enabled, cloud_opt_in = excluded.cloud_opt_in,
               raw_audio_recording_opt_in = excluded.raw_audio_recording_opt_in, preferred_provider = excluded.preferred_provider,
               model_overrides_json = excluded.model_overrides_json, cefr_level = excluded.cefr_level,
               english_voice_id = excluded.english_voice_id,
               pre_conversation_script_enabled = excluded.pre_conversation_script_enabled,
               updated_at_unix_ms = excluded.updated_at_unix_ms",
        )
        .bind(user_id).bind(format_correction_mode(clean.correction_mode)).bind(f64::from(speed.as_f32()))
        .bind(i64::from(clean.push_to_talk_enabled)).bind(i64::from(clean.cloud_opt_in))
        .bind(i64::from(clean.raw_audio_recording_opt_in))
        .bind(clean.preferred_provider.as_str()).bind(overrides_json).bind(clean.cefr_level.as_str())
        .bind(voice_id).bind(i64::from(clean.pre_conversation_script_enabled)).bind(now_unix_ms)
        .execute(&self.pool).await.map_err(|e| persistence_err("Failed to save learner preferences.", e))?;
        Ok(())
    }

    pub async fn create_conversation(
        &self,
        session_id: &SessionId,
        user_id: &str,
        correction_mode: CorrectionMode,
        started_at_unix_ms: i64,
    ) -> Result<(), SessionError> {
        sqlx::query("INSERT INTO conversations (id, user_id, scenario_mode, correction_mode, state, started_at_unix_ms) VALUES (?1, ?2, 'free_conversation', ?3, 'Listening', ?4)")
            .bind(session_id.as_str()).bind(user_id).bind(format_correction_mode(correction_mode)).bind(started_at_unix_ms)
            .execute(&self.pool).await.map_err(|e| persistence_err("Failed to create conversation session record.", e))?;
        Ok(())
    }

    pub async fn mark_conversation_ended(
        &self,
        session_id: &SessionId,
        ended_at_unix_ms: i64,
    ) -> Result<(), SessionError> {
        sqlx::query(
            "UPDATE conversations SET state = 'Ended', ended_at_unix_ms = ?2 WHERE id = ?1",
        )
        .bind(session_id.as_str())
        .bind(ended_at_unix_ms)
        .execute(&self.pool)
        .await
        .map_err(|e| persistence_err("Failed to mark conversation ended.", e))?;
        Ok(())
    }

    pub async fn append_turn(&self, turn: &ConversationTurn, seq: u32) -> Result<(), SessionError> {
        let meta = turn.stage_metadata.as_ref();
        sqlx::query(
            "INSERT INTO conversation_turns (
                id, conversation_id, sequence_number, role, text, correction_text,
                proper_way, feedback_why, follow_up_prompt, stt_duration_ms, llm_duration_ms,
                tts_duration_ms, timestamp_unix_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        )
        .bind(&turn.id)
        .bind(turn.session_id.as_str())
        .bind(i64::from(seq))
        .bind(format_role(turn.role))
        .bind(&turn.text)
        .bind(&turn.correction)
        .bind(&turn.proper_way)
        .bind(&turn.why)
        .bind(&turn.follow_up_prompt)
        .bind(meta.and_then(|m| m.stt_duration_ms).map(i64::from))
        .bind(meta.and_then(|m| m.llm_duration_ms).map(i64::from))
        .bind(meta.and_then(|m| m.tts_duration_ms).map(i64::from))
        .bind(turn.timestamp_unix_ms)
        .execute(&self.pool)
        .await
        .map_err(|e| persistence_err("Failed to save conversation turn.", e))?;
        Ok(())
    }
}
