# Speech-to-Text (STT) Integration Decision: whisper.cpp

**Task ID:** `T024`  
**Feature:** Voice-First Conversation Loop  
**Status:** Approved Architectural Decision  

---

## 1. Context & Requirements

FluentDesk AI requires local-first, low-latency, free-first speech-to-text recognition running on Windows 11 (WASAPI / x64) and other desktop platforms without mandatory internet access (`constitution.md` Principle 2).

Key evaluation points:
1. **Packaging & Stability:** Direct C++ source bindings in Rust (`whisper-rs`) require complex native toolchains (CMake, MSVC C++ runtime, OpenMP/BLAS) and native linking that can fail depending on host SDK configurations.
2. **Local Sidecar Architecture:** A pre-compiled or local standalone executable (`whisper-cli.exe` or controlled localhost HTTP server via whisper.cpp) isolates native memory, prevents segmentation faults from crashing the Tauri app, and permits out-of-process model life-cycle management.
3. **Pluggable Architecture:** Keeping Whisper behind the `SpeechToText` trait boundary (`stt/provider.rs`) ensures the app can run:
   - Mock provider for fast deterministic CI/unit tests (`stt/mock.rs`).
   - Local whisper.cpp executable / sidecar adapter (`stt/whisper.rs`).
   - Optional future hardware-accelerated engines (DirectML / CUDA).

---

## 2. Selected Architecture: Controlled Local whisper.cpp Adapter

The application communicates with a local whisper.cpp engine through a controlled process/sidecar adapter:
- **Audio Handoff:** 16 kHz mono WAV/PCM stream or bounded memory buffers.
- **Model Storage:** User-configured model path (`models/ggml-base.en.bin` or `tiny.en`).
- **Health Checks:** Validates model file existence, read permissions, and non-empty file size before starting sessions.
- **Error Isolation:** If Whisper encounters corrupted audio or terminates unexpectedly, it returns typed `SessionError` (`SttFailed` / `SttUnavailable`) without corrupting application state.
