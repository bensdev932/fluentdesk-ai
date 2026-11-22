# Performance, Memory Stability & Security Hardening Audit (`T055`, `T056`, `T057`)

## 1. Agreed Minimum Hardware Profile & Pipeline Stage Latency Targets (`T056`)

- **Target OS & Hardware Profile:** Windows 11 64-bit desktop/laptop, 16 GB RAM, 4-core/8-thread x86_64 CPU, integrated graphics or optional local GPU acceleration, standard WASAPI microphone & speakers.
- **Configured Local Models:**
  - STT: `whisper.cpp` `ggml-base.en.bin` (~142 MB) via controlled sidecar (`docs/whisper-integration.md`).
  - LLM: Ollama `llama3.2:3b` (~2.0 GB) via loopback HTTP `127.0.0.1:11434` (`docs/ollama-integration.md`).
  - TTS: Kokoro-82M ONNX / Piper local sidecar (~82–330 MB, `docs/tts-integration.md`).
- **Stage Latency Measurement Architecture:**
  - `TurnStageMetadata` (`stt_duration_ms`, `llm_duration_ms`, `tts_duration_ms`) records non-sensitive timing per conversation turn in `conversation_turns`.
  - **Simulated 30-Minute Orchestration Baseline (`t055_t056` in `src-tauri/src/modules/hardening_tests.rs`):** 180 full voice turns (representing 30 minutes at 10 s/turn) through `EndToEndVoiceLoop` + SQLite in-memory persistence + `BoundedUtteranceBuffer` completed in **0.12 s total test runtime** (mocked model inference overhead $< 0.7\text{ ms}$ per turn for Rust state machine, context windowing, WAV/PCM boxing, and SQLite writes).
  - *Note on Live Model Inference Timing:* Actual physical hardware wall-clock latency for `whisper-cli`, `ollama serve`, and `kokoro-tts` sidecar binaries depends on host CPU/GPU throughput and must be recorded manually on the target machine using the opt-in integration tests documented in `docs/manual-e2e-test-checklist.md`.

---

## 2. 30-Minute Memory & Buffer Stability (`T055`)

Verified in `src-tauri/src/modules/hardening_tests.rs`:
- **Audio Capture Channel:** `NonBlockingAudioSink` uses a bounded `tokio::sync::mpsc` queue (`try_send`) so slow consumers drop overflow frames cleanly rather than blocking audio callbacks or allocating unbounded memory.
- **Utterance Accumulation:** `BoundedUtteranceBuffer` enforces `MAX_UTTERANCE_SAMPLES_16KHZ = 480_000` (30 s cap at 16 kHz mono) and resets its vector on `take_utterance()`.
- **Active LLM Context Window:** `MaxContextTurns(1..=50)` and `BoundedConversationContext` evict the oldest turn in FIFO order (`pop_front`) so active prompt size stays constant over 180+ turns while SQLite stores the full chronological history.

---

## 3. Security, Privacy & IPC Hardening Audit (`T057`)

| Control Area | Verification & Implementation |
|---|---|
| **IPC Command Surface (`src-tauri/src/commands/`, `src/services/tauriService.ts`)** | Narrow typed commands only (`start_speaking_session`, `stop_speaking_session`, `pause_speaking_session`, `resume_speaking_session`, `list_audio_input_devices`, `set_audio_input_device`, `list_sessions`, `get_session_history`, `update_correction_mode`). Zero shell-execution, arbitrary path, or raw URL parameters cross the IPC boundary. |
| **Local Endpoint Binding (`src-tauri/src/ai/ollama.rs`)** | `OllamaConfig::try_new` strictly enforces `127.0.0.1`, `localhost`, or `::1`; remote hostnames/URLs fail validation before any socket connection. |
| **Raw Audio Privacy (`src-tauri/src/commands/settings.rs`, `001_initial_schema.sql`)** | `UserVoiceSettings::default()` sets `raw_audio_recording_opt_in = false` and `cloud_opt_in = false`. `RawAudioRetentionPolicy::require_explicit_recording_consent` blocks raw audio storage unless explicitly opted in. SQLite schema contains zero raw audio BLOB or file-path columns. |
| **Secret Handling & Log Redaction (`src-tauri/src/telemetry/mod.rs`, `src-tauri/src/domain/error.rs`)** | `RedactedSecret` masks secrets across `Debug` and `Display`. `SessionError::internal_detail` is marked `#[serde(skip)]` so internal traces/paths never leak across Tauri IPC JSON responses. Telemetry events explicitly exclude raw audio samples and scrub `Bearer` / API key patterns. |
