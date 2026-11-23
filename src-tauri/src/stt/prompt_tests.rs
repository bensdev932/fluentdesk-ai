//! TDD tests for `SttPromptContext` keyterm biasing, `--prompt` CLI args, and host wiring.

use crate::ai::mock::MockAiProvider;
use crate::domain::{
    ConversationRole, ConversationTurn, ProviderHealth, SessionError, SessionId, TutorReply,
};
use crate::modules::conversation::{
    manager::SessionPipelineAdapters, SpeakingSessionHost, StartSpeakingSessionConfig,
};
use crate::modules::speech::{
    audio_capture::{AudioInputDeviceInfo, MockAudioInput},
    playback::MockAudioOutput,
    utterance::CompletedUtterance,
};
use crate::stt::{
    mock::MockSttProvider,
    provider::{SpeechToText, SttPromptContext},
    sidecar::{WhisperProcessOutput, WhisperProcessRunner},
    whisper::{LocalWhisperAdapter, WhisperModelConfig},
};
use crate::tts::mock::MockTtsProvider;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

struct RecordingRunner {
    calls: Mutex<Vec<Vec<String>>>,
}

impl RecordingRunner {
    fn new() -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
        }
    }
    fn calls(&self) -> Vec<Vec<String>> {
        self.calls.lock().expect("lock args").clone()
    }
}

impl WhisperProcessRunner for RecordingRunner {
    fn run(&self, _bin: &Path, args: &[String]) -> Result<WhisperProcessOutput, SessionError> {
        self.calls.lock().expect("lock args").push(args.to_vec());
        Ok(WhisperProcessOutput {
            success: true,
            stdout: b"espresso please".to_vec(),
        })
    }
}

fn temp_model_file(tag: &str) -> (PathBuf, TempFileGuard) {
    let path = std::env::temp_dir().join(format!("fluentdesk_{tag}_{}.bin", std::process::id()));
    std::fs::write(&path, b"dummy").expect("write dummy model");
    (path.clone(), TempFileGuard(path))
}
struct TempFileGuard(PathBuf);
impl Drop for TempFileGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn tutor_turn(id: &str, text: &str, follow_up: Option<&str>) -> ConversationTurn {
    ConversationTurn {
        id: id.into(),
        session_id: SessionId::new("sess-prompt").unwrap(),
        role: ConversationRole::Tutor,
        text: text.into(),
        correction: None,
        proper_way: None,
        why: None,
        follow_up_prompt: follow_up.map(str::to_string),
        timestamp_unix_ms: 10,
        stage_metadata: None,
    }
}

#[test]
fn prompt_context_sanitizes_dedups_and_bounds_to_224_chars() {
    let empty = SttPromptContext::new(Some("  \r\n\t "), Vec::<String>::new());
    assert!(empty.is_empty());
    assert_eq!(empty.format_whisper_prompt(), None);

    let terms = ["boarding\r\npass", "Boarding Pass", "itinerary\x00"]
        .into_iter()
        .map(str::to_string)
        .chain((0..16).map(|i| format!("longvocabularytoken{i:02}")));
    let noisy = SttPromptContext::new(Some("Airport\nCheck-in\tCounter"), terms);
    let formatted = noisy.format_whisper_prompt().expect("non-empty prompt");
    assert!(formatted.chars().count() <= 224);
    assert!(!formatted.chars().any(char::is_control));
    assert!(formatted.contains("Airport Check-in Counter"));
    assert_eq!(formatted.to_lowercase().matches("boarding pass").count(), 1);
}

#[test]
fn prompt_context_from_session_prioritizes_vocab_and_recent_tutor_keywords() {
    let turns = vec![tutor_turn(
        "t-1",
        "Have you packed your passport and itinerary yet?",
        Some("Which terminal is your departure?"),
    )];
    let ctx = SttPromptContext::from_session(
        "Airport Check-in",
        &["espresso".into(), "croissant".into()],
        &turns,
    );
    let prompt = ctx.format_whisper_prompt().expect("prompt");
    assert!(prompt.contains("Airport Check-in"));
    assert!(prompt.contains("espresso"));
    assert!(prompt.contains("croissant"));
    assert!(prompt.contains("passport") || prompt.contains("itinerary"));
}

