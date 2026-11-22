# Spec: Voice Research Upgrades — Streaming LLM-to-TTS Pipeline, 4-Phase Pedagogy Scaffold & Whisper Keyterm Biasing

Status: ready-for-agent
Labels: enhancement, ready-for-agent

---

## Problem Statement

Research on production AI language tutors (Duolingo Max, Speak) identifies three structural weaknesses in sequential voice loops:

1. **Sequential LLM $\to$ TTS Turn Latency:** Waiting for the full structured LLM reply before synthesizing audio accumulates 3–4 seconds of turn silence. Streaming sentences from `AiProvider` straight into `TextToSpeech` and SSE browser speech drops initial spoken playback latency dramatically.
2. **Unbounded Sessions & Pedagogical Drift:** Conversations without phase boundaries, CEFR targets, or natural wrap-up drift off-topic, hallucinate, and exhaust learners cognitively.
3. **Blind L2 Speech Recognition:** Calling `whisper-cli` without contextual keyterm biasing (`--prompt`) causes accented pronunciations of lesson vocabulary to be smoothed over into common native n-grams.

## Solution

Deliver three cohesive architectural upgrades sequentially from 1 to 3 behind existing module seams:

1. **Ticket 01 — Pipelined LLM Token Stream into TTS Playback:**
   - Extend `AiProvider` with a streaming reply interface yielding incremental `spokenReply` sentences as LLM tokens arrive plus the validated final `RoutedTutorReply`.
   - Implement an incremental streaming extractor (`SpokenJsonStreamParser`) handling JSON escapes, key ordering, plain-text fallback, and sentence boundary segmentation.
   - Support true token streaming in `OllamaAiProvider` (`stream: true`), `OpenAiCompatProvider` (SSE `stream: true`), and `GeminiProvider` (`streamGenerateContent?alt=sse`), routed through `TutorRouter`.
   - Connect `SpeakingSessionHost` spoken loop concurrently so Sentence 1 synthesizes and plays via `TextToSpeech` + `AudioOutput` immediately while the LLM completes remaining sentences and coaching feedback, with full cancellation and text-only degradation.
   - Add SSE turn streaming (`POST /api/sessions/:id/turn/stream`) on `local_server` and sentence-queued speech synthesis in `speakingSessionClient.ts` so browser emulator turns also begin speaking early.

2. **Ticket 02 — Bound Speaking Sessions with 4-Phase Pedagogical Scaffold:**
   - Introduce domain types `CefrLevel`, `SessionPhase` (`Opener`, `FirstQuestion`, `TopicExchange`, `Closer`, `Completed`), `SessionTopic`, and `MaxSessionTurns` with SQLite migration `003_pedagogical_scaffolding.sql` persisting default `cefr_level` in **Learner Preferences**.
   - Deepen `SessionManager` / `SpeakingSessionHost` to track `user_turn_count`, `cefr_level`, `topic`, `target_vocabulary`, and automatic stage advancement through `Closer`.
   - Upgrade `TutorPrompt` assembly (`src-tauri/src/modules/tutor/prompts.rs`) with the **Three-Character Paradigm** (`System` curriculum/CEFR rules, `Assistant` persona, `User` turn) and phase-specific guidance (including learner-confusion/L1 adaptation hints and wrap-up without follow-up on `Closer`).
   - Surface compact session phase and turn progress (`phase`, `turnCount`, `maxTurns`, `cefrLevel`, `topic`, `sessionCompleted`) cleanly in status snapshots and the Speaking Room UI.

3. **Ticket 03 — Keyterm Vocabulary Biasing (`--prompt`) for Local Whisper STT:**
   - Introduce validated `SttPromptContext` holding sanitized `topic` and `keyterms` bounded to Whisper's initial-prompt token window.
   - Extend `SpeechToText` with `transcribe_with_context` (backwards-compatible default delegating to `transcribe`), passing `--prompt <TEXT>` into `whisper-cli` via `LocalWhisperAdapter`.
   - Wire active session topic + target vocabulary + recent tutor keywords from `SpeakingSessionHost` into **Speech Intake** $\to$ `SpeechToText::transcribe_with_context`, so local Whisper biases its beam search toward lesson vocabulary on non-native speech.

## Compliance Constraints

- Strict adherence to `rules.md` (`<= 200` lines/file, `<= 10` lines/method, `<= 50` lines/React component, value objects, zero `unsafe`).
- Strict TDD (`red -> green -> refactor`) for every ticket, verified via `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`, `npm run typecheck`, `npm test`, and `npm run build`.
