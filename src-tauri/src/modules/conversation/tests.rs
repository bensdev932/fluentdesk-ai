//! Unit tests for valid and invalid session state transitions (`T008`, `T009`).

use crate::ai::mock::MockAiProvider;
use crate::domain::{
    AudioChunk, AudioFormat, CorrectionMode, SessionErrorKind, SessionId, SessionState,
};
use crate::modules::conversation::{
    manager::{SessionManager, SessionPipelineAdapters},
    StartSpeakingSessionConfig,
};
use crate::modules::speech::{
    audio_capture::MockAudioInput, intake::TurnDetectionEvent, playback::MockAudioOutput,
};
use crate::stt::mock::MockSttProvider;
use crate::tts::mock::MockTtsProvider;
use std::sync::Arc;

fn make_test_manager() -> SessionManager {
    let adapters = SessionPipelineAdapters {
        audio_input: Arc::new(MockAudioInput::default()),
        stt: Arc::new(MockSttProvider::default()),
        ai: Arc::new(MockAiProvider::default()),
        tts: Arc::new(MockTtsProvider::default()),
        audio_output: Arc::new(MockAudioOutput::new()),
    };
    SessionManager::new(SessionId::new("sess-t008").unwrap(), adapters)
}

#[test]
fn valid_happy_path_voice_loop_state_transitions_succeed() {
    let mut mgr = make_test_manager();
    assert_eq!(mgr.state(), SessionState::Idle);

    for next in [
        SessionState::Initializing,
        SessionState::Listening,
        SessionState::Processing,
        SessionState::Thinking,
        SessionState::Speaking,
        SessionState::Listening,
        SessionState::Paused,
        SessionState::Listening,
        SessionState::Ended,
        SessionState::Idle,
    ] {
        let actual = mgr.transition_to(next).expect("valid transition");
        assert_eq!(actual, next);
        assert_eq!(mgr.state(), next);
    }
}

#[test]
fn invalid_state_transitions_are_rejected_without_mutating_state() {
    let mut mgr = make_test_manager();

    for invalid_from_idle in [
        SessionState::Listening,
        SessionState::Processing,
        SessionState::Thinking,
        SessionState::Speaking,
        SessionState::Paused,
    ] {
        let err = mgr
            .transition_to(invalid_from_idle)
            .expect_err("Idle cannot jump directly to active stage");
        assert_eq!(err.kind, SessionErrorKind::InvalidStateTransition);
        assert_eq!(mgr.state(), SessionState::Idle);
    }

    mgr.transition_to(SessionState::Initializing).unwrap();
    mgr.transition_to(SessionState::Listening).unwrap();
    mgr.transition_to(SessionState::Paused).unwrap();

    // Paused cannot jump directly to Speaking or Thinking without resuming Listening
    for invalid_from_paused in [SessionState::Thinking, SessionState::Speaking] {
        let err = mgr
            .transition_to(invalid_from_paused)
            .expect_err("Paused cannot jump to Thinking/Speaking");
        assert_eq!(err.kind, SessionErrorKind::InvalidStateTransition);
        assert_eq!(mgr.state(), SessionState::Paused);
    }
}

#[tokio::test]
async fn start_session_honors_push_to_talk_config_and_gentle_correction_default() {
    let default_cfg = StartSpeakingSessionConfig::default();
    assert_eq!(default_cfg.correction_mode, CorrectionMode::Gentle);
    assert!(!default_cfg.push_to_talk);

    let mut vad_mgr = make_test_manager();
    let vad_snap = vad_mgr.start(&default_cfg).await.unwrap();
    assert!(!vad_snap.push_to_talk);
    assert_eq!(vad_snap.correction_mode, CorrectionMode::Gentle);
    let voiced = AudioChunk::new_bounded(
        AudioFormat::WHISPER_16KHZ_MONO_F32,
        vec![0.1; 4_800],
        0,
        300,
    )
    .unwrap();
    assert_eq!(
        vad_mgr.process_captured_audio_chunk(&voiced).unwrap(),
        TurnDetectionEvent::SpeechContinuing
    );

    let mut ptt_mgr = make_test_manager();
    let ptt_cfg = StartSpeakingSessionConfig {
        push_to_talk: true,
        ..Default::default()
    };
    let ptt_snap = ptt_mgr.start(&ptt_cfg).await.unwrap();
    assert!(ptt_snap.push_to_talk);
    assert!(ptt_mgr.is_push_to_talk());
    assert_eq!(
        ptt_mgr.process_captured_audio_chunk(&voiced).unwrap(),
        TurnDetectionEvent::Silence
    );
}
