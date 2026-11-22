//! Encapsulated `LocalKokoroTtsAdapter` tests exercised directly through `TextToSpeech`.

use crate::domain::{ProviderHealth, SessionErrorKind, MAX_SYNTHESIZED_AUDIO_SAMPLES};
use crate::modules::speech::conversion::encode_pcm16_wav;
use crate::tts::{
    kokoro::{KokoroModelConfig, LocalKokoroTtsAdapter},
    provider::TextToSpeech,
    sidecar::TtsProcessOutput,
    streaming_helpers::ScriptedTtsRunner,
};
use std::path::Path;
use std::sync::Arc;

fn ok_wav(wav: Vec<u8>) -> Result<TtsProcessOutput, crate::domain::SessionError> {
    Ok(TtsProcessOutput {
        success: true,
        stdout: wav,
    })
}

#[tokio::test]
async fn local_kokoro_invokes_cli_per_sentence_assembles_over_5s_and_maps_failures() {
    let wav_30k = encode_pcm16_wav(&vec![0.1; 30_000], 24_000);
    let runner = Arc::new(ScriptedTtsRunner::with_outcomes(vec![
        ok_wav(wav_30k.clone()),
        ok_wav(wav_30k.clone()),
        ok_wav(wav_30k),
        Ok(TtsProcessOutput {
            success: false,
            stdout: Vec::new(),
        }),
        ok_wav(b"TRUNCATED".to_vec()),
    ]));
    let cfg = KokoroModelConfig::try_new("kokoro-cli.exe", "kokoro.onnx", "af_bella").unwrap();
    let adapter = LocalKokoroTtsAdapter::new_with_runner(cfg, runner.clone());

    let reply = "That is a wonderful story! Where did you stay in Kyoto? Tell me about the food.";
    let chunk = adapter.synthesize(reply, 1.1).await.expect("synthesized");
    assert_eq!(
        (chunk.samples_f32.len(), chunk.format.sample_rate_hz),
        (90_000, 24_000)
    );
    let calls = runner.calls();
    assert_eq!(calls.len(), 3);
    assert_eq!(calls[0].0, Path::new("kokoro-cli.exe"));
    assert_eq!(
        calls[0].1,
        [
            "--model",
            "kokoro.onnx",
            "--voice",
            "af_bella",
            "--speed",
            "1.10",
            "--text",
            "That is a wonderful story!"
        ]
    );
    assert_eq!(calls[1].1.last().unwrap(), "Where did you stay in Kyoto?");
    assert_eq!(calls[2].1.last().unwrap(), "Tell me about the food.");

    assert_eq!(
        adapter.synthesize("Bad exit.", 1.0).await.unwrap_err().kind,
        SessionErrorKind::TtsFailed
    );
    assert_eq!(
        adapter.synthesize("Bad wav.", 1.0).await.unwrap_err().kind,
        SessionErrorKind::TtsFailed
    );

    let missing = std::env::temp_dir().join("missing-kokoro-tts.exe");
    let unconfigured = LocalKokoroTtsAdapter::new(
        KokoroModelConfig::try_new(&missing, &missing, "af_bella").unwrap(),
    );
    assert_eq!(
        unconfigured.health_check().await,
        ProviderHealth::Misconfigured
    );
    assert_eq!(
        unconfigured
            .synthesize("Hello!", 1.0)
            .await
            .unwrap_err()
            .kind,
        SessionErrorKind::TtsFailed
    );
}

#[tokio::test]
async fn local_kokoro_rejects_mismatched_sentence_formats_and_oversized_assembly() {
    let half_max = encode_pcm16_wav(&vec![0.05; MAX_SYNTHESIZED_AUDIO_SAMPLES / 2 + 1], 24_000);
    let wav_24k = encode_pcm16_wav(&[0.1; 120], 24_000);
    let wav_16k = encode_pcm16_wav(&[0.1; 120], 16_000);
    let runner = Arc::new(ScriptedTtsRunner::with_outcomes(vec![
        ok_wav(wav_24k),
        ok_wav(wav_16k),
        ok_wav(half_max.clone()),
        ok_wav(half_max),
    ]));
    let cfg = KokoroModelConfig::try_new("kokoro-cli.exe", "kokoro.onnx", "af_bella").unwrap();
    let adapter = LocalKokoroTtsAdapter::new_with_runner(cfg, runner);
    assert_eq!(
        adapter.synthesize("One. Two.", 1.0).await.unwrap_err().kind,
        SessionErrorKind::TtsUnavailable
    );
    assert_eq!(
        adapter
            .synthesize("Three. Four.", 1.0)
            .await
            .unwrap_err()
            .kind,
        SessionErrorKind::TtsUnavailable
    );
}
