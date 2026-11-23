//! Integration tests for SQLite migration 006 structured feedback and review focuses.

use crate::database::connection::{init_sqlite_pool, run_migrations};
use crate::database::repository::SessionRepository;
use crate::domain::{
    CefrLevel, ConversationRole, ConversationTurn, CorrectionMode, MaxSessionTurns,
    PedagogicalScaffold, SessionId, SessionTopic,
};
use crate::modules::conversation::review_engine::build_session_review;
use sqlx::Row;

#[tokio::test]
async fn migration_006_persists_structured_feedback_and_populates_review_focuses() {
    let pool = init_sqlite_pool("sqlite::memory:")
        .await
        .expect("init sqlite");
    run_migrations(&pool).await.expect("idempotent migrations");

    let cols: Vec<String> = sqlx::query("PRAGMA table_info(conversation_turns)")
        .fetch_all(&pool)
        .await
        .unwrap()
        .into_iter()
        .map(|r| r.get::<String, _>("name"))
        .collect();
    assert!(cols.contains(&"proper_way".to_string()));
    assert!(cols.contains(&"feedback_why".to_string()));

    let repo = SessionRepository::new(pool);
    repo.ensure_default_user("u-fb", "Alex", 10).await.unwrap();
    let sid = SessionId::new("sess-fb-006").unwrap();
    repo.create_conversation(&sid, "u-fb", CorrectionMode::Gentle, 20)
        .await
        .unwrap();

    let user_turn = ConversationTurn {
        id: "t-u1".into(),
        session_id: sid.clone(),
        role: ConversationRole::User,
        text: "Give me coffee.".into(),
        correction: None,
        proper_way: None,
        why: None,
        follow_up_prompt: None,
        timestamp_unix_ms: 30,
        stage_metadata: None,
    };
    let tutor_turn = ConversationTurn {
        id: "t-t1".into(),
        session_id: sid.clone(),
        role: ConversationRole::Tutor,
        text: "Sure! What size would you like?".into(),
        correction: None,
        proper_way: Some("Could I get a coffee, please?".into()),
        why: Some("'Could I get...' sounds warmer and more natural at a counter.".into()),
        follow_up_prompt: Some("What size would you like?".into()),
        timestamp_unix_ms: 40,
        stage_metadata: None,
    };
    repo.append_turn(&user_turn, 0).await.unwrap();
    repo.append_turn(&tutor_turn, 1).await.unwrap();

    let history = repo.get_session_history(&sid).await.unwrap();
    assert_eq!(
        history.turns[1].proper_way.as_deref(),
        Some("Could I get a coffee, please?")
    );
    assert_eq!(
        history.turns[1].why.as_deref(),
        Some("'Could I get...' sounds warmer and more natural at a counter.")
    );

    let scaffold = PedagogicalScaffold {
        cefr_level: CefrLevel::A2,
        topic: SessionTopic::try_new("Cafe").unwrap(),
        max_turns: MaxSessionTurns::try_new(4).unwrap(),
        ..Default::default()
    };
    let review = build_session_review(&sid, &scaffold, &history.turns, 100);
    assert_eq!(
        review.grammar_focuses,
        vec!["Could I get a coffee, please? — 'Could I get...' sounds warmer and more natural at a counter."]
    );
}
