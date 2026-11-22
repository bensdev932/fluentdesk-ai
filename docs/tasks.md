# Tasks: Voice-First Conversation Loop

**Feature ID:** 001  
**Status:** Proposed task breakdown  
**Rule:** Complete tasks in dependency order. Each task must map to one or more acceptance scenarios or quality requirements.

## Phase 0 — Repository foundation

- [x] T001 Inspect repository, current toolchain, target OS, and existing user changes. Record findings; do not overwrite existing files.
- [x] T002 Establish Rust workspace/module boundaries for session orchestration, audio, STT, AI, TTS, persistence, and IPC.
- [x] T003 Add shared typed domain models for session state, audio format, transcript, conversation turn, tutor reply, correction mode, and errors.
- [x] T004 Add tracing with redaction rules; test that secrets and raw audio are never logged.
- [x] T005 Add SQLite migration framework and initial user/session/turn schema.
- [x] T006 Add mock implementations for audio, STT, LLM, TTS, and playback interfaces.
- [x] T007 Add CI checks for format, clippy, unit tests, and frontend tests.

## Phase 1 — Session lifecycle and state machine

- [x] T008 Write unit tests for valid and invalid session state transitions.
- [x] T009 Implement single-owner session state manager.
- [x] T010 Implement start, pause, resume, and stop session commands.
- [x] T011 Implement cancellation tokens and deterministic task cleanup on stop.
- [x] T012 Add UI state display for Idle, Initializing, Listening, Processing, Thinking, Speaking, Paused, and Error.
- [x] T013 Add integration tests for start/stop and cancellation during each pipeline stage.

## Phase 2 — Audio capture and turn segmentation

- [x] T014 Define audio adapter contract and validate supported sample formats.
- [x] T015 Write deterministic tests for bounded audio buffers and sample-rate/channel conversion.
- [x] T016 Implement audio input device enumeration and selection.
- [x] T017 Implement microphone capture with bounded channel handoff; keep callback non-blocking.
- [x] T018 Implement push-to-talk utterance capture and maximum-duration handling.
- [x] T019 Add VAD/endpoint detector behind a replaceable interface.
- [x] T020 Test silence, background noise, long pauses, quiet speech, and device disconnect using fixtures/mocks.
- [x] T021 Add manual microphone permission and device testing on the initial target OS.

## Phase 3 — Speech recognition

- [x] T022 Define STT adapter contract and transcript result schema.
- [x] T023 Write tests for empty transcript filtering, invalid audio, and recognizer errors.
- [x] T024 Integrate local whisper.cpp through a Rust binding or controlled local sidecar; document the selected integration.
- [x] T025 Add model-path/configuration validation and STT health check.
- [x] T026 Connect completed utterances to STT and emit transcript-ready events.
- [x] T027 Add transcript panel and clear distinction between recognized text and AI-generated text.
- [x] T028 Add fixture-based transcription tests and opt-in real-model tests.

## Phase 4 — AI conversation engine

- [x] T029 Define AI provider contract with bounded context and cancellation support.
- [x] T030 Write tests for context ordering, context limits, correction modes, and tutor prompt assembly.
- [x] T031 Implement Ollama local provider adapter and health/model checks.
- [x] T032 Implement structured tutor response parsing and plain-text fallback.
- [x] T033 Implement voice-first tutor instructions for natural conversation and gentle corrections.
- [x] T034 Connect transcript → context → LLM → tutor response event.
- [x] T035 Persist completed user transcript and tutor text in chronological order.
- [x] T036 Add mock-driven integration tests for relevant follow-up, context retention, provider failure, and malformed response.
- [x] T037 Add opt-in local Ollama integration test with documented model prerequisites.

## Phase 5 — Text-to-speech and playback

- [x] T038 Define TTS and audio output contracts with cancellation support.
- [x] T039 Add tests for synthesis failure, long text, invalid audio output, and playback cancellation.
- [x] T040 Evaluate and select a distributable local TTS engine; record model/voice license and platform constraints.
- [x] T041 Implement selected local TTS adapter.
- [x] T042 Implement audio output playback and device error recovery.
- [x] T043 Connect tutor text → TTS → playback and emit playback lifecycle events.
- [x] T044 Return session to Listening after playback if session remains active.
- [x] T045 Add tests for TTS failure preserving visible tutor text and not losing the transcript.

## Phase 6 — End-to-end speaking experience

- [x] T046 Build Speaking Room UI with truthful state, microphone control, push-to-talk, transcript, and tutor reply.
- [x] T047 Connect UI to typed Tauri commands and event/channel updates.
- [x] T048 Ensure stop control cancels capture, inference where possible, TTS, and playback immediately.
- [x] T049 Add manual end-to-end test: speak → transcript → AI reply → audible response → next listening turn.
- [x] T050 Add offline end-to-end test with local models already installed and network disabled.
- [x] T051 Add recovery tests for microphone, STT, LLM, and TTS failure paths.

## Phase 7 — Session review and hardening

- [x] T052 Build session history list and chronological transcript review.
- [x] T053 Add correction mode settings: Natural, Gentle, Detailed, Off.
- [x] T054 Verify raw audio is not persisted by default; add explicit recording consent only if separately approved.
- [x] T055 Test resource usage and memory stability during a 30-minute session.
- [x] T056 Measure stage latency on the agreed minimum hardware profile and document results.
- [x] T057 Review IPC permissions, local endpoint binding, secret handling, and log redaction.
- [x] T058 Run format, clippy, unit, integration, frontend, and packaging checks.
- [x] T059 Perform Spec Kit convergence: compare code and tests against every requirement and add tasks for gaps.

## Traceability

| Requirement | Primary tasks |
|---|---|
| FR-001, FR-002, FR-010 | T008–T013, T046–T048 |
| FR-003, FR-016 | T014–T021 |
| FR-004, FR-005 | T022–T028 |
| FR-006, FR-015 | T029–T037 |
| FR-007, FR-008 | T038–T045 |
| FR-009 | T035, T052 |
| FR-011 | T013, T023, T036, T039, T045, T051 |
| FR-012, FR-014 | T004, T054, T057 |
| FR-013 | T031, T037, T050 |
| FR-017 | T030, T033, T053 |
