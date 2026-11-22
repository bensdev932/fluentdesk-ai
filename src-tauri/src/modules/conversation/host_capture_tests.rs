//! Integration tests for `SpeakingSessionHost` device validation, microphone switching,
//! and live VAD capture pumping with Whisper noise suppression.

use crate::ai::mock::MockAiProvider;
use crate::database::{connection::init_sqlite_pool, repository::SessionRepository};
use crate::domain::{
    AudioChunk, AudioFormat, LearnerPreferencesPatch, ProviderHealth, SessionErrorKind, SessionId,
    SessionState, Transcript,
};
use crate::modules::conversation::{
    host::SpeakingSessionHost, manager::SessionPipelineAdapters, StartSpeakingSessionConfig,
};
use crate::modules::speech::{
    audio_capture::{AudioInputDeviceInfo, LiveAudioFeeder, MockAudioInput},
    playback::MockAudioOutput,
    utterance::CompletedUtterance,
};
use crate::preferences::LearnerPreferencesStore;
use crate::stt::mock::MockSttProvider;
use crate::tts::mock::MockTtsProvider;
use std::sync::Arc;

pub(super) fn mic(id: &str, is_default: bool) -> AudioInputDeviceInfo {
    AudioInputDeviceInfo {
        id: id.to_string(),
        name: format!("Mic {id}"),
        is_default,
    }
}

pub(super) fn chunk_16k(amp: f32, samples: usize, ts: u64, ms: u32) -> AudioChunk {
    AudioChunk::new_bounded(
        AudioFormat::WHISPER_16KHZ_MONO_F32,
        vec![amp; samples],
        ts,
        ms,
    )
    .expect("valid chunk")
}

async fn turn_count(repo: &SessionRepository, sid: &SessionId) -> usize {
    repo.list_turns_chronological(sid).await.unwrap().len()
}

pub(super) async fn make_live_host(
    devices: Vec<AudioInputDeviceInfo>,
    stt: Arc<MockSttProvider>,
) -> (
    Arc<SpeakingSessionHost>,
    Arc<MockAudioInput>,
    LiveAudioFeeder,
    SessionRepository,
) {
    let repo = SessionRepository::new(init_sqlite_pool("sqlite::memory:").await.unwrap());
    let (mock_in, feeder) = MockAudioInput::new_live(devices);
    let audio_in = Arc::new(mock_in);
    let adapters = SessionPipelineAdapters {
        audio_input: audio_in.clone(),
        stt,
        ai: Arc::new(MockAiProvider::default()),
        tts: Arc::new(MockTtsProvider::default()),
        audio_output: Arc::new(MockAudioOutput::new()),
    };
    let host = Arc::new(SpeakingSessionHost::new(adapters, Some(repo.clone())));
    (host, audio_in, feeder, repo)
}

#[tokio::test]
async fn host_validates_devices_on_start_and_switches_active_microphone() {
    let devs = vec![mic("mic-1", true), mic("mic-2", false)];
    let (host, audio_in, _feeder, repo) =
        make_live_host(devs.clone(), Arc::new(MockSttProvider::default())).await;
    assert_eq!(host.list_input_devices().await.unwrap(), devs);

    let bad_cfg = StartSpeakingSessionConfig {
        input_device_id: Some("missing-mic".into()),
        ..Default::default()
    };
    let start_err = host.start_session(bad_cfg).await.unwrap_err();
    assert_eq!(start_err.kind, SessionErrorKind::AudioDeviceUnavailable);
    assert!(repo
        .list_sessions_for_user("default-user")
        .await
        .unwrap()
        .is_empty());

    let ok_cfg = StartSpeakingSessionConfig {
        input_device_id: Some("mic-1".into()),
        ..Default::default()
    };
    let snap = host.start_session(ok_cfg).await.unwrap();
    assert_eq!(snap.state, SessionState::Listening);

    let switched = host
        .set_input_device(&snap.session_id, " mic-2 ")
        .await
        .unwrap();
    assert_eq!(switched.as_str(), "mic-2");
    let switch_err = host
        .set_input_device(&snap.session_id, "bad")
        .await
        .unwrap_err();
    assert_eq!(switch_err.kind, SessionErrorKind::AudioDeviceUnavailable);
    assert_eq!(
        audio_in.recorded_start_devices(),
        vec![Some("mic-1".into()), Some("mic-2".into())]
    );
}

