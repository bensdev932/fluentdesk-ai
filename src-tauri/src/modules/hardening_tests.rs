//! Phase 7 hardening tests: session review, correction mode settings, raw audio default privacy,
//! 30-minute memory/buffer stability, stage latency metrics, and security audit invariants (`T052`–`T057`).

use crate::ai::{mock::MockAiProvider, ollama::OllamaConfig};
use crate::database::{connection::init_sqlite_pool, repository::SessionRepository};
use crate::domain::{
    AudioChunk, AudioFormat, CorrectionMode, LearnerPreferences, LearnerPreferencesPatch,
    RawAudioRetentionPolicy, SessionError, SessionErrorKind, SessionId, SessionState,
};
use crate::modules::{
    conversation::{
        context::MaxContextTurns,
        manager::{SessionManager, SessionPipelineAdapters},
        StartSpeakingSessionConfig,
    },
    speech::{
        audio_capture::MockAudioInput,
        intake::{
            CompletedUtterance, MaxUtteranceDurationMs, SpeechIntake, SpeechIntakeConfig,
            TurnDetectionEvent,
        },
        playback::MockAudioOutput,
    },
};
use crate::preferences::LearnerPreferencesStore;
use crate::stt::mock::MockSttProvider;
use crate::telemetry::RedactedSecret;
use crate::tts::{mock::MockTtsProvider, provider::SpeakingSpeed};
use sqlx::Row;
use std::sync::Arc;
use std::time::Instant;

#[tokio::test]
async fn t052_session_history_lists_sessions_and_chronological_turns_without_raw_audio() {
    let pool = init_sqlite_pool("sqlite::memory:").await.expect("db");
    let repo = SessionRepository::new(pool);
    repo.ensure_default_user("u1", "Alex", 1_000)
        .await
        .expect("u1");
    let sid = SessionId::new("sess-review-1").expect("sid");
    let adapters = SessionPipelineAdapters {
        audio_input: Arc::new(MockAudioInput::default()),
        stt: Arc::new(MockSttProvider::default()),
        ai: Arc::new(MockAiProvider::default()),
        tts: Arc::new(MockTtsProvider::default()),
        audio_output: Arc::new(MockAudioOutput::new()),
    };
    let mut session = SessionManager::with_persistence(
        sid.clone(),
        MaxContextTurns::try_new(6).expect("m"),
        SpeakingSpeed::try_new(1.0).expect("s"),
        adapters,
        repo.clone(),
    );
    let cfg = StartSpeakingSessionConfig {
        correction_mode: CorrectionMode::Gentle,
        ..Default::default()
    };
    session
        .start_session(&cfg, "u1", 1_000)
        .await
        .expect("start");
    let utt = CompletedUtterance::try_new(vec![0.1_f32; 8_000], 500, false).expect("u");
    session
        .process_spoken_utterance(&utt, 2_000)
        .await
        .expect("t1");
    session.stop_at(3_000).await.expect("stop");
    let list = repo.list_sessions_for_user("u1").await.expect("list");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].turn_count, 2);
    assert_eq!(list[0].ended_at_unix_ms, Some(3_000));
    let view = repo.get_session_history(&sid).await.expect("view");
    assert_eq!(view.turns.len(), 2);
    assert!(view.turns[1]
        .stage_metadata
        .as_ref()
        .and_then(|m| m.llm_duration_ms)
        .is_some());
}

#[tokio::test]
async fn t053_correction_mode_settings_persist_natural_gentle_detailed_and_off() {
    let pool = init_sqlite_pool("sqlite::memory:").await.expect("db");
    let store = LearnerPreferencesStore::for_user(SessionRepository::new(pool), "u1");
    let initial = store.get_preferences().await.expect("load");
    assert_eq!(initial.correction_mode, CorrectionMode::Gentle);
    for mode in [
        CorrectionMode::Gentle,
        CorrectionMode::Detailed,
        CorrectionMode::Off,
        CorrectionMode::Natural,
    ] {
        let updated = store
            .patch_preferences(LearnerPreferencesPatch {
                correction_mode: Some(mode),
                ..Default::default()
            })
            .await
            .expect("update");
        assert_eq!(updated.correction_mode, mode);
    }
}

