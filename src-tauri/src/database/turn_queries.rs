//! Turn-level SQLite playback reconciliation and deletion helpers on `SessionRepository`.

use crate::database::mappers::{format_correction_mode, persistence_err};
use crate::database::repository::SessionRepository;
use crate::domain::{CorrectionMode, SessionError, SessionId};

impl SessionRepository {
    /// Creates a conversation row recording its active Roleplay Scenario ID.
    pub async fn create_conversation_with_scenario(
        &self,
        session_id: &SessionId,
        user_id: &str,
        scenario_mode: &str,
        correction_mode: CorrectionMode,
        started_at_unix_ms: i64,
    ) -> Result<(), SessionError> {
        sqlx::query(
            "INSERT INTO conversations (id, user_id, scenario_mode, correction_mode, state, started_at_unix_ms)
             VALUES (?1, ?2, ?3, ?4, 'Listening', ?5)",
        )
        .bind(session_id.as_str())
        .bind(user_id)
        .bind(scenario_mode)
        .bind(format_correction_mode(correction_mode))
        .bind(started_at_unix_ms)
        .execute(self.pool())
        .await
        .map_err(|e| persistence_err("Failed to create conversation session record.", e))?;
        Ok(())
    }

    /// Updates a tutor turn's spoken text, optional follow-up prompt, and measured playback duration.
    pub async fn update_tutor_turn_playback(
        &self,
        turn_id: &str,
        text: &str,
        follow_up_prompt: Option<&str>,
        tts_duration_ms: Option<u32>,
    ) -> Result<(), SessionError> {
        sqlx::query(
            "UPDATE conversation_turns
             SET text = ?2, follow_up_prompt = ?3, tts_duration_ms = COALESCE(?4, tts_duration_ms)
             WHERE id = ?1",
        )
        .bind(turn_id)
        .bind(text)
        .bind(follow_up_prompt)
        .bind(tts_duration_ms.map(i64::from))
        .execute(self.pool())
        .await
        .map_err(|e| persistence_err("Failed to reconcile tutor turn playback.", e))?;
        Ok(())
    }

    /// Updates only the measured TTS playback duration for an existing tutor turn.
    pub async fn update_turn_tts_duration(
        &self,
        turn_id: &str,
        tts_duration_ms: u32,
    ) -> Result<(), SessionError> {
        sqlx::query("UPDATE conversation_turns SET tts_duration_ms = ?2 WHERE id = ?1")
            .bind(turn_id)
            .bind(i64::from(tts_duration_ms))
            .execute(self.pool())
            .await
            .map_err(|e| persistence_err("Failed to update tutor turn TTS duration.", e))?;
        Ok(())
    }

    /// Deletes a turn row (used when barge-in interrupts before any tutor speech was heard).
    pub async fn delete_turn_by_id(&self, turn_id: &str) -> Result<(), SessionError> {
        sqlx::query("DELETE FROM conversation_turns WHERE id = ?1")
            .bind(turn_id)
            .execute(self.pool())
            .await
            .map_err(|e| persistence_err("Failed to remove unheard turn.", e))?;
        Ok(())
    }
}
