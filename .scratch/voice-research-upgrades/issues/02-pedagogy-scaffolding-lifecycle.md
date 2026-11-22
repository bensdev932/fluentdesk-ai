# 02: Bound Speaking Sessions with 4-Phase Pedagogical Scaffold

**What to build:** Track every **Speaking Session** through a structured 4-phase pedagogical scaffold (`Opener` -> `FirstQuestion` -> `TopicExchange` -> `Closer` -> `Completed`) anchored to CEFR proficiency, lesson topic/vocabulary, and bounded turn limits, surfaced through `SessionStatusSnapshot` and the Speaking Room UI.

**Blocked by:** 01: Pipelined LLM Token Stream into TTS Playback

**Status:** ready-for-human

- [x] Add domain value objects (`CefrLevel`, `SessionPhase`, `MaxSessionTurns`, `SessionTopic`) and persist `cefr_level` via idempotent SQLite migration `003_pedagogical_scaffolding.sql`, `mappers.rs`, and `LearnerPreferencesStore`.
- [x] Extend `StartSpeakingSessionConfig`, `SessionManager`, and `SessionStatusSnapshot` (plus HTTP DTOs) to track `phase`, `turn_count`, `max_turns`, `cefr_level`, `topic`, and `target_vocabulary`, enforcing bounds when the `Closer` turn completes.
- [x] Upgrade `modules/tutor/prompts.rs` with the **Three-Character Paradigm** (`System` curriculum/CEFR/confusion adaptation rules, `Assistant` persona, `User` turn) and phase-specific instructions (`Opener`, `FirstQuestion` using target vocabulary, `TopicExchange`, `Closer` wrapping up cleanly without follow-up question).
- [x] Update frontend domain types, session client codec, and Speaking Room UI (`<= 50` lines/component) to display live phase/turn progress (`CEFR` + `Turn N / Max`) and allow configuring `cefrLevel` and `topic` when idle or restarting.
- [x] All unit/integration tests (`sqlite::memory:`, lifecycle, prompts, frontend), linters, and builds pass within `rules.md` limits.
