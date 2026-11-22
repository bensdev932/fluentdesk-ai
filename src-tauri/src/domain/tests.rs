//! Unit tests for shared domain models (`T003`).

use crate::domain::*;

#[test]
fn session_id_rejects_blank_and_accepts_valid_identifiers() {
    assert_eq!(
        SessionId::new("   "),
        Err(DomainValidationError::EmptySessionId)
    );
    let id = SessionId::new("session-001").expect("valid session id");
    assert_eq!(id.as_str(), "session-001");
}

#[test]
fn session_state_reports_microphone_and_active_status_truthfully() {
    assert!(SessionState::Listening.is_microphone_capturing());
    for state in [
        SessionState::Idle,
        SessionState::Initializing,
        SessionState::Processing,
        SessionState::Thinking,
        SessionState::Speaking,
        SessionState::Paused,
        SessionState::Error,
        SessionState::Ended,
    ] {
        assert!(
            !state.is_microphone_capturing(),
            "State {state:?} must not report active microphone capture"
        );
    }

    assert!(!SessionState::Idle.is_active());
    assert!(!SessionState::Ended.is_active());
    assert!(SessionState::Listening.is_active());
    assert!(SessionState::Thinking.is_active());
    assert!(SessionState::Paused.is_active());
}

#[test]
fn audio_format_and_chunk_enforce_bounds() {
    let valid_fmt = AudioFormat::WHISPER_16KHZ_MONO_F32;
    assert!(valid_fmt.validate().is_ok());

    let zero_rate = AudioFormat {
        sample_rate_hz: 0,
        channels: 1,
        sample_format: SampleRepresentation::PcmF32,
    };
    assert!(matches!(
        zero_rate.validate(),
        Err(DomainValidationError::InvalidAudioFormat { .. })
    ));

    let valid_chunk =
        AudioChunk::new_bounded(valid_fmt, vec![0.1, -0.1, 0.2], 0, 20).expect("within bounds");
    assert_eq!(valid_chunk.samples_f32.len(), 3);

    let oversized = vec![0.0_f32; MAX_AUDIO_CHUNK_SAMPLES + 1];
    assert_eq!(
        AudioChunk::new_bounded(valid_fmt, oversized, 0, 5001),
        Err(DomainValidationError::AudioChunkExceedsBound {
            sample_count: MAX_AUDIO_CHUNK_SAMPLES + 1,
            max_samples: MAX_AUDIO_CHUNK_SAMPLES,
        })
    );
}

#[test]
fn transcript_validates_confidence_and_trims_whitespace() {
    let t = Transcript::new("  Hello world  ", "en", 1200, Some(0.95)).expect("valid");
    assert_eq!(t.text, "Hello world");
    assert!(!t.is_empty_or_whitespace());

    let bad_conf = Transcript::new("Hello", "en", 500, Some(1.5));
    assert!(matches!(
        bad_conf,
        Err(DomainValidationError::InvalidConfidence(_))
    ));
}

#[test]
fn tutor_reply_rejects_empty_spoken_reply_and_sanitizes_control_chars() {
    assert_eq!(
        TutorReply::new_validated("   \u{0000} ", None, None),
        Err(DomainValidationError::EmptyTutorReply)
    );

    let reply = TutorReply::new_validated(
        "That sounds fun!\u{0007} Where did you go?",
        Some("  Try saying: 'I went to the park.'  ".to_string()),
        Some("What did you see there?".to_string()),
    )
    .expect("valid reply");

    assert_eq!(reply.spoken_reply, "That sounds fun! Where did you go?");
    assert_eq!(
        reply.correction.as_deref(),
        Some("Try saying: 'I went to the park.'")
    );
}

#[test]
fn session_error_omits_internal_details_when_serialized_for_ipc() {
    let err = SessionError::new(
        SessionErrorKind::AiProviderUnavailable,
        "Local AI tutor service is not running. Please start Ollama and try again.",
        true,
        Some("Connection refused on 127.0.0.1:11434 (os error 10061)".to_string()),
    );
    let json = serde_json::to_string(&err).expect("serialize error");
    assert!(json.contains("AiProviderUnavailable"));
    assert!(!json.contains("10061"));
}
