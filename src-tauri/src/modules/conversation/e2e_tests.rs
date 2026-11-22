//! End-to-end speaking session, offline execution, stop-cancellation, and failure-recovery tests (`T048`, `T050`, `T051`).

use crate::ai::mock::MockAiProvider;
use crate::database::{connection::init_sqlite_pool, repository::SessionRepository};
use crate::domain::{
    ConversationTurn, CorrectionMode, ProviderHealth, SessionError, SessionErrorKind, SessionId,
    SessionState, TutorReply,
};
use crate::modules::conversation::{
    context::MaxContextTurns,
    manager::{SessionManager, SessionPipelineAdapters},
    turn::EndToEndTurnResult,
    SessionStatusSnapshot, StartSpeakingSessionConfig,
};
use crate::modules::speech::{
    audio_capture::{AudioInput, MockAudioInput},
    playback::MockAudioOutput,
    synthesis::TutorPlaybackOutcome,
    utterance::CompletedUtterance,
};
use crate::stt::mock::MockSttProvider;
use crate::tts::{mock::MockTtsProvider, provider::SpeakingSpeed};
use std::sync::Arc;

struct TestHarness {
    session: SessionManager,
    audio_in: Arc<MockAudioInput>,
    audio_out: Arc<MockAudioOutput>,
    repo: SessionRepository,
}

impl TestHarness {
    async fn start(&mut self, ts: i64) -> Result<SessionStatusSnapshot, SessionError> {
        let cfg = StartSpeakingSessionConfig {
            correction_mode: CorrectionMode::Gentle,
            ..Default::default()
        };
        self.session.start_session(&cfg, "u1", ts).await
    }

    async fn speak(&mut self, ts: i64) -> Result<EndToEndTurnResult, SessionError> {
        let utt = CompletedUtterance::try_new(vec![0.15_f32; 16_000], 1_000, false).expect("utt");
        self.session.process_spoken_utterance(&utt, ts).await
    }

    async fn turns(&self) -> Vec<ConversationTurn> {
        let sid = self.session.session_id();
        self.repo
            .list_turns_chronological(sid)
            .await
            .expect("turns")
    }
}

async fn setup_harness(
    stt: Arc<MockSttProvider>,
    ai: Arc<MockAiProvider>,
    tts: Arc<MockTtsProvider>,
) -> TestHarness {
    let pool = init_sqlite_pool("sqlite::memory:").await.expect("db");
    let repo = SessionRepository::new(pool);
    let audio_in = Arc::new(MockAudioInput::default());
    let audio_out = Arc::new(MockAudioOutput::new());
    let sid = SessionId::new("sess-e2e-1").expect("sid");
    let max = MaxContextTurns::try_new(6).expect("max");
    let speed = SpeakingSpeed::try_new(1.0).expect("speed");
    let adapters = SessionPipelineAdapters {
        audio_input: audio_in.clone(),
        stt,
        ai,
        tts,
        audio_output: audio_out.clone(),
    };
    let session = SessionManager::with_persistence(sid, max, speed, adapters, repo.clone());
    TestHarness {
        session,
        audio_in,
        audio_out,
        repo,
    }
}

#[tokio::test]
async fn offline_end_to_end_multi_turn_loop_completes_and_retains_capture_stream() {
    let stt = Arc::new(MockSttProvider::default());
    let ai = Arc::new(MockAiProvider::default());
    let tts = Arc::new(MockTtsProvider::default());
    let mut h = setup_harness(stt, ai, tts).await;
    h.start(1_000).await.expect("start");
    assert!(h.session.is_capture_stream_open());
    let r1 = h.speak(2_000).await.expect("t1");
    let r2 = h.speak(3_000).await.expect("t2");
    assert_eq!(r1.final_state, SessionState::Listening);
    assert_eq!(r2.final_state, SessionState::Listening);
    assert_eq!(h.audio_out.played_chunks().len(), 2);
    assert_eq!(h.turns().await.len(), 4);
}

#[tokio::test]
async fn stop_immediately_cancels_active_session_and_closes_capture_and_playback() {
    let stt = Arc::new(MockSttProvider::default());
    let ai = Arc::new(MockAiProvider::default());
    let tts = Arc::new(MockTtsProvider::default());
    let mut h = setup_harness(stt, ai, tts).await;
    h.start(1_000).await.expect("start");
    let snap = h.session.stop().await.expect("stop");
    assert_eq!(snap.state, SessionState::Ended);
    assert!(!h.session.is_capture_stream_open());
    assert!(!h.audio_in.is_capturing());
    assert!(h.audio_out.stopped_count() >= 1);
}

#[tokio::test]
async fn failed_startup_does_not_persist_orphan_listening_row_and_recovers_cleanly() {
    let stt_err = SessionError::new(SessionErrorKind::SttFailed, "STT timeout", true, None);
    let stt = Arc::new(MockSttProvider::new(
        ProviderHealth::Available,
        vec![Err(stt_err)],
    ));
    let ai = Arc::new(MockAiProvider::default());
    let tts = Arc::new(MockTtsProvider::default());
    let mut h = setup_harness(stt, ai, tts).await;
    let mic_err = SessionError::new(
        SessionErrorKind::AudioDeviceUnavailable,
        "Unplugged",
        true,
        None,
    );
    h.audio_in.set_failure(Some(mic_err));
    assert!(h.start(1_000).await.is_err());
    assert_eq!(h.session.state(), SessionState::Error);
    assert_eq!(
        h.repo
            .list_sessions_for_user("u1")
            .await
            .expect("sessions")
            .len(),
        0
    );
    h.audio_in.set_failure(None);
    h.start(1_500).await.expect("recovered start");
    assert_eq!(
        h.repo
            .list_sessions_for_user("u1")
            .await
            .expect("sessions")
            .len(),
        1
    );
    assert!(h.speak(2_000).await.is_err());
    assert_eq!(h.session.state(), SessionState::Error);
    let recovered = h.session.resume().await.expect("recover");
    assert_eq!(recovered.state, SessionState::Listening);
}

#[tokio::test]
async fn recovery_from_llm_and_tts_failures_preserves_transcript_and_continues_session() {
    let llm_err = SessionError::new(SessionErrorKind::AiInferenceFailed, "LLM error", true, None);
    let ok_reply = TutorReply::new_validated("Great job! Keep going.", None, None).expect("r");
    let ai = Arc::new(MockAiProvider::new(
        ProviderHealth::Available,
        vec![Err(llm_err), Ok(ok_reply)],
    ));
    let tts = Arc::new(MockTtsProvider::new(ProviderHealth::Unavailable, vec![]));
    let mut h = setup_harness(Arc::new(MockSttProvider::default()), ai, tts).await;
    h.start(1_000).await.expect("start");
    assert!(h.speak(2_000).await.is_err());
    assert_eq!(h.session.state(), SessionState::Error);
    h.session.resume().await.expect("recover");
    let turn2 = h.speak(3_000).await.expect("t2");
    assert_eq!(turn2.final_state, SessionState::Listening);
    let pb = turn2.playback_event.expect("playback event");
    assert!(matches!(
        pb.outcome,
        TutorPlaybackOutcome::TextOnlyDegraded { .. }
    ));
    let saved = h.turns().await;
    assert_eq!(saved.len(), 3);
    assert!(!saved[0].text.trim().is_empty());
}
