# Phase 0 — Repository Foundation & Process Log

**Feature ID:** 001 — Voice-First Conversation Loop  
**Date:** 2026-11-15  
**Scope:** Phase 0 (`T001`–`T007`) of [tasks.md](../tasks.md)

---

## Process Log Summary (What Has Been Done So Far)

1. **Specification & Design Review:**
   - Read and analyzed all project governance and design documents in the workspace:
     - `AGENTS.md`: AI coding agent instructions, product guardrail (voice-first tutor), implementation rules, TDD workflow, and required completion report.
     - `constitution.md`: Project binding principles v1.0.0 (voice-first product boundary, local-first privacy, strict module boundaries, single-owner session state machine, bounded audio/concurrency, untrusted model output, Spec-Driven TDD, desktop security, truthfulness).
     - `prd.md`, `spec.md`, `plan.md`, `software-design.md`, `tasks.md`, `tdd.md`.
   - Verified casual speaking support (`FR-004: Free conversation mode` and `Natural Conversation` correction mode) and answered architectural review questions.

2. **`T001` — Repository, Toolchain, Target OS, and Existing User Changes Inspection:**
   - Inspected workspace files, git repository status, operating system environment, and installed compiler/runtime toolchains without overwriting any existing user files.

---

## T001 Findings Record

### 1. Existing Repository State & User Changes
- **Workspace Path:** `c:\Users\ben\Documents\English AI tutor`
- **Git Status:** Not yet initialized as a Git repository (`fatal: not a git repository`).
- **Existing Files (8 Markdown specification files, preserved untouched):**
  - `AGENTS.md` (2,707 bytes)
  - `constitution.md` (6,329 bytes)
  - `plan.md` (12,487 bytes)
  - `prd.md` (17,745 bytes)
  - `software-design.md` (28,690 bytes)
  - `spec.md` (14,193 bytes)
  - `tasks.md` (5,535 bytes)
  - `tdd.md` (16,743 bytes)
- **Note on Constitution Path:** `AGENTS.md` references `.specify/memory/constitution.md`, whereas `constitution.md` resides in the repository root. Both paths are preserved/mirrored so tooling and agents referencing either path resolve identical binding principles.

### 2. Target Operating System
- **OS:** Windows 11 (`Win32NT 10.0.26300.0`, 64-bit)
- **Audio Subsystem Target:** WASAPI (via `cpal` / `rodio` in Rust backend)

### 3. Toolchain Inspection Results

| Tool / Component | Status | Version / Location | Notes |
|---|---|---|---|
| **Node.js** | Installed | `v24.22.1` (`C:\Program Files\nodejs\node.exe`) | Ready for React/TypeScript/Vite frontend & Vitest |
| **npm** | Installed | `12.2.0` (`C:\Program Files\nodejs\npm.ps1`) | Works inside standard sandbox |
| **Git** | Installed | `git version 2.55.0.windows.3` | Ready for repo initialization & diff checks |
| **Python** | Installed | `Python 3.10.11` | Available for auxiliary scripts |
| **Visual Studio C++ Tools** | Installed | `C:\Program Files\Microsoft Visual Studio` & `(x86)` | Required for native Windows Rust (`msvc`) linking |
| **Ollama** | Installed | `C:\Users\ben\AppData\Local\Programs\Ollama\ollama.exe` | Available for Phase 4 local LLM integration |
| **FFmpeg** | Installed | `C:\Users\ben\AppData\Local\Microsoft\WinGet\Links\ffmpeg.exe` | Available for audio fixture utilities |
| **Rust (`cargo` / `rustc`)** | Missing initially | Not found in `$env:USERPROFILE\.cargo\bin` or `PATH` | Required for Rust backend compilation, `cargo fmt`, `cargo clippy`, and `cargo test` |

---

## Phase 0 Implementation Log (`T001`–`T007`)

### `T001` — Repository, Toolchain, and Environment Inspection
- Recorded initial state of all 8 specification documents (`docs/AGENTS.md`, `docs/constitution.md`, `docs/plan.md`, `docs/prd.md`, `docs/software-design.md`, `docs/spec.md`, `docs/tasks.md`, `docs/tdd.md`) and mirrored `constitution.md` to `.specify/memory/constitution.md` so path references in `AGENTS.md` resolve cleanly.
- Installed Rust toolchain (`cargo 1.100.0`, `rustc 1.100.0`) via `winget install --id Rustlang.Rustup`.
- Diagnosed Windows MSVC linker resolution: host had both Visual Studio 18 Community (with `onecore` libs only) and Visual Studio 2022 BuildTools (`14.44.35207`, containing full desktop x64 CRT `msvcrt.lib` and Windows 10 SDK `10.0.26100.0`). Configured local `.cargo/config.toml` (git-ignored) to point `rustc` and `cc` at the complete VS 2022 BuildTools x64 toolchain.