#[tokio::test]
async fn local_whisper_adapter_passes_prompt_flag_only_when_non_empty() {
    let (model_path, _guard) = temp_model_file("whisper_prompt");
    let cfg = WhisperModelConfig::try_new(&model_path, "en", 2).unwrap();
    let runner = Arc::new(RecordingRunner::new());
    let exe = std::env::current_exe().unwrap();
    let adapter = LocalWhisperAdapter::new_with_runner(&exe, cfg, runner.clone());
    let utt = CompletedUtterance::try_new(vec![0.05; 320], 20, false).unwrap();

    let biased = SttPromptContext::new(Some("Coffee Ordering"), vec!["macchiato".into()]);
    adapter
        .transcribe_with_context(&utt, Some(&biased))
        .await
        .unwrap();
    let blank = SttPromptContext::new(None, Vec::<String>::new());
    adapter
        .transcribe_with_context(&utt, Some(&blank))
        .await
        .unwrap();

    let calls = runner.calls();
    let p_idx = calls[0].iter().position(|a| a == "--prompt").unwrap();
    assert!(calls[0][p_idx + 1].contains("Coffee Ordering"));
    assert!(calls[0][p_idx + 1].contains("macchiato"));
    assert!(calls[0].contains(&"--no-speech-thold".to_string()));
    assert!(calls[0].contains(&"0.60".to_string()));
    assert!(calls[0].contains(&"--suppress-nst".to_string()));
    assert!(!calls[1].contains(&"--prompt".to_string()));
}

#[tokio::test]
async fn speaking_session_host_wires_topic_vocab_and_tutor_history_into_stt_prompt() {
    let dev = AudioInputDeviceInfo {
        id: "mic-1".into(),
        name: "Default".into(),
        is_default: true,
    };
    let mock_stt = Arc::new(MockSttProvider::default());
    let reply = TutorReply::new_validated(
        "Would you like cinnamon or hazelnut syrup?",
        None,
        Some("Try requesting hazelnut latte.".into()),
    )
    .unwrap();
    let mock_ai = Arc::new(MockAiProvider::new(
        ProviderHealth::Available,
        vec![Ok(reply)],
    ));
    let adapters = SessionPipelineAdapters {
        audio_input: Arc::new(MockAudioInput::new(vec![dev], vec![])),
        stt: mock_stt.clone(),
        ai: mock_ai,
        tts: Arc::new(MockTtsProvider::default()),
        audio_output: Arc::new(MockAudioOutput::new()),
    };
    let host = SpeakingSessionHost::new(adapters, None);
    let snap = host
        .start_session(StartSpeakingSessionConfig {
            input_device_id: Some("mic-1".into()),
            topic: Some("Coffee Shop Ordering".into()),
            target_vocabulary: vec!["espresso".into(), "croissant".into()],
            ..Default::default()
        })
        .await
        .unwrap();
    let utt = CompletedUtterance::try_new(vec![0.1; 1600], 100, false).unwrap();
    host.process_spoken_utterance(&snap.session_id, &utt, 1_000)
        .await
        .unwrap();
    host.process_spoken_utterance(&snap.session_id, &utt, 2_000)
        .await
        .unwrap();

    let recorded = mock_stt.recorded_prompts();
    let p1 = recorded[0].as_deref().unwrap();
    assert!(p1.contains("Coffee Shop Ordering") && p1.contains("espresso"));
    assert!(recorded[1].as_deref().unwrap().contains("hazelnut"));

    use crate::modules::speech::transcription::{is_noise_or_hallucination, is_prompt_echo};
    assert!(is_noise_or_hallucination("thank you thank you thank you"));
    assert!(!is_noise_or_hallucination("No thanks, thank you!"));
    let ctx = SttPromptContext::new(Some("Coffee Shop Ordering"), vec!["oat milk".into()]);
    assert!(is_prompt_echo("Coffee Shop Ordering", &ctx));
    assert!(!is_prompt_echo("Can I get an oat milk latte please", &ctx));
}