#[tokio::test]
async fn host_pumps_vad_audio_filters_whisper_noise_to_listening_and_runs_valid_turn() {
    let stt = Arc::new(MockSttProvider::new(
        ProviderHealth::Available,
        vec![
            Ok(Transcript::new("[silence]", "en", 1_100, Some(0.2)).unwrap()),
            Ok(Transcript::new("I like rainy mornings.", "en", 1_100, Some(0.95)).unwrap()),
        ],
    ));
    let (host, _audio_in, feeder, repo) = make_live_host(vec![], stt.clone()).await;
    let snap = host
        .start_session(StartSpeakingSessionConfig::default())
        .await
        .unwrap();

    // First utterance is Whisper noise -> returns straight to Listening, 0 turns saved.
    assert!(feeder.push(chunk_16k(0.2, 4_800, 0, 300)).await);
    assert!(feeder.push(chunk_16k(0.0, 17_600, 300, 1_100)).await);
    let r1 = host
        .pump_capture_turn(&snap.session_id, 1_000)
        .await
        .unwrap()
        .unwrap();
    assert!(r1.transcript_event.is_empty_or_hallucinated && r1.tutor_text_event.is_none());
    assert_eq!(r1.final_state, SessionState::Listening);
    assert_eq!(turn_count(&repo, &snap.session_id).await, 0);

    // Second utterance is real learner speech -> runs full STT -> AI -> TTS and persists 2 turns.
    assert!(feeder.push(chunk_16k(0.2, 4_800, 1_400, 300)).await);
    assert!(feeder.push(chunk_16k(0.0, 17_600, 1_700, 1_100)).await);
    let r2 = host
        .pump_capture_turn(&snap.session_id, 2_000)
        .await
        .unwrap()
        .unwrap();
    assert!(!r2.transcript_event.is_empty_or_hallucinated && r2.tutor_text_event.is_some());
    assert_eq!(r2.final_state, SessionState::Listening);
    assert_eq!(stt.recorded_utterance_stats().len(), 2);
    assert_eq!(turn_count(&repo, &snap.session_id).await, 2);
}

#[tokio::test]
async fn host_applies_persisted_speaking_speed_from_sqlite_preferences_to_tts() {
    let repo = SessionRepository::new(init_sqlite_pool("sqlite::memory:").await.unwrap());
    let store = LearnerPreferencesStore::new(repo.clone());
    store
        .patch_preferences(LearnerPreferencesPatch {
            speaking_speed: Some(1.45),
            ..Default::default()
        })
        .await
        .unwrap();
    let tts = Arc::new(MockTtsProvider::default());
    let adapters = SessionPipelineAdapters {
        audio_input: Arc::new(MockAudioInput::default()),
        stt: Arc::new(MockSttProvider::default()),
        ai: Arc::new(MockAiProvider::default()),
        tts: tts.clone(),
        audio_output: Arc::new(MockAudioOutput::new()),
    };
    let host = SpeakingSessionHost::new(adapters, Some(repo));
    let snap = host
        .start_session(StartSpeakingSessionConfig::default())
        .await
        .unwrap();
    let utt = CompletedUtterance::try_new(vec![0.1_f32; 8_000], 500, false).unwrap();
    host.process_spoken_utterance(&snap.session_id, &utt, 1_000)
        .await
        .unwrap();
    let calls = tts.synthesized_calls();
    assert!(!calls.is_empty());
    assert!(calls.iter().all(|c| (c.1 - 1.45).abs() < 1e-6));
}
