//! Tests for STT hallucination filtering, direct utterance transcription, and session STT states.

use crate::ai::mock::MockAiProvider;
use crate::domain::{ProviderHealth, SessionId, SessionState, Transcript};
use crate::modules::conversation::manager::{SessionManager, SessionPipelineAdapters};
use crate::modules::conversation::StartSpeakingSessionConfig;
use crate::modules::speech::{
    audio_capture::{AudioInputDeviceInfo, MockAudioInput},
    playback::MockAudioOutput,
    transcription::{is_noise_or_hallucination, transcribe_completed_utterance},
    utterance::CompletedUtterance,
};
use crate::stt::mock::MockSttProvider;
use crate::tts::mock::MockTtsProvider;
use std::sync::Arc;

#[test]
fn noise_and_hallucination_filter_detects_whisper_artifacts() {
    for artifact in [
        "",
        "   ...  ",
        "!??",
        "[music]",
        "[silence]",
        "thanks for watching!",
    ] {
        assert!(is_noise_or_hallucination(artifact));
    }
    assert!(!is_noise_or_hallucination("Hello, how are you today?"));
    assert!(!is_noise_or_hallucination("I went to the store."));
}

#[tokio::test]
async fn transcribe_completed_utterance_passes_long_utterance_directly_and_flags_noise() {
    let mock_stt = MockSttProvider::new(
        ProviderHealth::Available,
        vec![
            Ok(Transcript::new("[applause]", "en", 400, Some(0.4)).unwrap()),
            Ok(Transcript::new("Twelve second utterance.", "en", 12_000, Some(0.96)).unwrap()),
        ],
    );
    let noise_utt = CompletedUtterance::try_new(vec![0.01; 1_600], 100, false).unwrap();
    assert!(
        transcribe_completed_utterance(&mock_stt, &noise_utt)
            .await
            .unwrap()
            .is_empty_or_hallucinated
    );
    let long_utt = CompletedUtterance::try_new(vec![0.1; 16_000 * 12], 12_000, false).unwrap();
    let event2 = transcribe_completed_utterance(&mock_stt, &long_utt)
        .await
        .unwrap();
    assert!(!event2.is_empty_or_hallucinated);
    assert_eq!(event2.transcript.text, "Twelve second utterance.");
    assert_eq!(
        mock_stt.recorded_utterance_stats(),
        vec![(1_600, 100), (192_000, 12_000)]
    );
}

fn make_stt_session(stt: Arc<MockSttProvider>) -> SessionManager {
    let dev = AudioInputDeviceInfo {
        id: "mic-1".into(),
        name: "Default Mic".into(),
        is_default: true,
    };
    let adapters = SessionPipelineAdapters {
        audio_input: Arc::new(MockAudioInput::new(vec![dev], vec![])),
        stt,
        ai: Arc::new(MockAiProvider::default()),
        tts: Arc::new(MockTtsProvider::default()),
        audio_output: Arc::new(MockAudioOutput::new()),
    };
    SessionManager::new(SessionId::new("sess-t026").unwrap(), adapters)
}

#[tokio::test]
async fn session_manager_transitions_to_listening_on_noise_and_thinking_on_valid_speech() {
    let mock_stt = Arc::new(MockSttProvider::new(
        ProviderHealth::Available,
        vec![
            Ok(Transcript::new("[music]", "en", 500, Some(0.3)).unwrap()),
            Ok(Transcript::new("Practice ordering food?", "en", 1500, Some(0.95)).unwrap()),
        ],
    ));
    let mut mgr = make_stt_session(mock_stt);
    let cfg = StartSpeakingSessionConfig {
        input_device_id: Some("mic-1".into()),
        ..Default::default()
    };
    mgr.start(&cfg).await.unwrap();
    let utt = CompletedUtterance::try_new(vec![0.05; 8_000], 500, false).unwrap();
    assert!(
        mgr.transcribe_utterance(&utt)
            .await
            .unwrap()
            .is_empty_or_hallucinated
    );
    assert_eq!(mgr.state(), SessionState::Listening);
    assert!(
        !mgr.transcribe_utterance(&utt)
            .await
            .unwrap()
            .is_empty_or_hallucinated
    );
    assert_eq!(mgr.state(), SessionState::Thinking);
}
