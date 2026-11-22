//! SQLite aggregation queries for pure-English `LearnerProgress` metrics.

use crate::database::mappers::persistence_err;
use crate::database::repository::SessionRepository;
use crate::domain::{LearnerProgress, SessionError};
use sqlx::Row;
use std::collections::BTreeSet;

const MS_PER_DAY: i64 = 86_400_000;
const MAX_SESSION_MS: i64 = 30 * 60 * 1_000;

impl SessionRepository {
    /// Distills completed sessions and reviews into a real pure-English `LearnerProgress` snapshot.
    pub async fn compute_learner_progress(
        &self,
        user_id: &str,
        now_unix_ms: i64,
    ) -> Result<LearnerProgress, SessionError> {
        let conv_rows = self.fetch_completed_conversation_times(user_id).await?;
        let rev_rows = self.fetch_review_progress_rows(user_id).await?;
        Ok(aggregate_progress(&conv_rows, &rev_rows, now_unix_ms))
    }

    async fn fetch_completed_conversation_times(
        &self,
        user_id: &str,
    ) -> Result<Vec<(i64, Option<i64>)>, SessionError> {
        let rows = sqlx::query(
            "SELECT c.started_at_unix_ms, c.ended_at_unix_ms
             FROM conversations c
             WHERE c.user_id = ?1
               AND (c.ended_at_unix_ms IS NOT NULL
                    OR EXISTS (SELECT 1 FROM conversation_turns t WHERE t.conversation_id = c.id))",
        )
        .bind(user_id)
        .fetch_all(self.pool())
        .await
        .map_err(|e| persistence_err("Failed to query conversation durations.", e))?;
        Ok(rows
            .into_iter()
            .map(|r| (r.get("started_at_unix_ms"), r.get("ended_at_unix_ms")))
            .collect())
    }

    async fn fetch_review_progress_rows(
        &self,
        user_id: &str,
    ) -> Result<Vec<(String, String, i64, i64)>, SessionError> {
        let rows = sqlx::query(
            "SELECT r.completed_goal_ids_json, r.vocabulary_used_json, r.learner_word_count, r.created_at_unix_ms
             FROM session_reviews r
             INNER JOIN conversations c ON c.id = r.session_id
             WHERE c.user_id = ?1
             ORDER BY r.created_at_unix_ms ASC",
        )
        .bind(user_id)
        .fetch_all(self.pool())
        .await
        .map_err(|e| persistence_err("Failed to query session review progress.", e))?;
        Ok(rows
            .into_iter()
            .map(|r| {
                (
                    r.get("completed_goal_ids_json"),
                    r.get("vocabulary_used_json"),
                    r.get("learner_word_count"),
                    r.get("created_at_unix_ms"),
                )
            })
            .collect())
    }
}

fn aggregate_progress(
    convs: &[(i64, Option<i64>)],
    reviews: &[(String, String, i64, i64)],
    now_unix_ms: i64,
) -> LearnerProgress {
    let total_sessions = u32::try_from(convs.len().max(reviews.len())).unwrap_or(0);
    let duration_ms: i64 = convs
        .iter()
        .map(|(start, end)| {
            end.unwrap_or(*start)
                .saturating_sub(*start)
                .clamp(0, MAX_SESSION_MS)
        })
        .sum();
    let total_words: i64 = reviews.iter().map(|(_, _, w, _)| (*w).max(0)).sum();
    let word_floor_ms = (total_words * 60_000) / 110;
    let effective_ms = duration_ms.max(word_floor_ms);
    let practice_minutes = if effective_ms > 0 {
        u32::try_from(((effective_ms + 59_999) / 60_000).max(1)).unwrap_or(0)
    } else {
        0
    };
    let (completed_goals_count, vocabulary_used) = collect_goals_and_vocab(reviews);
    let day_streak = compute_day_streak(convs, reviews, now_unix_ms);
    LearnerProgress {
        total_sessions,
        practice_minutes,
        completed_goals_count,
        vocabulary_used,
        day_streak,
    }
}

fn collect_goals_and_vocab(reviews: &[(String, String, i64, i64)]) -> (u32, Vec<String>) {
    let mut goals_total = 0_u32;
    let mut seen_lower = BTreeSet::new();
    let mut vocab = Vec::new();
    for (goals_json, vocab_json, _, _) in reviews {
        let goals: Vec<String> = serde_json::from_str(goals_json).unwrap_or_default();
        goals_total = goals_total.saturating_add(u32::try_from(goals.len()).unwrap_or(0));
        let words: Vec<String> = serde_json::from_str(vocab_json).unwrap_or_default();
        for word in words {
            let trimmed = word.trim();
            if !trimmed.is_empty() && seen_lower.insert(trimmed.to_ascii_lowercase()) {
                vocab.push(trimmed.to_string());
            }
        }
    }
    (goals_total, vocab)
}

fn compute_day_streak(
    convs: &[(i64, Option<i64>)],
    reviews: &[(String, String, i64, i64)],
    now_unix_ms: i64,
) -> u32 {
    let mut days: BTreeSet<i64> = convs
        .iter()
        .map(|(start, end)| end.unwrap_or(*start))
        .chain(reviews.iter().map(|(_, _, _, created)| *created))
        .filter(|ts| *ts > 0)
        .map(|ts| ts / MS_PER_DAY)
        .collect();
    let Some(&latest_day) = days.iter().next_back() else {
        return 0;
    };
    let today = if now_unix_ms > 0 {
        now_unix_ms / MS_PER_DAY
    } else {
        latest_day
    };
    if today.saturating_sub(latest_day) > 1 {
        return 0;
    }
    let mut cursor = latest_day;
    let mut streak = 0_u32;
    while days.remove(&cursor) {
        streak = streak.saturating_add(1);
        cursor -= 1;
    }
    streak
}
