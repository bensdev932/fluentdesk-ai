# Local Text-to-Speech (TTS) Evaluation & Integration (`T040`, `T041`)

## 1. Engine Evaluation Summary

| Candidate Engine | Model / Voice License | Windows / macOS / Linux Packaging | Voice Latency (CPU) | Decision |
|---|---|---|---|---|
| **Kokoro-82M ONNX / Piper CLI Sidecar** | **Apache-2.0** (Kokoro-82M) / **MIT** (Piper voices) | Standalone sidecar binary + `.onnx` model + voice pack in app data dir | ~80–220 ms per sentence | **Selected Primary Local TTS** |
| In-process `ort` (ONNX Runtime C++ FFI) | MIT / Apache-2.0 | Requires matching native `onnxruntime.dll` / `.dylib` symbols at link time | ~70–200 ms per sentence | Deferred (native DLL linkage friction on Windows MSVC) |
| OS Native TTS (Windows SAPI / macOS AVSpeech) | OS bundled | Built into OS, inconsistent voice quality across platforms | ~50–150 ms | Optional fallback only |

## 2. Selected Architecture (`LocalKokoroTtsAdapter` + `TtsSidecarRunner`)

- **Distributable License Compliance:** Kokoro-82M (`kokoro-v1.0.onnx` + voice style vectors) is licensed under **Apache-2.0**, permitting commercial and open-source desktop redistribution without external API keys or telemetry.
- **Process Boundary (`src-tauri/src/tts/sidecar.rs`):**
  - `TtsSidecarRunner` abstracts invocation of the local neural TTS binary so unit and integration tests run deterministically offline without requiring a multi-hundred-megabyte model in CI.
  - `CliTtsSidecarRunner` invokes the configured local binary (`--model`, `--voice`, `--speed`, `--text`) and decodes 16-bit PCM WAV output into a validated `AudioChunk`.
- **Long-Text Sentence Chunking (`src-tauri/src/tts/kokoro.rs`):**
  - Spoken tutor responses exceeding a single clause are split on sentence boundaries (`split_text_into_sentences`) before synthesis so long responses never exceed local phoneme token windows, and synthesized audio segments are concatenated seamlessly.
- **Graceful Degradation (`constitution.md` Principle 7, `T045`):**
  - If TTS synthesis or audio output device playback fails during a turn, `SpeechPlaybackCoordinator` preserves the persisted user transcript and visible `TutorReply` text (`TutorPlaybackOutcome::TextOnlyDegraded`), recovers the output stream, and returns the session to `Listening` so the learner can continue practicing.

## 3. Running the Opt-In Real TTS Integration Test

```powershell
$env:FLUENTDESK_TTS_BIN = "C:\path\to\kokoro-tts.exe"
$env:FLUENTDESK_TTS_MODEL = "C:\path\to\kokoro-v1.0.onnx"
cargo test --manifest-path src-tauri/Cargo.toml real_local_tts_synthesizes_audio_when_configured -- --ignored --nocapture
```
