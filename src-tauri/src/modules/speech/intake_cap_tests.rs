//! Public-seam tests for `SpeechIntake` max-duration capping, direct STT handoff,
//! noise suppression, invalid format handling, and stop-time flushing.

use crate::domain::{
    AudioChunk, AudioFormat, ProviderHealth, SampleRepresentation, SessionErrorKind, Transcript,
};
use crate::modules::speech::{
    intake::{
        CompletedUtterance, MaxUtteranceDurationMs, SpeechIntake, SpeechIntakeConfig,
        TurnDetectionEvent, VadEndpointConfig,
    },
    intake_tests::feed,
};
use crate::stt::mock::MockSttProvider;

#[test]
fn single_max_duration_cap_bounds_and_resets_both_vad_and_ptt() {
    let vad_cfg = VadEndpointConfig::try_new(0.03, 200, 100, 500).unwrap();
    let mut vad = SpeechIntake::new(SpeechIntakeConfig::VoiceActivity(vad_cfg));
    assert_eq!(
        feed(&mut vad, 0.2, 4_800, 0, 300),
        TurnDetectionEvent::SpeechContinuing
    );
    let TurnDetectionEvent::TurnCompleted(vad_utt) = feed(&mut vad, 0.2, 4_800, 300, 300) else {
        panic!("expected VAD cap completion");
    };
    assert_eq!(
        (vad_utt.sample_count(), vad_utt.is_truncated()),
        (8_000, true)
    );

    let ptt_cap = MaxUtteranceDurationMs::try_new(500).unwrap();
    let mut ptt = SpeechIntake::new(SpeechIntakeConfig::PushToTalk(ptt_cap));
    ptt.set_ptt_held(true).unwrap();
    let TurnDetectionEvent::TurnCompleted(ptt_utt) = feed(&mut ptt, 0.1, 9_600, 0, 600) else {
        panic!("expected PTT cap completion");
    };
    assert_eq!(
        (ptt_utt.sample_count(), ptt_utt.is_truncated()),
        (8_000, true)
    );
    assert!(!ptt.is_ptt_held());
}

#[tokio::test]
async fn speech_intake_transcribes_completed_utterance_and_filters_noise() {
    let stt = MockSttProvider::new(
        ProviderHealth::Available,
        vec![
            Ok(Transcript::new("[music]", "en", 1_000, Some(0.2)).unwrap()),
            Ok(Transcript::new("How is the weather?", "en", 1_000, Some(0.96)).unwrap()),
        ],
    );
    let intake = SpeechIntake::for_session(false);
    let utt = CompletedUtterance::try_new(vec![0.1; 16_000], 1_000, false).unwrap();

    assert!(
        intake
            .transcribe(&stt, &utt)
            .await
            .unwrap()
            .is_empty_or_hallucinated
    );
    assert!(
        !intake
            .transcribe(&stt, &utt)
            .await
            .unwrap()
            .is_empty_or_hallucinated
    );
    assert_eq!(stt.recorded_utterance_stats(), vec![(16_000, 1_000); 2]);
}

#[test]
fn invalid_format_errors_without_corrupting_state_and_flush_on_stop_honors_threshold() {
    let mut intake = SpeechIntake::for_session(false);
    let bad = AudioChunk {
        format: AudioFormat {
            sample_rate_hz: 1_000,
            channels: 1,
            sample_format: SampleRepresentation::PcmF32,
        },
        samples_f32: vec![0.2; 100],
        start_timestamp_ms: 0,
        duration_ms: 100,
    };
    assert_eq!(
        intake.process_chunk(&bad).unwrap_err().kind,
        SessionErrorKind::AudioDeviceUnavailable
    );
    assert!(intake.flush_on_stop().is_none());

    assert_eq!(
        feed(&mut intake, 0.2, 4_000, 0, 250),
        TurnDetectionEvent::SpeechContinuing
    );
    let flushed = intake.flush_on_stop().expect("flushed speech");
    assert_eq!(flushed.sample_count(), 4_000);
    assert!(intake.flush_on_stop().is_none());
}

#[test]
fn cefr_vad_scales_hangover_prepends_200ms_preroll_and_trims_trailing_silence() {
    let mut a2 = SpeechIntake::for_session_with_cefr(false, crate::domain::CefrLevel::A2);
    assert_eq!(
        feed(&mut a2, 0.0, 3_200, 0, 200),
        TurnDetectionEvent::Silence
    );
    assert_eq!(
        feed(&mut a2, 0.2, 4_800, 200, 300),
        TurnDetectionEvent::SpeechContinuing
    );
    // 1050ms pause does NOT prematurely endpoint an A2 learner (requires 1400ms).
    assert_eq!(
        feed(&mut a2, 0.0, 16_800, 500, 1_050),
        TurnDetectionEvent::SpeechContinuing
    );
    let TurnDetectionEvent::TurnCompleted(utt) = feed(&mut a2, 0.0, 5_600, 1_550, 350) else {
        panic!("expected A2 TurnCompleted at 1400ms");
    };
    // 200ms pre-roll (3200) + 300ms voiced (4800) + <= 200ms trimmed tail (3200) = 11_200 samples (700ms).
    assert_eq!((utt.sample_count(), utt.duration_ms()), (11_200, 700));
}
