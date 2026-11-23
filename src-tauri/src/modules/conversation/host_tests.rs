//! Integration tests for `SpeakingSessionHost` session verification,
//! multi-turn SQLite history context, and AI-failure transcript preservation.

use crate::ai::mock::MockAiProvider;
use crate::domain::{
    CorrectionMode, ProviderHealth, SessionError, SessionErrorKind, SessionId, SessionState,
    Transcript, TutorReply,
};
use crate::modules::conversation::{pipeline_tests::make_host, StartSpeakingSessionConfig};
use crate::modules::speech::{audio_capture::AudioInput, playback::MockAudioOutput};
use crate::stt::mock::MockSttProvider;
use std::sync::Arc;

#[tokio::test]
async fn host_rejects_mismatched_session_id_and_preserves_turns_across_ai_failure() {
    let err_ai = Arc::new(MockAiProvider::new(
        ProviderHealth::Available,
        vec![
            Err(SessionError::new(
                SessionErrorKind::AiInferenceFailed,
                "down",
                true,
                None,
            )),
            Ok(TutorReply::new_validated(
                "Recovered! What next?",
                Some("Say 'went'.".into()),
                Some("What next?".into()),
            )
            .unwrap()),
        ],
    ));
    let (host, _, repo) = make_host(
        Arc::new(MockSttProvider::default()),
        err_ai,
        Arc::new(MockAudioOutput::new()),
    )
    .await;
    let wrong = SessionId::new("wrong-session").unwrap();
    assert_eq!(
        host.get_status(&wrong).await.unwrap_err().kind,
        SessionErrorKind::InvalidStateTransition
    );

    let started = host
        .start_session(StartSpeakingSessionConfig::default())
        .await
        .unwrap();
    assert_eq!(
        host.get_status(&wrong).await.unwrap_err().kind,
        SessionErrorKind::InvalidStateTransition
    );
    let tx1 = Transcript::new("Yesterday I go home.", "en", 0, None).unwrap();
    assert_eq!(
        host.submit_text_turn(&started.session_id, &tx1, None, 1_000)
            .await
            .unwrap_err()
            .kind,
        SessionErrorKind::AiInferenceFailed
    );
    assert_eq!(
        host.get_status(&started.session_id).await.unwrap().state,
        SessionState::Error
    );
    assert_eq!(
        host.resume_session(&started.session_id)
            .await
            .unwrap()
            .state,
        SessionState::Listening
    );

    let tx2 = Transcript::new("Then I ate dinner.", "en", 0, None).unwrap();
    let ok = host
        .submit_text_turn(
            &started.session_id,
            &tx2,
            Some(CorrectionMode::Gentle),
            2_000,
        )
        .await
        .unwrap();
    assert_eq!(ok.state, SessionState::Listening);
    assert_eq!(ok.tutor_turn.correction.as_deref(), Some("Say 'went'."));
    assert_eq!(
        repo.list_turns_chronological(&started.session_id)
            .await
            .unwrap()
            .len(),
        3
    );
}

#[tokio::test]
async fn host_supplies_prior_turns_to_ai_and_stops_previous_session_on_new_start() {
    let ai = Arc::new(MockAiProvider::default());
    let (host, _, _) = make_host(
        Arc::new(MockSttProvider::default()),
        ai.clone(),
        Arc::new(MockAudioOutput::new()),
    )
    .await;

    let s1 = host
        .start_session(StartSpeakingSessionConfig::default())
        .await
        .unwrap();
    let t1 = Transcript::new("I adopted a puppy.", "en", 0, None).unwrap();
    let t2 = Transcript::new("His name is Milo.", "en", 0, None).unwrap();
    host.submit_text_turn(&s1.session_id, &t1, None, 1_000)
        .await
        .unwrap();
    host.submit_text_turn(&s1.session_id, &t2, None, 2_000)
        .await
        .unwrap();

    let calls = ai.recorded_calls();
    assert_eq!((calls[0].0, calls[1].0), (0, 2));
    assert_eq!(calls[1].2, CorrectionMode::Gentle);

    let s2 = host
        .start_session(StartSpeakingSessionConfig {
            correction_mode: CorrectionMode::Detailed,
            push_to_talk: true,
            ..Default::default()
        })
        .await
        .unwrap();
    assert_ne!(s1.session_id, s2.session_id);
    assert_eq!(
        host.get_status(&s1.session_id).await.unwrap_err().kind,
        SessionErrorKind::InvalidStateTransition
    );
    let summaries = host.list_sessions().await.unwrap();
    assert_eq!(summaries.len(), 2);
    let prev_hist = host
        .find_session_history(&s1.session_id)
        .await
        .unwrap()
        .unwrap();
    assert!(prev_hist.ended_at_unix_ms.is_some());
    assert_eq!(prev_hist.turns.len(), 4);
}

#[tokio::test]
async fn start_pause_resume_and_stop_manage_audio_and_sqlite_truthfully() {
    let (host, audio_in, repo) = make_host(
        Arc::new(MockSttProvider::default()),
        Arc::new(MockAiProvider::default()),
        Arc::new(MockAudioOutput::new()),
    )
    .await;
    let cfg = StartSpeakingSessionConfig {
        correction_mode: CorrectionMode::Gentle,
        push_to_talk: true,
        ..Default::default()
    };
    let s = host.start_session(cfg).await.unwrap();
    assert_eq!(
        (s.state, s.correction_mode, s.push_to_talk),
        (SessionState::Listening, CorrectionMode::Gentle, true)
    );
    assert!(audio_in.is_capturing());
    assert_eq!(
        host.pause_session(&s.session_id).await.unwrap().state,
        SessionState::Paused
    );
    assert!(!audio_in.is_capturing());
    assert_eq!(
        host.resume_session(&s.session_id).await.unwrap().state,
        SessionState::Listening
    );
    assert!(audio_in.is_capturing());
    let stopped = host.stop_session(&s.session_id).await.unwrap();
    assert_eq!(stopped.state, SessionState::Ended);
    assert!(!audio_in.is_capturing());
    assert!(host.is_cancelled(&s.session_id).await.unwrap());
    let saved = repo
        .find_session_history(&s.session_id)
        .await
        .unwrap()
        .unwrap();
    assert!(saved.ended_at_unix_ms.is_some());
}
