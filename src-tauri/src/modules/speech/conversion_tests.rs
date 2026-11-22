//! Deterministic tests for audio adapter contracts, bounded buffers, non-blocking handoff,
//! and sample-rate/channel conversion (`T014`, `T015`, `T017`).

use crate::domain::{
    AudioChunk, AudioFormat, SampleRepresentation, SessionErrorKind, MAX_SYNTHESIZED_AUDIO_SAMPLES,
};
use crate::modules::speech::{
    audio_capture::{AudioInput, AudioInputDeviceInfo, MockAudioInput, NonBlockingAudioSink},
    conversion::{
        decode_wav_bytes_to_audio_chunk, encode_pcm16_wav, normalize_chunk_to_16khz_mono,
        pcm_i16_to_f32, resample_mono_f32, stereo_to_mono_f32,
    },
    utterance::BoundedUtteranceBuffer,
};

#[tokio::test]
async fn mock_audio_input_streams_bounded_chunks_and_tracks_capture_lifecycle() {
    let device = AudioInputDeviceInfo {
        id: "mic-default".to_string(),
        name: "Mock USB Headset".to_string(),
        is_default: true,
    };
    let chunk = AudioChunk::new_bounded(
        AudioFormat::WHISPER_16KHZ_MONO_F32,
        vec![0.1, 0.2, -0.1],
        0,
        20,
    )
    .expect("valid chunk");

    let mock = MockAudioInput::new(vec![device.clone()], vec![chunk.clone()]);
    assert!(!mock.is_capturing());
    assert_eq!(mock.list_input_devices().await.unwrap(), vec![device]);

    let mut rx = mock
        .start_capture(Some("mic-default"), AudioFormat::WHISPER_16KHZ_MONO_F32)
        .await
        .expect("start capture");
    assert!(mock.is_capturing());
    assert_eq!(rx.recv().await.unwrap(), chunk);

    mock.stop_capture().await.expect("stop capture");
    assert!(!mock.is_capturing());
}

#[tokio::test]
async fn mock_audio_input_propagates_simulated_device_error() {
    let mock = MockAudioInput::default();
    let disconnect_err = mock.simulate_device_disconnect();
    assert_eq!(
        disconnect_err.kind,
        SessionErrorKind::AudioDeviceUnavailable
    );

    let err = mock
        .start_capture(None, AudioFormat::WHISPER_16KHZ_MONO_F32)
        .await
        .expect_err("should fail");
    assert_eq!(err.kind, SessionErrorKind::AudioDeviceUnavailable);
}

#[test]
fn pcm_i16_stereo_downmix_and_resampling_produce_expected_16khz_mono_output() {
    let normalized = pcm_i16_to_f32(&[0, i16::MAX, i16::MIN]);
    assert!((normalized[0] - 0.0).abs() < 1e-5);
    assert!((normalized[1] - 1.0).abs() < 1e-4);
    assert!((normalized[2] + 1.0).abs() < 1e-4);

    let mono = stereo_to_mono_f32(&[0.4, 0.2, -0.6, -0.2]);
    assert_eq!(mono.len(), 2);
    assert!((mono[0] - 0.3).abs() < 1e-5);
    assert!((mono[1] + 0.4).abs() < 1e-5);

    // 48 kHz mono (480 samples = 10 ms) resampled to 16 kHz mono (160 samples = 10 ms)
    let src_48k = vec![0.25_f32; 480];
    let dst_16k = resample_mono_f32(&src_48k, 48_000, 16_000).expect("resample 48k -> 16k");
    assert_eq!(dst_16k.len(), 160);
    assert!((dst_16k[0] - 0.25).abs() < 1e-5);

    let stereo_48k_fmt = AudioFormat {
        sample_rate_hz: 48_000,
        channels: 2,
        sample_format: SampleRepresentation::PcmF32,
    };
    let stereo_chunk =
        AudioChunk::new_bounded(stereo_48k_fmt, vec![0.5_f32; 960], 0, 10).expect("valid chunk");
    let converted = normalize_chunk_to_16khz_mono(&stereo_chunk).expect("convert to 16k mono");
    assert_eq!(converted.len(), 160);
}

