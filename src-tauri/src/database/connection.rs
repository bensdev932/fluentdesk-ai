//! SQLite connection pool and versioned migration runner (`T005`).

pub use crate::database::schema_migrations::{
    run_migrations, MIGRATION_001_INITIAL_SCHEMA, MIGRATION_002_LEARNER_PREFERENCES,
    MIGRATION_003_PEDAGOGICAL_SCAFFOLDING, MIGRATION_004_REVIEWS_AND_MEMORY,
    MIGRATION_005_VOICE_PREFERENCE, MIGRATION_006_STRUCTURED_FEEDBACK,
};
use crate::domain::{SessionError, SessionErrorKind};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::SqlitePool;
use std::path::Path;
use std::str::FromStr;
use std::time::Duration;

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
