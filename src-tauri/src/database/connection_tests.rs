use crate::database::connection::{init_sqlite_pool, init_sqlite_pool_at_path};
use sqlx::SqlitePool;

async fn pragma_i64(pool: &SqlitePool, name: &str) -> i64 {
    sqlx::query_scalar(&format!("PRAGMA {name}"))
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn file_backed_sqlite_sets_wal_normal_sync_and_memory_stays_compatible() {
    let dir = std::env::temp_dir().join(format!("fluentdesk_wal_{}", std::process::id()));
    let pool = init_sqlite_pool_at_path(&dir.join("wal.db")).await.unwrap();
    let mode: String = sqlx::query_scalar("PRAGMA journal_mode")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(mode.to_ascii_lowercase(), "wal");
    assert_eq!(pragma_i64(&pool, "synchronous").await, 1);
    assert_eq!(pragma_i64(&pool, "foreign_keys").await, 1);
    assert_eq!(pragma_i64(&pool, "busy_timeout").await, 5000);
    pool.close().await;
    let _ = std::fs::remove_dir_all(&dir);
    let mem = init_sqlite_pool("sqlite::memory:").await.unwrap();
    assert_eq!(pragma_i64(&mem, "foreign_keys").await, 1);
    assert_eq!(pragma_i64(&mem, "busy_timeout").await, 5000);
}
