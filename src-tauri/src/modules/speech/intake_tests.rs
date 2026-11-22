//! Public-seam tests for `SpeechIntake` device validation, energy VAD hangover,
//! and push-to-talk press/release behavior.

use crate::domain::{AudioChunk, AudioFormat, SampleRepresentation, SessionErrorKind};
use crate::modules::speech::{
    audio_capture::{validate_audio_input_device, AudioInputDeviceInfo, MockAudioInput},
    intake::{SpeechIntake, TurnDetectionEvent},
};

pub(super) fn chunk_16k(amp: f32, samples: usize, ts: u64, ms: u32) -> AudioChunk {
    AudioChunk::new_bounded(
        AudioFormat::WHISPER_16KHZ_MONO_F32,
        vec![amp; samples],
        ts,
        ms,
    )
    .expect("valid chunk")
}

pub(super) fn feed(
    intake: &mut SpeechIntake,
    amp: f32,
    samples: usize,
    ts: u64,
    ms: u32,
) -> TurnDetectionEvent {
    intake
        .process_chunk(&chunk_16k(amp, samples, ts, ms))
        .expect("process chunk")
}

#[tokio::test]
async fn device_validation_accepts_listed_mic_and_rejects_blank_or_unknown() {
    let devices = vec![AudioInputDeviceInfo {
        id: "mic-usb".into(),
        name: "USB Mic".into(),
        is_default: true,
    }];
    let audio_in = MockAudioInput::new(devices, vec![]);
    let valid = validate_audio_input_device(&audio_in, "  mic-usb ")
        .await
        .expect("valid mic");
    assert_eq!(valid.as_str(), "mic-usb");

    for bad in ["   ", "mic-missing"] {
        let err = validate_audio_input_device(&audio_in, bad)
            .await
            .unwrap_err();
        assert_eq!(err.kind, SessionErrorKind::AudioDeviceUnavailable);
    }
}

#[test]
fn vad_endpoints_48k_stereo_speech_with_hangover_and_drops_short_spikes() {
    let mut intake = SpeechIntake::for_session(false);
    assert!(!intake.is_push_to_talk());
    assert_eq!(
        feed(&mut intake, 0.0, 320, 0, 20),
        TurnDetectionEvent::Silence
    );

    // Transient 100ms spike (< 200ms min_speech_ms) followed by 800ms silence drops cleanly.
    assert_eq!(
        feed(&mut intake, 0.2, 1_600, 20, 100),
        TurnDetectionEvent::SpeechContinuing
    );
    assert_eq!(
        feed(&mut intake, 0.0, 12_800, 120, 800),
        TurnDetectionEvent::Silence
    );

    // 48 kHz stereo speech (300ms = 28,800 interleaved samples) + 800ms silence -> 16 kHz mono turn.
    let stereo_48k = AudioFormat {
        sample_rate_hz: 48_000,
        channels: 2,
        sample_format: SampleRepresentation::PcmF32,
    };
    let voiced_48k = AudioChunk::new_bounded(stereo_48k, vec![0.15; 28_800], 1_000, 300).unwrap();
    assert_eq!(
        intake.process_chunk(&voiced_48k).unwrap(),
        TurnDetectionEvent::SpeechContinuing
    );
    let TurnDetectionEvent::TurnCompleted(utt) = feed(&mut intake, 0.0, 12_800, 1_300, 800) else {
        panic!("expected TurnCompleted");
    };
    assert_eq!(
        (utt.sample_count(), utt.duration_ms(), utt.is_truncated()),
        (17_600, 1_100, false)
    );
}

#[test]
fn push_to_talk_ignores_unheld_audio_captures_on_hold_release_and_guards_vad_mode() {
    let mut vad = SpeechIntake::for_session(false);
    assert_eq!(
        vad.set_ptt_held(true).unwrap_err().kind,
        SessionErrorKind::InvalidStateTransition
    );

    let mut ptt = SpeechIntake::for_session(true);
    assert!(ptt.is_push_to_talk() && !ptt.is_ptt_held());
    assert_eq!(
        feed(&mut ptt, 0.2, 1_600, 0, 100),
        TurnDetectionEvent::Silence
    );

    assert!(ptt.set_ptt_held(true).unwrap().is_none());
    assert!(ptt.is_ptt_held());
    assert_eq!(
        feed(&mut ptt, 0.05, 4_000, 100, 250),
        TurnDetectionEvent::SpeechContinuing
    );
    assert_eq!(
        feed(&mut ptt, 0.01, 4_000, 350, 250),
        TurnDetectionEvent::SpeechContinuing
    );

    let utt = ptt.set_ptt_held(false).unwrap().expect("released");
    assert!(!ptt.is_ptt_held());
    assert_eq!((utt.sample_count(), utt.duration_ms()), (8_000, 500));
    assert!(ptt.set_ptt_held(false).unwrap().is_none());
}