#[test]
fn bounded_utterance_buffer_caps_samples_without_unbounded_growth() {
    let mut buffer = BoundedUtteranceBuffer::new(10);
    let chunk1 =
        AudioChunk::new_bounded(AudioFormat::WHISPER_16KHZ_MONO_F32, vec![0.1; 6], 0, 10).unwrap();
    let chunk2 =
        AudioChunk::new_bounded(AudioFormat::WHISPER_16KHZ_MONO_F32, vec![0.2; 8], 10, 10).unwrap();

    assert!(!buffer.push_chunk(&chunk1).unwrap());
    assert!(buffer.push_chunk(&chunk2).unwrap());
    assert!(buffer.is_full());

    let utterance = buffer.take_utterance().expect("utterance");
    assert_eq!(utterance.sample_count(), 10);
    assert!(utterance.is_truncated());
}

#[tokio::test]
async fn non_blocking_audio_sink_never_blocks_and_counts_dropped_chunks_when_full() {
    let (sink, mut rx) = NonBlockingAudioSink::new_bounded(2);
    let chunk =
        AudioChunk::new_bounded(AudioFormat::WHISPER_16KHZ_MONO_F32, vec![0.1; 4], 0, 10).unwrap();

    assert!(sink.push_from_callback(chunk.clone()));
    assert!(sink.push_from_callback(chunk.clone()));
    assert!(!sink.push_from_callback(chunk));
    assert_eq!(sink.dropped_chunk_count(), 1);

    assert!(rx.recv().await.is_some());
}

#[test]
fn pcm16_wav_codec_roundtrips_valid_audio_and_rejects_malformed_headers() {
    let wav_16k = encode_pcm16_wav(&[0.0, 0.5, -0.5], 16_000);
    assert_eq!(&wav_16k[0..4], b"RIFF");
    assert_eq!(&wav_16k[8..12], b"WAVE");
    assert_eq!(wav_16k.len(), 44 + 6);

    let wav_24k = encode_pcm16_wav(&[0.25, -0.25, 0.5, -0.5], 24_000);
    let chunk = decode_wav_bytes_to_audio_chunk(&wav_24k).expect("wav");
    assert_eq!(chunk.samples_f32.len(), 4);
    assert_eq!(chunk.format.sample_rate_hz, 24_000);

    assert_eq!(
        decode_wav_bytes_to_audio_chunk(b"NOT_A_WAV")
            .unwrap_err()
            .kind,
        SessionErrorKind::TtsFailed
    );
    let zero_samples_wav = encode_pcm16_wav(&[], 24_000);
    assert_eq!(
        decode_wav_bytes_to_audio_chunk(&zero_samples_wav)
            .unwrap_err()
            .kind,
        SessionErrorKind::TtsFailed
    );
    let zero_rate_wav = encode_pcm16_wav(&[0.1, 0.2], 0);
    assert_eq!(
        decode_wav_bytes_to_audio_chunk(&zero_rate_wav)
            .unwrap_err()
            .kind,
        SessionErrorKind::TtsFailed
    );

    let mut truncated_payload = wav_24k[..45].to_vec();
    truncated_payload[40..44].copy_from_slice(&1_u32.to_le_bytes());
    assert_eq!(
        decode_wav_bytes_to_audio_chunk(&truncated_payload)
            .unwrap_err()
            .kind,
        SessionErrorKind::TtsFailed
    );

    let oversized = vec![0.0_f32; MAX_SYNTHESIZED_AUDIO_SAMPLES + 1];
    let oversized_wav = encode_pcm16_wav(&oversized, 48_000);
    assert_eq!(
        decode_wav_bytes_to_audio_chunk(&oversized_wav)
            .unwrap_err()
            .kind,
        SessionErrorKind::TtsFailed
    );
}
