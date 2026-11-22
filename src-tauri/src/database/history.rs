//! Session summary and chronological history queries on `SessionRepository`.

use crate::database::mappers::{map_session_summary_row, map_turn_row, persistence_err};
use crate::database::repository::SessionRepository;
use crate::domain::{
    ConversationTurn, SessionError, SessionHistoryView, SessionId, SessionSummary,
};
use sqlx::Row;

impl SessionRepository {
    pub async fn list_turns_chronological(
        &self,
        session_id: &SessionId,
    ) -> Result<Vec<ConversationTurn>, SessionError> {
        let rows = sqlx::query(
            "SELECT t.id, t.conversation_id, t.role, t.text, t.correction_text, t.follow_up_prompt,
                    t.stt_duration_ms, t.llm_duration_ms, t.tts_duration_ms, t.timestamp_unix_ms,
                    c.correction_mode AS conversation_correction_mode
             FROM conversation_turns t
             JOIN conversations c ON c.id = t.conversation_id
             WHERE t.conversation_id = ?1
             ORDER BY t.sequence_number ASC, t.timestamp_unix_ms ASC",
        )
        .bind(session_id.as_str())
        .fetch_all(self.pool())
        .await
        .map_err(|e| persistence_err("Failed to load conversation history.", e))?;
        rows.iter().map(map_turn_row).collect()
    }

    pub async fn list_sessions_for_user(
        &self,
        user_id: &str,
    ) -> Result<Vec<SessionSummary>, SessionError> {
        let rows = sqlx::query(
            "SELECT c.id, c.started_at_unix_ms, c.ended_at_unix_ms, c.correction_mode, COUNT(t.id) AS turn_count
             FROM conversations c LEFT JOIN conversation_turns t ON t.conversation_id = c.id
             WHERE c.user_id = ?1 GROUP BY c.id ORDER BY c.started_at_unix_ms DESC, c.rowid DESC",
        )
        .bind(user_id)
        .fetch_all(self.pool())
        .await
        .map_err(|e| persistence_err("Failed to list speaking sessions.", e))?;
        rows.iter().map(map_session_summary_row).collect()
    }

    pub async fn find_session_history(
        &self,
        session_id: &SessionId,
    ) -> Result<Option<SessionHistoryView>, SessionError> {
        let row = sqlx::query(
            "SELECT started_at_unix_ms, ended_at_unix_ms FROM conversations WHERE id = ?1",
        )
        .bind(session_id.as_str())
        .fetch_optional(self.pool())
        .await
        .map_err(|e| persistence_err("Failed to load speaking session metadata.", e))?;
        let Some(row) = row else {
            return Ok(None);
        };
        let turns = self.list_turns_chronological(session_id).await?;
        Ok(Some(SessionHistoryView {
            session_id: session_id.clone(),
            started_at_unix_ms: row.get("started_at_unix_ms"),
            ended_at_unix_ms: row.get("ended_at_unix_ms"),
            turns,
        }))
    }

    pub async fn get_session_history(
        &self,
        sid: &SessionId,
    ) -> Result<SessionHistoryView, SessionError> {
        self.find_session_history(sid).await?.ok_or_else(|| {
            persistence_err("Failed to load speaking session metadata.", "Not found")
        })
    }
}
