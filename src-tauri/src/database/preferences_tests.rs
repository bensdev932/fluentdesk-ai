//! Integration tests for SQLite migration 002, `SessionRepository` preferences, and `LearnerPreferencesStore`.

use crate::database::connection::{
    init_sqlite_pool, init_sqlite_pool_at_path, run_migrations, MIGRATION_001_INITIAL_SCHEMA,
};
use crate::database::repository::SessionRepository;
use crate::domain::{
    CefrLevel, CorrectionMode, LearnerPreferencesPatch, ProviderId, ProviderPreference,
    RawAudioRetentionPolicy,
};
use crate::preferences::LearnerPreferencesStore;
use sqlx::{Executor, Row, SqlitePool};
use std::collections::HashMap;

#[tokio::test]
async fn migration_002_upgrades_v1_schema_idempotently_and_preserves_existing_rows() {
    let pool = SqlitePool::connect("sqlite::memory:")
        .await
        .expect("raw pool");
    pool.execute(
        "CREATE TABLE _schema_migrations (id TEXT PRIMARY KEY NOT NULL, applied_at_unix_ms INTEGER NOT NULL);",
    )
    .await
    .unwrap();
    pool.execute(MIGRATION_001_INITIAL_SCHEMA).await.unwrap();
    sqlx::query(
        "INSERT INTO _schema_migrations (id, applied_at_unix_ms) VALUES ('001_initial_schema', 1)",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO users (id, display_name, created_at_unix_ms, updated_at_unix_ms) VALUES ('default-user', 'Alex', 1, 1)")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO session_settings (user_id, correction_mode, speaking_speed, push_to_talk_enabled, cloud_opt_in, raw_audio_recording_opt_in, updated_at_unix_ms) VALUES ('default-user', 'Gentle', 1.4, 1, 1, 0, 1)")
        .execute(&pool)
        .await
        .unwrap();

    run_migrations(&pool).await.expect("upgrade to 002");
    run_migrations(&pool).await.expect("idempotent second run");

    let cols: Vec<String> = sqlx::query("PRAGMA table_info(session_settings)")
        .fetch_all(&pool)
        .await
        .unwrap()
        .into_iter()
        .map(|r| r.get::<String, _>("name"))
        .collect();
    assert!(cols.contains(&"preferred_provider".to_string()));
    assert!(cols.contains(&"model_overrides_json".to_string()));

    let store = LearnerPreferencesStore::new(SessionRepository::new(pool));
    let prefs = store.get_preferences().await.expect("load upgraded");
    assert_eq!(prefs.correction_mode, CorrectionMode::Gentle);
    assert_eq!(prefs.speaking_speed.as_f32(), 1.4);
    assert!(prefs.push_to_talk_enabled && prefs.cloud_opt_in);
    assert_eq!(prefs.preferred_provider, ProviderPreference::Auto);
    assert!(prefs.model_overrides.is_empty());
}

#[tokio::test]
async fn preferences_store_validates_speed_resets_cloud_provider_and_persists_across_reopen() {
    let dir = std::env::temp_dir().join(format!("fluentdesk-prefs-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let db_path = dir.join("fluentdesk.db");

    let pool1 = init_sqlite_pool_at_path(&db_path).await.expect("open db");
    let store1 = LearnerPreferencesStore::new(SessionRepository::new(pool1.clone()));

    let initial = store1.get_preferences().await.expect("defaults");
    assert_eq!(initial.correction_mode, CorrectionMode::Natural);
    assert_eq!(initial.cefr_level, CefrLevel::B1);
    assert!(!initial.cloud_opt_in && !initial.raw_audio_recording_opt_in);
    assert!(RawAudioRetentionPolicy::from_preferences(&initial)
        .require_explicit_recording_consent()
        .is_err());

    let bad = store1
        .patch_preferences(LearnerPreferencesPatch {
            speaking_speed: Some(2.4),
            ..Default::default()
        })
        .await;
    assert!(bad.is_err());

    let bad_voice = store1
        .patch_preferences(LearnerPreferencesPatch {
            english_voice_id: Some("hi-IN".into()),
            ..Default::default()
        })
        .await;
    assert!(bad_voice.is_err());

    let enabled = store1
        .patch_preferences(LearnerPreferencesPatch {
            correction_mode: Some(CorrectionMode::Detailed),
            speaking_speed: Some(1.35),
            push_to_talk_enabled: Some(true),
            cloud_opt_in: Some(true),
            raw_audio_recording_opt_in: Some(true),
            preferred_provider: Some(ProviderPreference::Gemini),
            model_overrides: HashMap::from([(ProviderId::Gemini, "gemini-3.5-flash".into())]),
            cefr_level: Some(CefrLevel::C1),
            english_voice_id: Some("en-GB".into()),
        })
        .await
        .expect("patch valid");
    assert_eq!(enabled.preferred_provider, ProviderPreference::Gemini);
    assert_eq!(enabled.cefr_level, CefrLevel::C1);
    assert_eq!(enabled.english_voice_id, "en-GB");

    let reset = store1
        .patch_preferences(LearnerPreferencesPatch {
            cloud_opt_in: Some(false),
            ..Default::default()
        })
        .await
        .expect("reset cloud");
    assert!(!reset.cloud_opt_in);
    assert_eq!(reset.preferred_provider, ProviderPreference::Auto);
    pool1.close().await;

    let pool2 = init_sqlite_pool_at_path(&db_path).await.expect("reopen db");
    let store2 = LearnerPreferencesStore::new(SessionRepository::new(pool2.clone()));
    let reloaded = store2.get_preferences().await.expect("reload");
    assert_eq!(reloaded.correction_mode, CorrectionMode::Detailed);
    assert_eq!(reloaded.cefr_level, CefrLevel::C1);
    assert_eq!(reloaded.speaking_speed.as_f32(), 1.35);
    assert!(reloaded.push_to_talk_enabled && reloaded.raw_audio_recording_opt_in);
    assert!(!reloaded.cloud_opt_in);
    assert_eq!(reloaded.preferred_provider, ProviderPreference::Auto);
    assert_eq!(
        reloaded
            .model_overrides
            .get(&ProviderId::Gemini)
            .map(String::as_str),
        Some("gemini-3.5-flash")
    );
    pool2.close().await;
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn memory_pool_migration_002_is_idempotent() {
    let pool = init_sqlite_pool("sqlite::memory:").await.unwrap();
    run_migrations(&pool).await.unwrap();
    let store = LearnerPreferencesStore::new(SessionRepository::new(pool));
    let prefs = store.get_preferences().await.unwrap();
    assert_eq!(prefs.preferred_provider, ProviderPreference::Auto);
}
