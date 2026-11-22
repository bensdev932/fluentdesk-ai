//! Integration tests for bounded 4-phase pedagogical scaffolding & auto-closure on `SpeakingSessionHost`.

use crate::ai::mock::MockAiProvider;
use crate::database::{connection::init_sqlite_pool, repository::SessionRepository};
use crate::domain::{
    CefrLevel, ProviderHealth, SessionErrorKind, SessionPhase, SessionState, Transcript, TutorReply,
};
use crate::modules::conversation::{
    manager::SessionPipelineAdapters, SpeakingSessionHost, StartSpeakingSessionConfig,
};
use crate::modules::speech::{
    audio_capture::MockAudioInput, playback::MockAudioOutput, utterance::CompletedUtterance,
};
use crate::stt::mock::MockSttProvider;
use crate::tts::mock::MockTtsProvider;
use std::sync::Arc;

async fn make_scaffold_host(ai: Arc<MockAiProvider>) -> (SpeakingSessionHost, SessionRepository) {
    let repo = SessionRepository::new(init_sqlite_pool("sqlite::memory:").await.unwrap());
    let adapters = SessionPipelineAdapters {
        audio_input: Arc::new(MockAudioInput::default()),
        stt: Arc::new(MockSttProvider::default()),
        ai,
        tts: Arc::new(MockTtsProvider::default()),
        audio_output: Arc::new(MockAudioOutput::new()),
    };
    (SpeakingSessionHost::new(adapters, Some(repo.clone())), repo)
}

#[tokio::test]
async fn bounded_session_advances_through_all_four_phases_and_auto_ends() {
    let ai = Arc::new(MockAiProvider::new(
        ProviderHealth::Available,
        vec![
            Ok(TutorReply::new_validated("Welcome! Ready?", None, Some("Ready?".into())).unwrap()),
            Ok(
                TutorReply::new_validated("Check your luggage.", None, Some("Weight?".into()))
                    .unwrap(),
            ),
            Ok(TutorReply::new_validated("Have a safe flight!", None, None).unwrap()),
        ],
    ));
    let (host, repo) = make_scaffold_host(ai.clone()).await;
    let cfg = StartSpeakingSessionConfig {
        cefr_level: Some(CefrLevel::A2),
        topic: Some("Airport Check-in".into()),
        target_vocabulary: vec!["luggage".into(), "boarding pass".into()],
        max_turns: Some(3),
        ..Default::default()
    };
    let snap = host.start_session(cfg).await.unwrap();
    assert_eq!(snap.phase, SessionPhase::Opener);
    assert_eq!((snap.turn_count, snap.max_turns), (0, 3));
    assert_eq!(snap.cefr_level, CefrLevel::A2);
    assert_eq!(snap.topic, "Airport Check-in");

    let utt = CompletedUtterance::try_new(vec![0.1; 8_000], 500, false).unwrap();
    let t1 = host
        .process_spoken_utterance(&snap.session_id, &utt, 2_000)
        .await
        .unwrap();
    assert_eq!(t1.final_state, SessionState::Listening);
    assert_eq!(
        host.get_status(&snap.session_id).await.unwrap().phase,
        SessionPhase::FirstQuestion
    );

    let tx2 = Transcript::new("Two bags please.", "en", 800, Some(0.95)).unwrap();
    let t2 = host
        .submit_text_turn(&snap.session_id, &tx2, None, 3_000)
        .await
        .unwrap();
    assert_eq!(
        (t2.state, t2.phase, t2.turn_count),
        (SessionState::Listening, SessionPhase::Closer, 2)
    );

    let tx3 = Transcript::new("Thank you!", "en", 700, Some(0.95)).unwrap();
    let t3 = host
        .submit_text_turn(&snap.session_id, &tx3, None, 4_000)
        .await
        .unwrap();
    assert_eq!(
        (t3.state, t3.phase, t3.turn_count),
        (SessionState::Ended, SessionPhase::Completed, 3)
    );

    let view = repo
        .find_session_history(&snap.session_id)
        .await
        .unwrap()
        .unwrap();
    assert!(view.ended_at_unix_ms.is_some());

    let phases_seen: Vec<Option<SessionPhase>> = ai
        .recorded_scaffolds()
        .into_iter()
        .map(|s| s.map(|sc| sc.current_phase()))
        .collect();
    assert_eq!(
        phases_seen,
        vec![
            Some(SessionPhase::Opener),
            Some(SessionPhase::FirstQuestion),
            Some(SessionPhase::Closer),
        ]
    );

    let after_end = host
        .submit_text_turn(&snap.session_id, &tx3, None, 5_000)
        .await;
    assert_eq!(
        after_end.unwrap_err().kind,
        SessionErrorKind::InvalidStateTransition
    );
}

#[tokio::test]
async fn scenario_preset_persists_scenario_mode_tracks_goals_and_avoids_closer_question_conflict() {
    let ai = Arc::new(MockAiProvider::new(
        ProviderHealth::Available,
        vec![
            Ok(TutorReply::new_validated(
                "Welcome! What would you like?",
                None,
                Some("What size?".into()),
            )
            .unwrap()),
            Ok(
                TutorReply::new_validated("Great pastry choice! Have a lovely day.", None, None)
                    .unwrap()
                    .with_completed_goals(vec!["ask_option".into()]),
            ),
        ],
    ));
    let (host, repo) = make_scaffold_host(ai.clone()).await;
    let cfg = StartSpeakingSessionConfig {
        scenario_id: Some("cafe_order".into()),
        max_turns: Some(2),
        ..Default::default()
    };
    let snap = host.start_session(cfg).await.unwrap();
    assert_eq!(
        (snap.scenario_id.as_str(), snap.cefr_level, snap.goals.len()),
        ("cafe_order", CefrLevel::A2, 3)
    );

    let tx1 = Transcript::new(
        "Could I order an iced latte with oat milk please?",
        "en",
        0,
        None,
    )
    .unwrap();
    let t1 = host
        .submit_text_turn(&snap.session_id, &tx1, None, 1_000)
        .await
        .unwrap();
    assert!(t1.completed_goal_ids.contains(&"order_drink".to_string()));

    let tx2 =
        Transcript::new("And can I have a muffin to go with receipt?", "en", 0, None).unwrap();
    let t2 = host
        .submit_text_turn(&snap.session_id, &tx2, None, 2_000)
        .await
        .unwrap();
    assert!(t2.completed_goal_ids.contains(&"ask_option".to_string()));
    assert!(t2
        .completed_goal_ids
        .contains(&"checkout_choice".to_string()));

    let saved_mode: String =
        sqlx::query_scalar("SELECT scenario_mode FROM conversations WHERE id = ?1")
            .bind(snap.session_id.as_str())
            .fetch_one(repo.pool())
            .await
            .unwrap();
    assert_eq!(saved_mode, "cafe_order");

    let closer_prompt = crate::modules::tutor::prompts::assemble_scaffolded_tutor_prompt(
        &[],
        &tx2,
        crate::domain::CorrectionMode::Natural,
        ai.recorded_scaffolds().last().unwrap().as_ref(),
    );
    assert!(closer_prompt
        .system_instruction()
        .contains("Set followUpPrompt to null and ask no further question"));
    assert!(!closer_prompt
        .system_instruction()
        .contains("where followUpPrompt contains only that closing question"));
}
