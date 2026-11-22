//! Tests for async `SessionReviewEngine`, text-only `LearnerFact` extraction, memory injection,
//! and explicit `AI Estimate` review labeling.

use crate::ai::mock::MockAiProvider;
use crate::database::{connection::init_sqlite_pool, repository::SessionRepository};
use crate::domain::{CefrLevel, CorrectionMode, Transcript, TutorReply};
use crate::modules::conversation::{
    manager::SessionPipelineAdapters, SpeakingSessionHost, StartSpeakingSessionConfig,
};
use crate::modules::speech::{audio_capture::MockAudioInput, playback::MockAudioOutput};
use crate::modules::tutor::prompts::assemble_scaffolded_tutor_prompt;
use crate::stt::mock::MockSttProvider;
use crate::tts::mock::MockTtsProvider;
use std::sync::Arc;

async fn setup_review_host(ai: Arc<MockAiProvider>) -> (SpeakingSessionHost, SessionRepository) {
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
async fn stop_generates_ai_estimate_review_and_injects_deletable_learner_facts() {
    let ai = Arc::new(MockAiProvider::default());
    let (host, repo) = setup_review_host(ai.clone()).await;

    let s1 = host
        .start_session(StartSpeakingSessionConfig {
            scenario_id: Some("cafe_order".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    let tx1 = Transcript::new(
        "My name is Maya and I work as a designer. I want an iced latte please.",
        "en",
        0,
        None,
    )
    .unwrap();
    host.submit_text_turn(&s1.session_id, &tx1, Some(CorrectionMode::Gentle), 1_000)
        .await
        .unwrap();
    host.stop_session_at(&s1.session_id, 2_000).await.unwrap();

    let review = host
        .get_or_generate_session_review(&s1.session_id)
        .await
        .unwrap()
        .expect("review generated");
    assert_eq!(review.estimate_label, "AI Estimate");
    assert_eq!(review.scenario_id, "cafe_order");
    assert_eq!(review.cefr_signal, CefrLevel::A2);
    assert!(review.summary.contains("AI Estimate"));
    assert!(review
        .completed_goal_ids
        .contains(&"order_drink".to_string()));

    let facts = host.list_learner_facts().await.unwrap();
    assert!(!facts.is_empty());
    assert!(facts[0].fact_text.contains("Maya"));

    // Next session injects stored Learner Facts into RoleplayDirector prompt automatically.
    let s2 = host
        .start_session(StartSpeakingSessionConfig::default())
        .await
        .unwrap();
    let tx2 = Transcript::new("Hello again!", "en", 0, None).unwrap();
    host.submit_text_turn(&s2.session_id, &tx2, None, 3_000)
        .await
        .unwrap();
    let second_scaffold = ai.recorded_scaffolds().last().unwrap().clone().unwrap();
    let next_prompt = assemble_scaffolded_tutor_prompt(
        &[],
        &tx2,
        CorrectionMode::Natural,
        Some(&second_scaffold),
    );
    assert!(next_prompt.system_instruction().contains("Maya"));

    assert!(host.delete_learner_fact(&facts[0].id).await.unwrap());
    assert!(host.list_learner_facts().await.unwrap().is_empty());
    let _ = (repo, TutorReply::new_validated("Ok", None, None));
}
