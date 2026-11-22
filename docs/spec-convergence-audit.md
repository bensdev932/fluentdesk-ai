# Spec Kit Convergence Audit (`T059`)

Comparison of every functional and quality requirement in `docs/spec.md` against the implemented codebase and verified automated tests after Phase 7.

| Requirement | Implementation Location | Automated Verification | Manual / Follow-Up Gap |
|---|---|---|---|
| **FR-001** Start & end voice session | `SessionManager::{start, stop}`, `ConversationCommandService`, `SpeakingRoom.tsx` | `pipeline_tests::start_pause_resume_and_stop_session_commands_manage_audio_truthfully`, `SpeakingRoom.test.ts` | Live desktop window smoke run (`npm run tauri dev`) |
| **FR-002** Truthful states (`Idle..Error`) | `SessionState`, `SessionStateBadge.tsx`, `sessionStatePresentation.ts` | `SessionStateBadge.test.ts`, `domain::tests::session_state_reports_microphone_and_active_status_truthfully` | None in automated suite |
| **FR-003 / FR-016** Capture, VAD endpointing & push-to-talk | `NonBlockingAudioSink`, `EnergyTurnDetector`, `PushToTalkController`, `SpeakingControls.tsx` | `vad_and_ptt_tests.rs`, `conversion_tests.rs`, `SpeakingRoom.test.ts` | Physical mic WASAPI hot-plug test (`docs/manual-audio-test-checklist.md`) |
| **FR-004 / FR-005** Local STT & empty/noise filter | `LocalWhisperAdapter`, `CliWhisperSidecarRunner`, `TranscriptionCoordinator` | `stt::tests.rs` (5 unit/fixture tests) | Opt-in `real_whisper_cli_transcribes_wav_fixture_when_configured` |
| **FR-006 / FR-015** Bounded context + AI provider, voice-first | `AiProvider`, `OllamaAiProvider`, `BoundedConversationContext`, `VoiceTurnCoordinator` | `ai::tests.rs`, `modules::tutor::tests.rs` | Opt-in `real_local_ollama_generates_tutor_reply_when_available` |
| **FR-007 / FR-008** Spoken TTS reply + return to Listening | `LocalKokoroTtsAdapter`, `SpeechPlaybackCoordinator`, `SessionManager::speak_tutor_reply` | `tts::tests.rs` (5 unit/integration tests) | Opt-in `real_local_tts_synthesizes_audio_when_configured` |
| **FR-009** Chronological persistence & session review | `SessionRepository`, `DashboardCommandService`, `SessionHistoryPanel.tsx` | `database::tests.rs`, `hardening_tests::t052_...`, `Phase7ReviewAndSettings.test.ts` | None in automated suite |
| **FR-010** Immediate stop & resource release | `SessionCancellationToken`, `SessionManager::stop`, `EndToEndVoiceLoop::stop_immediately` | `pipeline_tests::stop_session_cancels_and_cleans_up_during_every_pipeline_stage`, `e2e_tests.rs` | None |
| **FR-011** Recoverable mic/STT/LLM/TTS errors | `SessionError`, `EndToEndVoiceLoop::recover_to_listening`, `TutorPlaybackOutcome::TextOnlyDegraded` | `e2e_tests::recovery_from_microphone_and_stt_failures_restores_listening_state`, `recovery_from_llm_and_tts_failures_...` | Physical hardware disconnect test (`docs/manual-audio-test-checklist.md`) |
| **FR-012 / FR-014** No raw audio by default; cloud opt-in off | `001_initial_schema.sql`, `UserVoiceSettings`, `RawAudioRetentionPolicy` | `hardening_tests::t054_...`, `database::tests::migrations_are_idempotent_...` | None |
| **FR-013** Offline local-first operation | `LocalWhisperAdapter`, `OllamaAiProvider`, `LocalKokoroTtsAdapter`, `EndToEndVoiceLoop` | `e2e_tests::offline_end_to_end_multi_turn_loop_completes_and_returns_to_listening` | Physical Airplane-Mode E2E test (`docs/manual-e2e-test-checklist.md`) |
| **FR-017** Correction modes (`Natural`, `Gentle`, `Detailed`, `Off`) | `CorrectionMode`, `SettingsCommandService`, `CorrectionModeSelector.tsx` | `tutor::tests.rs`, `hardening_tests::t053_...`, `Phase7ReviewAndSettings.test.ts` | None |

## Follow-Up Backlog Tasks Identified During Convergence

- **[ ] T060 (Post-MVP Desktop Shell Binding):** Wire `tauri::Builder::default().invoke_handler(...)` in a dedicated `tauri-app` binary target once system WebView2 / `tauri` crate dependencies are pulled online into Cargo cache (currently core library + Vite frontend are decoupled behind `TauriVoiceIpcClient` so tests run offline cleanly).
- **[ ] T061 (Hardware Profiling Run):** Record real-hardware millisecond stage timings on target Windows 11 profile once local `whisper-cli`, `ollama`, and `kokoro-tts` binaries and models are installed on the host.
