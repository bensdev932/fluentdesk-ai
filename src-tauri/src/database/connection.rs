//! SQLite connection pool and versioned migration runner (`T005`).

use crate::domain::{SessionError, SessionErrorKind};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::{Executor, Row, SqlitePool};
use std::path::Path;
use std::str::FromStr;
use std::time::Duration;

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

const MIGRATION_001_ID: &str = "001_initial_schema";
const MIGRATION_002_ID: &str = "002_learner_preferences";
const MIGRATION_003_ID: &str = "003_pedagogical_scaffolding";
const MIGRATION_004_ID: &str = "004_session_reviews_and_memory";
const MIGRATION_005_ID: &str = "005_learner_preferences_voice";

/// Opens a SQLite connection pool (e.g., `sqlite::memory:` or a local database path),
/// enables foreign key enforcement, and applies all pending schema migrations.
pub async fn init_sqlite_pool(database_url: &str) -> Result<SqlitePool, SessionError> {
    let is_memory = database_url.contains(":memory:");
    let base = SqliteConnectOptions::from_str(database_url)
        .map_err(|err| persistence_err("Failed to configure local database connection.", err))?;
    let options = configure_connect_options(base, !is_memory);
    let max_connections = if is_memory { 1 } else { 5 };
    connect_and_migrate(options, max_connections).await
}

/// Opens a persistent file-backed SQLite pool at `path`, creating parent folders if missing.
pub async fn init_sqlite_pool_at_path(path: &Path) -> Result<SqlitePool, SessionError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| persistence_err("Failed to create local database directory.", err))?;
    }
    let base = SqliteConnectOptions::new().filename(path);
    connect_and_migrate(configure_connect_options(base, true), 5).await
}

fn configure_connect_options(
    options: SqliteConnectOptions,
    file_backed: bool,
) -> SqliteConnectOptions {
    let common = options
        .create_if_missing(true)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(5));
    if file_backed {
        common
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
    } else {
        common
    }
}

async fn connect_and_migrate(
    options: SqliteConnectOptions,
    max_connections: u32,
) -> Result<SqlitePool, SessionError> {
    let pool = SqlitePoolOptions::new()
        .max_connections(max_connections)
        .connect_with(options)
        .await
        .map_err(|err| persistence_err("Failed to open local SQLite database.", err))?;
    run_migrations(&pool).await?;
    Ok(pool)
}

fn persistence_err(msg: &'static str, err: impl std::fmt::Display) -> SessionError {
    SessionError::new(
        SessionErrorKind::PersistenceFailed,
        msg,
        false,
        Some(err.to_string()),
    )
}

/// Applies versioned migrations idempotently to the given SQLite pool.
pub async fn run_migrations(pool: &SqlitePool) -> Result<(), SessionError> {
    ensure_migrations_table(pool).await?;
    apply_migration(pool, MIGRATION_001_ID, MIGRATION_001_INITIAL_SCHEMA, false).await?;
    let skip_002 = has_column(pool, "session_settings", "preferred_provider").await?;
    apply_migration(
        pool,
        MIGRATION_002_ID,
        MIGRATION_002_LEARNER_PREFERENCES,
        skip_002,
    )
    .await?;
    let skip_003 = has_column(pool, "session_settings", "cefr_level").await?;
    apply_migration(
        pool,
        MIGRATION_003_ID,
        MIGRATION_003_PEDAGOGICAL_SCAFFOLDING,
        skip_003,
    )
    .await?;
    let skip_004 = has_table(pool, "session_reviews").await?;
    apply_migration(
        pool,
        MIGRATION_004_ID,
        MIGRATION_004_REVIEWS_AND_MEMORY,
        skip_004,
    )
    .await?;
    let skip_005 = has_column(pool, "session_settings", "english_voice_id").await?;
    apply_migration(
        pool,
        MIGRATION_005_ID,
        MIGRATION_005_VOICE_PREFERENCE,
        skip_005,
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