### `T002` — Rust Workspace & Frontend Module Boundaries
- Pinned toolchain in `rust-toolchain.toml` (`stable`, `rustfmt`, `clippy`).
- Created frontend shell (`package.json`, `tsconfig.json`, `vite.config.ts`, `index.html`, `src/main.tsx`, `src/App.tsx`, `src/types/domain.ts`, `src/services/tauriService.ts`).
- Created Rust crate `fluentdesk-core` (`src-tauri/Cargo.toml`, `src-tauri/src/lib.rs`, `src-tauri/src/main.rs`) with explicit module boundaries:
  - `domain`: Shared typed domain contracts.
  - `telemetry`: Structured tracing and secret/audio redaction.
  - `commands`: Narrow, typed IPC commands (`conversation`, `audio`, `settings`, `dashboard`).
  - `modules`: Application domain logic (`conversation`, `speech`, `tutor`, `learning`).
  - `ai`: LLM provider trait and adapters (`provider`, `ollama`, `llama_cpp`, `mock`).
  - `stt`: Speech-to-text trait and adapters (`provider`, `whisper`, `mock`).
  - `tts`: Text-to-speech trait and adapters (`provider`, `kokoro`, `mock`).
  - `database`: SQLite connection, migrations (`001_initial_schema.sql`), and repositories.

### `T003` — Shared Typed Domain Models (TDD Red $\to$ Green)
- **Red Step:** Added unit tests in `src/types/domain.test.ts` (frontend) and `src-tauri/src/domain/mod.rs` (Rust) covering `SessionId`, `SessionState` (`Idle`, `Initializing`, `Listening`, `Processing`, `Thinking`, `Speaking`, `Paused`, `Error`, `Ended`), `AudioFormat`, bounded `AudioChunk`, `Transcript`, `ConversationTurn`, `TutorReply`, `CorrectionMode` (`Natural`, `Gentle`, `Detailed`, `Off`), `ProviderHealth`, and `SessionError`. Confirmed initial failures before implementation.
- **Green Step:** Implemented domain validation (`MAX_AUDIO_CHUNK_SAMPLES = 80,000` samples / 5s at 16 kHz mono, confidence bounds `0.0..=1.0`, control-character stripping on untrusted model output, `#[serde(skip)]` on internal error details) and verified all 6 Rust domain tests and 5 frontend Vitest tests pass.

### `T004` — Tracing with Redaction Rules (TDD Red $\to$ Green)
- **Red Step:** Added unit tests in `src-tauri/src/telemetry/mod.rs` asserting that `RedactedSecret` never leaks in `Debug`, `Display`, or `tracing::info!` spans, that `AudioChunk` never logs raw PCM samples (`<REDACTED_RAW_AUDIO>`), and that free-form log messages scrub `Bearer` tokens, `sk-...` keys, and `api_key=...` patterns. Confirmed initial failure.
- **Green Step:** Implemented `RedactedSecret`, custom `fmt::Debug` on `AudioChunk`, and `redact_sensitive_log_text`. Verified all 3 redaction tests pass.

### `T005` — SQLite Migration Framework & Initial Schema (TDD Red $\to$ Green)
- **Red Step:** Created `src-tauri/src/database/migrations/001_initial_schema.sql` (`users`, `session_settings`, `conversations`, `conversation_turns` with chronological and user-session indexes, foreign keys, and zero raw-audio/BLOB columns) and wrote 3 async repository tests in `src-tauri/src/database/repository.rs`. Confirmed initial failure.
- **Green Step:** Implemented `init_sqlite_pool`, idempotent `run_migrations` (tracked in `_schema_migrations`), and `SessionRepository`. Verified all 3 SQLite migration/repository tests pass in-memory.

### `T006` — Mock Implementations for Audio, STT, LLM, TTS, and Playback Interfaces (TDD Red $\to$ Green)
- **Red Step:** Defined async traits (`AudioInput`, `SpeechToText`, `AiProvider`, `TextToSpeech`, `AudioOutput`) and wrote 6 contract unit tests for `MockAudioInput`, `MockSttProvider`, `MockAiProvider`, `MockTtsProvider`, and `MockAudioOutput`. Confirmed initial failure.
- **Green Step:** Implemented deterministic mock providers with bounded channels, scripted responses, call recording, and failure injection. Verified all 6 mock contract tests pass.

