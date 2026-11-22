//! SQLite repository persistence for `SessionReview` and text-only `LearnerFact` items.

use crate::database::mappers::persistence_err;
use crate::database::repository::SessionRepository;
use crate::domain::{CefrLevel, LearnerFact, SessionError, SessionId, SessionReview};
use sqlx::{sqlite::SqliteRow, Row};
use std::str::FromStr;

impl SessionRepository {
    pub async fn save_session_review(&self, r: &SessionReview) -> Result<(), SessionError> {
        let goals = serde_json::to_string(&r.completed_goal_ids).unwrap_or_else(|_| "[]".into());
        let focuses = serde_json::to_string(&r.grammar_focuses).unwrap_or_else(|_| "[]".into());
        let vocab = serde_json::to_string(&r.vocabulary_used).unwrap_or_else(|_| "[]".into());
        sqlx::query(
            "INSERT INTO session_reviews (
                session_id, scenario_id, estimate_label, summary, cefr_signal,
                completed_goal_ids_json, grammar_focuses_json, vocabulary_used_json,
                turn_count, learner_word_count, created_at_unix_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
             ON CONFLICT(session_id) DO UPDATE SET
               scenario_id = excluded.scenario_id, estimate_label = excluded.estimate_label,
               summary = excluded.summary, cefr_signal = excluded.cefr_signal,
               completed_goal_ids_json = excluded.completed_goal_ids_json,
               grammar_focuses_json = excluded.grammar_focuses_json,
               vocabulary_used_json = excluded.vocabulary_used_json,
               turn_count = excluded.turn_count, learner_word_count = excluded.learner_word_count,
               created_at_unix_ms = excluded.created_at_unix_ms",
        )
        .bind(r.session_id.as_str())
        .bind(&r.scenario_id)
        .bind(&r.estimate_label)
        .bind(&r.summary)
        .bind(r.cefr_signal.as_str())
        .bind(goals)
        .bind(focuses)
        .bind(vocab)
        .bind(i64::from(r.turn_count))
        .bind(i64::from(r.learner_word_count))
        .bind(r.created_at_unix_ms)
        .execute(self.pool())
        .await
        .map_err(|e| persistence_err("Failed to save session review.", e))?;
        Ok(())
    }

    pub async fn find_session_review(
        &self,
        session_id: &SessionId,
    ) -> Result<Option<SessionReview>, SessionError> {
        let row = sqlx::query(
            "SELECT session_id, scenario_id, estimate_label, summary, cefr_signal,
                    completed_goal_ids_json, grammar_focuses_json, vocabulary_used_json,
                    turn_count, learner_word_count, created_at_unix_ms
             FROM session_reviews WHERE session_id = ?1",
        )
        .bind(session_id.as_str())
        .fetch_optional(self.pool())
        .await
        .map_err(|e| persistence_err("Failed to load session review.", e))?;
        row.as_ref().map(map_review_row).transpose()
    }

    pub async fn upsert_learner_fact(
        &self,
        user_id: &str,
        fact: &LearnerFact,
    ) -> Result<(), SessionError> {
        let exists = sqlx::query(
            "SELECT id FROM learner_facts WHERE user_id = ?1 AND lower(fact_text) = lower(?2) LIMIT 1",
        )
        .bind(user_id)
        .bind(&fact.fact_text)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| persistence_err("Failed to check learner memory.", e))?;
        if exists.is_some() {
            return Ok(());
        }
        sqlx::query(
            "INSERT INTO learner_facts (id, user_id, fact_text, source_session_id, created_at_unix_ms)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO NOTHING",
        )
        .bind(&fact.id).bind(user_id).bind(&fact.fact_text)
        .bind(&fact.source_session_id).bind(fact.created_at_unix_ms)
        .execute(self.pool()).await
        .map_err(|e| persistence_err("Failed to save learner fact.", e))?;
        Ok(())
    }

    pub async fn list_learner_facts(
        &self,
        user_id: &str,
        limit: u32,
    ) -> Result<Vec<LearnerFact>, SessionError> {
        let rows = sqlx::query(
            "SELECT id, fact_text, source_session_id, created_at_unix_ms
             FROM learner_facts WHERE user_id = ?1 ORDER BY created_at_unix_ms DESC LIMIT ?2",
        )
        .bind(user_id)
        .bind(i64::from(limit.min(50)))
        .fetch_all(self.pool())
        .await
        .map_err(|e| persistence_err("Failed to load learner facts.", e))?;
        Ok(rows.iter().map(map_fact_row).collect())
    }

    pub async fn delete_learner_fact(
        &self,
        user_id: &str,
        fact_id: &str,
    ) -> Result<bool, SessionError> {
        let res = sqlx::query("DELETE FROM learner_facts WHERE user_id = ?1 AND id = ?2")
            .bind(user_id)
            .bind(fact_id)
            .execute(self.pool())
            .await
            .map_err(|e| persistence_err("Failed to delete learner fact.", e))?;
        Ok(res.rows_affected() > 0)
    }

    pub async fn clear_learner_facts(&self, user_id: &str) -> Result<u64, SessionError> {
        let res = sqlx::query("DELETE FROM learner_facts WHERE user_id = ?1")
            .bind(user_id)
            .execute(self.pool())
            .await
            .map_err(|e| persistence_err("Failed to clear learner facts.", e))?;
        Ok(res.rows_affected())
    }

    pub async fn find_session_meta(
        &self,
        session_id: &SessionId,
    ) -> Result<Option<(String, String)>, SessionError> {
        let row = sqlx::query("SELECT user_id, scenario_mode FROM conversations WHERE id = ?1")
            .bind(session_id.as_str())
            .fetch_optional(self.pool())
            .await
            .map_err(|e| persistence_err("Failed to load session metadata.", e))?;
        Ok(row.map(|r| (r.get("user_id"), r.get("scenario_mode"))))
    }
}

fn map_review_row(row: &SqliteRow) -> Result<SessionReview, SessionError> {
    let raw_id: String = row.get("session_id");
    let sid = SessionId::new(raw_id).map_err(|e| persistence_err("Invalid session id.", e))?;
    let cefr: String = row.get("cefr_signal");
    Ok(SessionReview {
        session_id: sid,
        scenario_id: row.get("scenario_id"),
        estimate_label: row.get("estimate_label"),
        summary: row.get("summary"),
        cefr_signal: CefrLevel::from_str(&cefr).unwrap_or_default(),
        completed_goal_ids: parse_json_vec(row.get("completed_goal_ids_json")),
        grammar_focuses: parse_json_vec(row.get("grammar_focuses_json")),
        vocabulary_used: parse_json_vec(row.get("vocabulary_used_json")),
        turn_count: row.get::<i64, _>("turn_count").max(0) as u32,
        learner_word_count: row.get::<i64, _>("learner_word_count").max(0) as u32,
        created_at_unix_ms: row.get("created_at_unix_ms"),
    })
}

fn map_fact_row(row: &SqliteRow) -> LearnerFact {
    LearnerFact {
        id: row.get("id"),
        fact_text: row.get("fact_text"),
        source_session_id: row.get("source_session_id"),
        created_at_unix_ms: row.get("created_at_unix_ms"),
    }
}

fn parse_json_vec(raw: String) -> Vec<String> {
    serde_json::from_str(&raw).unwrap_or_default()
}