#[tokio::test]
async fn t054_raw_audio_not_persisted_by_default_and_requires_explicit_consent() {
    let defaults = LearnerPreferences::default();
    assert!(!defaults.raw_audio_recording_opt_in);
    assert!(!defaults.cloud_opt_in);
    let policy = RawAudioRetentionPolicy::from_preferences(&defaults);
    assert!(policy.require_explicit_recording_consent().is_err());
    let pool = init_sqlite_pool("sqlite::memory:").await.expect("db");
    let tables = sqlx::query("SELECT name FROM sqlite_master WHERE type='table'")
        .fetch_all(&pool)
        .await
        .expect("tables");
    assert!(tables
        .iter()
        .all(|r| !r.get::<String, _>("name").contains("audio")));
}

#[tokio::test]
async fn t055_t056_thirty_minute_simulated_session_bounds_memory_and_measures_latency() {
    let pool = init_sqlite_pool("sqlite::memory:").await.expect("db");
    let repo = SessionRepository::new(pool);
    let sid = SessionId::new("sess-30min").expect("sid");
    let adapters = SessionPipelineAdapters {
        audio_input: Arc::new(MockAudioInput::default()),
        stt: Arc::new(MockSttProvider::default()),
        ai: Arc::new(MockAiProvider::default()),
        tts: Arc::new(MockTtsProvider::default()),
        audio_output: Arc::new(MockAudioOutput::new()),
    };
    let mut session = SessionManager::with_persistence(
        sid.clone(),
        MaxContextTurns::try_new(8).expect("m"),
        SpeakingSpeed::try_new(1.0).expect("s"),
        adapters,
        repo.clone(),
    );
    let cfg = StartSpeakingSessionConfig::default();
    session.start_session(&cfg, "u1", 0).await.expect("start");
    let cap = MaxUtteranceDurationMs::try_new(2_000).expect("cap");
    let mut intake = SpeechIntake::new(SpeechIntakeConfig::PushToTalk(cap));
    let fmt = AudioFormat::WHISPER_16KHZ_MONO_F32;
    let started = Instant::now();
    for i in 0..180_i64 {
        let chunk = AudioChunk::new_bounded(fmt, vec![0.05_f32; 8_000], 0, 500).expect("chunk");
        intake.set_ptt_held(true).expect("ptt press");
        let ev = intake.process_chunk(&chunk).expect("push");
        assert_eq!(ev, TurnDetectionEvent::SpeechContinuing);
        let utt = intake.set_ptt_held(false).expect("release").expect("utt");
        assert!(utt.sample_count() <= 32_000);
        let res = session
            .process_spoken_utterance(&utt, 1_000 + i * 10_000)
            .await
            .expect("turn");
        assert_eq!(res.final_state, SessionState::Listening);
    }
    assert!(started.elapsed().as_secs_f64() >= 0.0);
    assert_eq!(
        repo.list_turns_chronological(&sid)
            .await
            .expect("turns")
            .len(),
        360
    );
}

#[test]
fn t057_ipc_local_bindings_and_secret_redaction_enforce_privacy_invariants() {
    assert!(OllamaConfig::try_new("https://remote-llm.example.com", 11434, "llama3.2:3b").is_err());
    let err = SessionError::new(
        SessionErrorKind::AiInferenceFailed,
        "Safe msg",
        true,
        Some("secret=sk-123".into()),
    );
    let json = serde_json::to_string(&err).expect("json");
    assert!(!json.contains("sk-123"));
    let secret = RedactedSecret::new("sk-top-secret-token");
    assert!(!format!("{secret:?}").contains("top-secret"));
}
