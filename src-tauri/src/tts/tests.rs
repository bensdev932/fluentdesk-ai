//! Tests for TTS config validation, cancellation, `SessionManager` TTS lifecycle,
//! and opt-in real-binary synthesis.

use crate::ai::mock::MockAiProvider;
use crate::database::{connection::init_sqlite_pool, repository::SessionRepository};
use crate::domain::*;
use crate::modules::conversation::{
    cancellation::SessionCancellationToken,
    context::MaxContextTurns,
    manager::{SessionManager, SessionPipelineAdapters},
    StartSpeakingSessionConfig,
};
use crate::modules::speech::{
    audio_capture::MockAudioInput,
    playback::{play_with_cancellation, MockAudioOutput},
    synthesis::{PlaybackLifecycleStage, TutorPlaybackEvent, TutorPlaybackOutcome},
};
use crate::stt::mock::MockSttProvider;
use crate::tts::{
    kokoro::{KokoroModelConfig, LocalKokoroTtsAdapter},
    mock::MockTtsProvider,
    provider::{synthesize_with_cancellation, SpeakingSpeed, TextToSpeech},
};
use std::sync::Arc;

async fn execute_spoken_turn_with_tts(
    tts: Arc<dyn TextToSpeech>,
    user_text: &str,
) -> (SessionManager, SessionRepository, TutorPlaybackEvent) {
    let sid = SessionId::new("sess-tts-test").expect("sid");
    let repo = SessionRepository::new(init_sqlite_pool("sqlite::memory:").await.expect("db"));
    let speed = SpeakingSpeed::try_new(1.0).expect("sp");
    let adapters = SessionPipelineAdapters {
        audio_input: Arc::new(MockAudioInput::default()),
        stt: Arc::new(MockSttProvider::default()),
        ai: Arc::new(MockAiProvider::default()),
        tts,
        audio_output: Arc::new(MockAudioOutput::new()),
    };
    let mut mgr = SessionManager::with_persistence(
        sid,
        MaxContextTurns::try_new(4).expect("m"),
        speed,
        adapters,
        repo.clone(),
    );
    let cfg = StartSpeakingSessionConfig {
        correction_mode: CorrectionMode::Gentle,
        ..Default::default()
    };
    mgr.start_session(&cfg, "u1", 100).await.expect("start");
    mgr.transition_to(SessionState::Processing).expect("proc");
    mgr.transition_to(SessionState::Thinking).expect("think");
    let tx = Transcript::new(user_text, "en", 1000, Some(0.95)).expect("tx");
    let ready = mgr.process_user_transcript(&tx, 200).await.expect("turn");
    let pb = mgr.speak_tutor_reply(&ready, speed).await.expect("speak");
    (mgr, repo, pb)
}

#[test]
fn speaking_speed_and_kokoro_config_validate_inputs() {
    assert!(SpeakingSpeed::try_new(0.3).is_err() && SpeakingSpeed::try_new(2.5).is_err());
    assert_eq!(SpeakingSpeed::try_new(1.0).expect("ok").as_f32(), 1.0);
    assert!(KokoroModelConfig::try_new("", "model.onnx", "af_bella").is_err());
    assert!(KokoroModelConfig::try_new("kokoro.exe", "model.onnx", "   ").is_err());
}

#[tokio::test]
async fn synthesize_and_play_with_cancellation_abort_and_stop_output_immediately() {
    let (tts, out) = (MockTtsProvider::default(), MockAudioOutput::new());
    let mut tok = SessionCancellationToken::new();
    let speed = SpeakingSpeed::try_new(1.0).expect("sp");
    tok.cancel();
    assert_eq!(
        synthesize_with_cancellation(&tts, "Hi", speed, &mut tok)
            .await
            .expect_err("c")
            .kind,
        SessionErrorKind::Cancelled
    );
    let chunk =
        AudioChunk::new_bounded(AudioFormat::WHISPER_16KHZ_MONO_F32, vec![0.1; 80], 0, 5).unwrap();
    assert_eq!(
        play_with_cancellation(&out, &chunk, &mut tok)
            .await
            .expect_err("c")
            .kind,
        SessionErrorKind::Cancelled
    );
    assert!(out.stopped_count() >= 1);
}

#[tokio::test]
async fn session_manager_speaks_tutor_reply_and_returns_to_listening_when_active() {
    let (mgr, _repo, pb) =
        execute_spoken_turn_with_tts(Arc::new(MockTtsProvider::default()), "I went hiking.").await;
    assert_eq!(mgr.state(), SessionState::Listening);
    assert!(pb
        .stages
        .contains(&PlaybackLifecycleStage::PlaybackCompleted));
}

#[tokio::test]
async fn tts_failure_preserves_visible_tutor_text_and_persisted_transcript() {
    let failing = Arc::new(MockTtsProvider::new(ProviderHealth::Unavailable, vec![]));
    let (mgr, repo, pb) = execute_spoken_turn_with_tts(failing, "Practice travel phrases?").await;
    assert!(matches!(
        pb.outcome,
        TutorPlaybackOutcome::TextOnlyDegraded { .. }
    ));
    assert!(!pb.reply.spoken_reply.is_empty() && mgr.state() == SessionState::Listening);
    assert_eq!(
        repo.list_turns_chronological(mgr.session_id())
            .await
            .expect("t")
            .len(),
        2
    );
}

#[tokio::test]
#[ignore = "Requires local TTS binary (FLUENTDESK_TTS_BIN) and model (FLUENTDESK_TTS_MODEL)"]
async fn real_local_tts_synthesizes_audio_when_configured() {
    let bin = std::env::var("FLUENTDESK_TTS_BIN").expect("b");
    let model = std::env::var("FLUENTDESK_TTS_MODEL").expect("m");
    let cfg = KokoroModelConfig::try_new(bin, model, "af_bella").expect("cfg");
    let adapter = LocalKokoroTtsAdapter::new(cfg);
    let chunk = adapter.synthesize("Hello!", 1.0).await.expect("s");
    assert!(!chunk.samples_f32.is_empty());
}