### `T007` — CI Checks for Format, Clippy, Unit Tests, and Frontend Tests
- Added `.github/workflows/ci.yml` enforcing `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, `npm run typecheck`, and `npm test`.
- Executed all checks locally and verified zero warnings and 100% passing tests.

---

## Phase 1 Implementation Log (`T008`–`T013`)

### Architectural Refactoring & Guardrails (`rules.md`)
- Established `rules.md` enforcing SOLID principles, Value Objects with private inner storage (`SessionId`, `SessionCancellationToken`), Law of Demeter, Tell-Don't-Ask, $\le 10$ lines per method, $\le 50$ lines per React `.tsx` component, and a strict **$\le 200$ lines per source file** cap across the repository.
- Split `src-tauri/src/domain/` (`session.rs`, `audio.rs`, `conversation.rs`, `error.rs`, `tests.rs`) and `src-tauri/src/database/` (`connection.rs`, `repository.rs`, `mappers.rs`, `tests.rs`) so all files remain well below 200 lines.

### `T008` & `T009` — Session State Transition Rules & Single-Owner `SessionManager` (TDD Red $\to$ Green)
- **Red Step:** Added unit tests in `src-tauri/src/modules/conversation/tests.rs` verifying valid voice-loop transitions (`Idle -> Initializing -> Listening -> Processing -> Thinking -> Speaking -> Listening -> Paused -> Listening -> Ended -> Idle`) and rejecting invalid jumps (`Idle -> Speaking`, `Paused -> Thinking`, etc.) with `SessionErrorKind::InvalidStateTransition`.
- **Green Step:** Implemented `SessionState::can_transition_to` in `src-tauri/src/domain/session.rs` and single-owner `SessionManager` in `src-tauri/src/modules/conversation/manager.rs`.

### `T010` & `T011` — Start, Pause, Resume, Stop Commands & Cancellation Cleanup (TDD Red $\to$ Green)
- Implemented `SessionCancellationToken` (`src-tauri/src/modules/conversation/cancellation.rs`) using `tokio::sync::watch`.
- Implemented `SessionManager::{start, pause, resume, stop}` and `ConversationCommandService` (`src-tauri/src/commands/conversation.rs`) with Tell-Don't-Ask encapsulation of child adapters and deterministic audio capture/playback teardown on `pause` and `stop`.

### `T012` — Frontend UI State Display (TDD Red $\to$ Green)
- Added `src/components/SessionStateBadge.test.ts` (Red $\to$ Green), `src/components/sessionStatePresentation.ts`, and `src/components/SessionStateBadge.tsx` (33 lines) displaying truthful labels, descriptions, and `Mic Live` / `Mic Off` status for `Idle`, `Initializing`, `Listening`, `Processing`, `Thinking`, `Speaking`, `Paused`, `Error`, and `Ended`.

### `T013` — Pipeline Stage Cancellation Integration Tests (TDD Red $\to$ Green)
- Added `src-tauri/src/modules/conversation/pipeline_tests.rs` testing `start` $\to$ `pause` $\to$ `resume` $\to$ `stop`, session ID validation, and deterministic cancellation and resource cleanup across every active pipeline stage (`Listening`, `Processing`, `Thinking`, `Speaking`, `Paused`).

---

## Phase 2 Implementation Log (`T014`–`T021`)

### `T014`, `T015`, `T017` — Audio Adapter Contract, Sample Conversion & Non-Blocking Handoff (TDD Red $\to$ Green)
- **Red Step:** Added `src-tauri/src/modules/speech/conversion_tests.rs` testing `pcm_i16_to_f32`, `stereo_to_mono_f32`, 48 kHz $\to$ 16 kHz resampling, `BoundedUtteranceBuffer` capacity capping, and `NonBlockingAudioSink::push_from_callback` (`try_send` drop counting without blocking).
- **Green Step:** Implemented `conversion.rs` (146 lines) and `NonBlockingAudioSink` in `audio_capture.rs` (188 lines).

### `T016`, `T018`, `T019`, `T020`, `T021` — Device Selection, Push-to-Talk, VAD Endpointing & Fixture Tests (TDD Red $\to$ Green)
- **Red Step:** Added `src-tauri/src/modules/speech/vad_and_ptt_tests.rs` testing device enumeration/selection (`AudioDeviceCommandService`), push-to-talk auto-cutoff at `MaxUtteranceDurationMs`, and `EnergyTurnDetector` (`TurnDetector` trait) across silence, background noise, 400 ms learner mid-sentence pauses (`< 800 ms` hangover), quiet speech, and device disconnect.
- **Green Step:** Implemented `commands/audio.rs` (92 lines), `push_to_talk.rs` (94 lines), `vad.rs` (172 lines), and documented the Windows 11 WASAPI manual microphone permission & hot-plug test runbook in `docs/manual-audio-test-checklist.md`.

---

## Phase 3 Implementation Log (`T022`–`T028`)

### `T022` & `T024` — STT Adapter Contract & whisper.cpp Decision
- Formally defined `SpeechToText` adapter trait in `src-tauri/src/stt/provider.rs` (14 lines).
- Documented the whisper.cpp local integration decision in `docs/whisper-integration.md` (sidecar/controlled process architecture to ensure memory safety, native linking resilience, and out-of-process model lifecycle).

### `T023`, `T025`, `T026`, `T028` — Whisper Config, Transcription Coordinator & Fixture Tests (TDD Red $\to$ Green)
- Implemented `WhisperModelConfig` and `LocalWhisperAdapter` in `src-tauri/src/stt/whisper.rs` (151 lines) with model path validation and health check verification.
- Implemented `TranscriptionCoordinator` and Whisper noise/hallucination filtering (`is_noise_or_hallucination`) in `src-tauri/src/modules/speech/transcription.rs` (89 lines), emitting `UserTranscriptReadyEvent`.
- Added unit and fixture tests in `src-tauri/src/stt/tests.rs` (103 lines) covering noise filtering, empty audio handling, model configuration validation, and offline fixture transcription.

### `T027` — Frontend Transcript Panel
- Created `src/components/TranscriptPanel.tsx` (45 lines) and `src/components/TranscriptPanel.test.ts` (26 lines), clearly distinguishing recognized user speech (with confidence indicators) from AI tutor responses.

---

## Phase 4 Implementation Log (`T029`–`T037`)

### `T029` & `T030` — AI Provider Cancellation Contract & Bounded Context (TDD Red $\to$ Green)
- **Red Step:** Added unit and integration tests in `src-tauri/src/modules/tutor/tests.rs` and `src-tauri/src/ai/tests.rs` verifying `MaxContextTurns` bounds (`1..=50`), `BoundedConversationContext` FIFO ordering and oldest-turn eviction, and `generate_with_cancellation` aborting with `SessionErrorKind::Cancelled`.
- **Green Step:** Implemented `generate_with_cancellation` in `src-tauri/src/ai/provider.rs` and `BoundedConversationContext` in `src-tauri/src/modules/conversation/context.rs`.

### `T031`, `T032`, `T033` — Ollama Local Adapter, Voice-First Prompts & Structured Response Fallback (TDD Red $\to$ Green)
- Implemented `OllamaConfig` (enforcing `127.0.0.1` / `localhost` / `::1`), `OllamaAiProvider`, and `LocalHttpOllamaTransport` in `src-tauri/src/ai/ollama.rs` and `src-tauri/src/ai/ollama_http.rs`.
- Implemented `assemble_voice_tutor_prompt` (`src-tauri/src/modules/tutor/prompts.rs`) with concise voice-first system instructions tailored to `CorrectionMode::{Natural, Gentle, Detailed, Off}`.
- Implemented `parse_tutor_response_or_fallback` (`src-tauri/src/modules/tutor/feedback.rs`) handling structured JSON, Markdown code fences, `CorrectionMode::Off` suppression, and safe plain-text fallback on malformed model output.

### `T034`, `T035`, `T036`, `T037` — Voice Turn Coordinator, Chronological SQLite Persistence & Opt-In Ollama Test
- Implemented `VoiceTurnCoordinator::process_user_transcript` (`src-tauri/src/modules/conversation/turn.rs`) persisting the user's completed transcript turn before invoking LLM inference so user speech is never lost if the AI provider fails (`constitution.md` Principle 7).
- Documented local Ollama setup and opt-in integration test execution in `docs/ollama-integration.md`.

---

## Phase 5 Implementation Log (`T038`–`T045`)

### `T038` & `T040` — TTS & Playback Cancellation Contracts & Local TTS Engine Selection
- Documented the local TTS evaluation and licensing decision (Kokoro-82M Apache-2.0 / Piper MIT via controlled local sidecar) in `docs/tts-integration.md`.
- Implemented `SpeakingSpeed` (`0.5..=2.0`) and `synthesize_with_cancellation` in `src-tauri/src/tts/provider.rs`, and `play_with_cancellation` in `src-tauri/src/modules/speech/playback.rs`.

### `T039`, `T041`, `T042` — Local Kokoro TTS Adapter, Sentence Splitting & WAV Validation (TDD Red $\to$ Green)
- Implemented `KokoroModelConfig`, `split_text_into_sentences`, and `LocalKokoroTtsAdapter` in `src-tauri/src/tts/kokoro.rs`, along with `TtsSidecarRunner`, `CliTtsSidecarRunner`, `encode_pcm16_wav`, and `decode_wav_bytes_to_audio_chunk` in `src-tauri/src/tts/sidecar.rs`.

### `T043`, `T044`, `T045` — Playback Lifecycle Events, Return to Listening & Graceful Text-Only Degradation
- Implemented `SpeechPlaybackCoordinator::speak_tutor_event` in `src-tauri/src/modules/speech/synthesis.rs` and `SessionManager::speak_tutor_reply` in `src-tauri/src/modules/conversation/turn.rs`.
- Verified via `src-tauri/src/tts/tests.rs` that active sessions return from `Speaking` to `Listening` after playback (`T044`) and that TTS or playback failures degrade gracefully to `TutorPlaybackOutcome::TextOnlyDegraded` while preserving visible tutor text and persisted transcripts (`T045`).

---

## Phase 6 Implementation Log (`T046`–`T051`)

### `T046` & `T047` — Speaking Room UI & Typed Tauri IPC Event Wiring (TDD Red $\to$ Green)
- Implemented `SpeakingRoomViewModel`, `getSpeakingControlPermissions`, and `reduceVoiceLoopEvent` in `src/components/speakingRoomController.ts` (113 lines) and typed event subscriptions in `src/services/tauriService.ts` (128 lines).
- Created modular React components under 50 lines each: `SpeakingControls.tsx` (43 lines), `TutorReplyCard.tsx` (40 lines), `SpeakingRoom.tsx` (39 lines), and updated `App.tsx` (20 lines).
- Verified with `src/components/SpeakingRoom.test.ts` (`12/12` Vitest tests passing, `npm run typecheck` passing).

### `T048`, `T049`, `T050`, `T051` — End-to-End Voice Loop, Stop Cancellation, Offline Verification & Failure Recovery
- Implemented `EndToEndVoiceLoop` in `src-tauri/src/modules/conversation/e2e_loop.rs` (170 lines) coordinating `start_session`, `process_spoken_utterance`, `recover_to_listening`, and `stop_immediately`.
- Added `src-tauri/src/modules/conversation/e2e_tests.rs` (163 lines) verifying multi-turn offline execution (`T050`), immediate stop cancellation (`T048`), and recovery from microphone, STT, LLM, and TTS failures (`T051`).
- Documented manual E2E and offline network-disabled verification procedures in `docs/manual-e2e-test-checklist.md` (`T049`, `T050`).

---

## Phase 7 Implementation Log (`T052`–`T059`)

### `T052` & `T053` — Session Review, Chronological History & Correction Mode Settings (TDD Red $\to$ Green)
- Extended `SessionRepository` (`src-tauri/src/database/repository.rs`, `mappers.rs`) with `list_sessions_for_user`, `get_session_history`, and `update_settings`.
- Implemented `DashboardCommandService` (`src-tauri/src/commands/dashboard.rs`) and `SettingsCommandService` (`src-tauri/src/commands/settings.rs`) validating `CorrectionMode::{Natural, Gentle, Detailed, Off}` and `SpeakingSpeed`.
- Built `CorrectionModeSelector.tsx` (39 lines), `SessionHistoryPanel.tsx` (41 lines), updated `SpeakingRoom.tsx` (47 lines) and `tauriService.ts` (149 lines), and verified with `Phase7ReviewAndSettings.test.ts`.

### `T054`–`T059` — Privacy Verification, 30-Min Stability, Security Audit & Spec Convergence
- Implemented `RawAudioRetentionPolicy` enforcing non-persistence of raw audio unless explicitly enabled (`T054`).
- Added `src-tauri/src/modules/hardening_tests.rs` (186 lines) verifying raw audio non-persistence, 180-turn simulated 30-minute session memory/buffer stability (`T055`), stage timing instrumentation (`T056`), and IPC/localhost/redaction invariants (`T057`).
- Documented findings in `docs/performance-and-security-audit.md` and full Spec Kit traceability + post-MVP Tauri shell follow-ups (`T060`, `T061`) in `docs/spec-convergence-audit.md` (`T059`).
- Verified `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test --offline` (`58 passed; 0 failed; 3 ignored`), `npm test -- --run` (`14 passed`), `npm run typecheck`, `npm run build`, and `cargo build --release` (`T058`).




