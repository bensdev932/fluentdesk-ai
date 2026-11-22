//! Unit and integration tests for SQLite schema migrations and `SessionRepository` (`T005`).

use crate::database::connection::{init_sqlite_pool, init_sqlite_pool_at_path, run_migrations};
use crate::database::repository::SessionRepository;
use crate::domain::{
    ConversationRole, ConversationTurn, CorrectionMode, SessionId, TurnStageMetadata,
};
use sqlx::Row;

#[tokio::test]
async fn migrations_are_idempotent_and_create_required_tables_without_raw_audio_columns() {
    let pool = init_sqlite_pool("sqlite::memory:")
        .await
        .expect("initialize in-memory sqlite pool");

    run_migrations(&pool)
        .await
        .expect("second migration run should succeed idempotently");

    let rows = sqlx::query(
        "SELECT name FROM sqlite_master WHERE type = 'table' AND name IN ('users', 'session_settings', 'conversations', 'conversation_turns') ORDER BY name",
    )
    .fetch_all(&pool)
    .await
    .expect("query sqlite_master");

    let table_names: Vec<String> = rows.iter().map(|r| r.get::<String, _>("name")).collect();
    assert_eq!(
        table_names,
        vec![
            "conversation_turns",
            "conversations",
            "session_settings",
            "users"
        ]
    );

    let turn_cols = sqlx::query("PRAGMA table_info(conversation_turns)")
        .fetch_all(&pool)
        .await
        .expect("inspect conversation_turns columns");

    for col in turn_cols {
        let name: String = col.get("name");
        let col_type: String = col.get("type");
        assert!(
            !name.to_ascii_lowercase().contains("audio"),
            "Unexpected raw audio column {name}"
        );
        assert_ne!(
            col_type.to_ascii_uppercase(),
            "BLOB",
            "Unexpected BLOB column {name} in conversation_turns"
        );
    }
}

#[tokio::test]
async fn repository_persists_user_session_coaching_tips_and_end_timestamp() {
    let pool = init_sqlite_pool("sqlite::memory:")
        .await
        .expect("init sqlite");
    let repo = SessionRepository::new(pool);

    repo.ensure_default_user("user-1", "Alex", 1_700_000_000_000)
        .await
        .expect("user");
    let settings = repo
        .get_or_create_preferences("user-1", 1_700_000_000_000)
        .await
        .expect("settings");
    assert_eq!(settings.correction_mode, CorrectionMode::Natural);
    assert!(!settings.cloud_opt_in && !settings.raw_audio_recording_opt_in);

    let session_id = SessionId::new("sess-100").expect("valid session id");
    repo.create_conversation(
        &session_id,
        "user-1",
        CorrectionMode::Gentle,
        1_700_000_001_000,
    )
    .await
    .expect("create conversation");

    let turn_user = ConversationTurn {
        id: "turn-1".to_string(),
        session_id: session_id.clone(),
        role: ConversationRole::User,
        text: "Yesterday I go to the cinema.".to_string(),
        correction: None,
        follow_up_prompt: None,
        timestamp_unix_ms: 1_700_000_002_000,
        stage_metadata: Some(TurnStageMetadata {
            stt_duration_ms: Some(180),
            llm_duration_ms: None,
            tts_duration_ms: None,
            correction_mode: Some(CorrectionMode::Gentle),
        }),
    };

    let turn_tutor = ConversationTurn {
        id: "turn-2".to_string(),
        session_id: session_id.clone(),
        role: ConversationRole::Tutor,
        text: "Oh nice, you went to the cinema! What movie did you watch?".to_string(),
        correction: Some("Say \"I went\" for yesterday.".to_string()),
        follow_up_prompt: Some("Who did you go with?".to_string()),
        timestamp_unix_ms: 1_700_000_003_000,
        stage_metadata: Some(TurnStageMetadata {
            stt_duration_ms: None,
            llm_duration_ms: Some(420),
            tts_duration_ms: Some(210),
            correction_mode: Some(CorrectionMode::Gentle),
        }),
    };

    repo.append_turn(&turn_user, 0).await.expect("u turn");
    repo.append_turn(&turn_tutor, 1).await.expect("t turn");
    repo.mark_conversation_ended(&session_id, 1_700_000_010_000)
        .await
        .expect("end");

    let review = repo
        .get_session_history(&session_id)
        .await
        .expect("history");
    assert_eq!(review.ended_at_unix_ms, Some(1_700_000_010_000));
    assert_eq!(review.turns.len(), 2);
    assert_eq!(review.turns[0].text, "Yesterday I go to the cinema.");
    assert_eq!(
        review.turns[1].correction.as_deref(),
        Some("Say \"I went\" for yesterday.")
    );
    assert_eq!(
        review.turns[1].follow_up_prompt.as_deref(),
        Some("Who did you go with?")
    );
    assert_eq!(
        review.turns[1]
            .stage_metadata
            .as_ref()
            .and_then(|m| m.correction_mode),
        Some(CorrectionMode::Gentle)
    );
}

#[tokio::test]
async fn repository_enforces_foreign_key_and_unique_sequence_constraints() {
    let pool = init_sqlite_pool("sqlite::memory:")
        .await
        .expect("init sqlite");
    let repo = SessionRepository::new(pool);

    let missing_session = SessionId::new("non-existent-session").expect("valid id");
    let orphan_turn = ConversationTurn {
        id: "orphan-1".to_string(),
        session_id: missing_session,
        role: ConversationRole::User,
        text: "Hello?".to_string(),
        correction: None,
        follow_up_prompt: None,
        timestamp_unix_ms: 1_700_000_000_000,
        stage_metadata: None,
    };

    let err = repo.append_turn(&orphan_turn, 0).await;
    assert!(err.is_err());
}

#[tokio::test]
async fn init_sqlite_pool_at_path_persists_across_reopen_and_find_history_handles_missing() {
    let dir = std::env::temp_dir().join(format!("fluentdesk-db-test-{}", std::process::id()));
    let db_path = dir.join("nested").join("fluentdesk.db");
    let pool = init_sqlite_pool_at_path(&db_path).await.expect("open db");
    let repo = SessionRepository::new(pool.clone());
    let missing = SessionId::new("does-not-exist").unwrap();
    assert!(repo.find_session_history(&missing).await.unwrap().is_none());
    let sid = SessionId::new("sess-file-1").unwrap();
    repo.ensure_default_user("u1", "Learner", 10).await.unwrap();
    repo.create_conversation(&sid, "u1", CorrectionMode::Natural, 20)
        .await
        .unwrap();
    pool.close().await;

    let reopened = init_sqlite_pool_at_path(&db_path).await.expect("reopen");
    let view = SessionRepository::new(reopened.clone())
        .find_session_history(&sid)
        .await
        .unwrap()
        .expect("persisted session");
    assert_eq!(view.session_id, sid);
    reopened.close().await;
    let _ = std::fs::remove_dir_all(dir);
}
