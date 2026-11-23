//! Embedded SQLite migrations and idempotent schema migration runner (`T005`).

use crate::domain::{SessionError, SessionErrorKind};
use sqlx::{Executor, Row, SqlitePool};

/// Initial schema SQL migration (`001_initial_schema.sql`).
pub const MIGRATION_001_INITIAL_SCHEMA: &str = include_str!("migrations/001_initial_schema.sql");
/// Learner routing preferences SQL migration (`002_learner_preferences.sql`).
pub const MIGRATION_002_LEARNER_PREFERENCES: &str =
    include_str!("migrations/002_learner_preferences.sql");
/// Learner CEFR proficiency preference SQL migration (`003_pedagogical_scaffolding.sql`).
pub const MIGRATION_003_PEDAGOGICAL_SCAFFOLDING: &str =
    include_str!("migrations/003_pedagogical_scaffolding.sql");
/// Post-session reviews and learner memory SQL migration (`004_session_reviews_and_memory.sql`).
pub const MIGRATION_004_REVIEWS_AND_MEMORY: &str =
    include_str!("migrations/004_session_reviews_and_memory.sql");
/// Pure-English voice preference SQL migration (`005_learner_preferences_voice.sql`).
pub const MIGRATION_005_VOICE_PREFERENCE: &str =
    include_str!("migrations/005_learner_preferences_voice.sql");
/// Structured coaching feedback SQL migration (`006_structured_feedback.sql`).
pub const MIGRATION_006_STRUCTURED_FEEDBACK: &str =
    include_str!("migrations/006_structured_feedback.sql");
/// Pre-conversation script toggle SQL migration (`007_pre_conversation_script.sql`).
pub const MIGRATION_007_PRE_CONVERSATION_SCRIPT: &str =
    include_str!("migrations/007_pre_conversation_script.sql");

fn persistence_err(msg: &'static str, err: impl std::fmt::Display) -> SessionError {
    SessionError::new(
        SessionErrorKind::PersistenceFailed,
        msg,
        false,
        Some(err.to_string()),
    )
}

async fn migrate_cols(
    pool: &SqlitePool,
    items: &[(&str, &str, &str, &str)],
) -> Result<(), SessionError> {
    for &(id, sql, tbl, col) in items {
        let skip = has_column(pool, tbl, col).await?;
        apply_migration(pool, id, sql, skip).await?;
    }
    Ok(())
}

/// Applies versioned migrations idempotently to the given SQLite pool.
pub async fn run_migrations(pool: &SqlitePool) -> Result<(), SessionError> {
    ensure_migrations_table(pool).await?;
    apply_migration(
        pool,
        "001_initial_schema",
        MIGRATION_001_INITIAL_SCHEMA,
        false,
    )
    .await?;
    migrate_cols(
        pool,
        &[
            (
                "002_learner_preferences",
                MIGRATION_002_LEARNER_PREFERENCES,
                "session_settings",
                "preferred_provider",
            ),
            (
                "003_pedagogical_scaffolding",
                MIGRATION_003_PEDAGOGICAL_SCAFFOLDING,
                "session_settings",
                "cefr_level",
            ),
        ],
    )
    .await?;
    let skip_004 = has_table(pool, "session_reviews").await?;
    apply_migration(
        pool,
        "004_session_reviews_and_memory",
        MIGRATION_004_REVIEWS_AND_MEMORY,
        skip_004,
    )
    .await?;
    migrate_cols(
        pool,
        &[
            (
                "005_learner_preferences_voice",
                MIGRATION_005_VOICE_PREFERENCE,
                "session_settings",
                "english_voice_id",
            ),
            (
                "006_structured_feedback",
                MIGRATION_006_STRUCTURED_FEEDBACK,
                "conversation_turns",
                "proper_way",
            ),
            (
                "007_pre_conversation_script",
                MIGRATION_007_PRE_CONVERSATION_SCRIPT,
                "session_settings",
                "pre_conversation_script_enabled",
            ),
        ],
    )
    .await
}

async fn has_table(pool: &SqlitePool, name: &str) -> Result<bool, SessionError> {
    let row = sqlx::query("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1")
        .bind(name)
        .fetch_optional(pool)
        .await
        .map_err(|err| persistence_err("Failed to inspect database tables.", err))?;
    Ok(row.is_some())
}

async fn ensure_migrations_table(pool: &SqlitePool) -> Result<(), SessionError> {
    pool.execute(
        "CREATE TABLE IF NOT EXISTS _schema_migrations (
            id TEXT PRIMARY KEY NOT NULL,
            applied_at_unix_ms INTEGER NOT NULL
        );",
    )
    .await
    .map_err(|err| persistence_err("Failed to initialize schema migration table.", err))?;
    Ok(())
}

async fn has_column(pool: &SqlitePool, table: &str, col: &str) -> Result<bool, SessionError> {
    let query = format!("PRAGMA table_info({table})");
    let rows = sqlx::query(&query)
        .fetch_all(pool)
        .await
        .map_err(|err| persistence_err("Failed to inspect database schema columns.", err))?;
    Ok(rows.iter().any(|r| r.get::<String, _>("name") == col))
}

async fn apply_migration(
    pool: &SqlitePool,
    id: &str,
    sql: &str,
    skip_sql: bool,
) -> Result<(), SessionError> {
    let existing = sqlx::query("SELECT id FROM _schema_migrations WHERE id = ?1")
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(|err| persistence_err("Failed to check applied database migrations.", err))?;
    if existing.is_some() {
        return Ok(());
    }
    let mut tx = pool
        .begin()
        .await
        .map_err(|err| persistence_err("Failed to start database migration transaction.", err))?;
    if !skip_sql {
        tx.execute(sql)
            .await
            .map_err(|err| persistence_err("Failed to apply database schema migration.", err))?;
    }
    sqlx::query("INSERT INTO _schema_migrations (id, applied_at_unix_ms) VALUES (?1, ?2)")
        .bind(id)
        .bind(0_i64)
        .execute(&mut *tx)
        .await
        .map_err(|err| persistence_err("Failed to record applied database migration.", err))?;
    tx.commit()
        .await
        .map_err(|err| persistence_err("Failed to commit database migration transaction.", err))
}
