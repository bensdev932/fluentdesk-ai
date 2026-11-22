# Spec: Architecture Deepening — Unified Speaking Session, Speech Intake, Learner Preferences & Speech Adapters

Status: ready-for-agent
Labels: enhancement, ready-for-agent

---

## Problem Statement

Learners running FluentDesk AI in browser or desktop mode currently hit split-brain behavior and silent reliability gaps across the **Speaking Session** lifecycle, audio intake, preferences, and local speech synthesis:

1. **Disjointed sessions and lost persistence:** Active browser sessions run on a separate in-memory client state machine that skips lifecycle states, uses a different default **Correction Mode**, ignores push-to-talk settings, drops saved coaching notes from session review, and sends stateless batches of up to 50 **Conversation Turn** entries per request instead of reusing the persistent Rust **Speaking Session** and SQLite history.
2. **Unwired audio capture and wasted resampling:** Live audio capture streams are opened during a **Speaking Session** without being drained into any **Turn Detector**; microphone selection is stored in an unreferenced module; and completed **Utterance** buffers are normalized into 16 kHz mono audio, immediately re-sliced into 5-second chunks, and then concatenated back into the exact same 16 kHz buffer before Whisper runs.
3. **Fragmented preferences and duplicated consent rules:** Learner settings and cloud opt-in state are stored and enforced in three separate places—SQLite session settings, a standalone JSON preferences file next to the encrypted key vault, and duplicated React hook logic—so updates from one flow fail to synchronize with the other.
4. **High latency and brittle local speech tests:** Spoken **Tutor Reply** synthesis runs all sentence processes sequentially and concatenates every decoded waveform into a single buffer before playback begins, while public helper re-exports encourage tests to bypass the speech recognition and speech synthesis interfaces instead of exercising real adapter behavior.

## Solution

Deepen the four identified module clusters sequentially behind clean, minimal seams so callers interact with deep, reliable backend modules through unified ports:

1. **Unify the Speaking Session host and port:** Collapse the shallow session command pass-through and duplicated frontend session store into a single Rust-owned **Speaking Session** module wired into SQLite persistence and the **Tutor Router**, exposed over a single typed **Speaking Session** port used by the UI in both live and offline fallback modes.
2. **Collapse Speech Intake and Turn Detector:** Merge voice activity detection, push-to-talk control, bounded 16 kHz buffering, device validation, and noise/hallucination filtering into one deep **Speech Intake** module actively pumped from the live capture stream and connected straight to `SpeechToText` with zero re-slicing.
3. **Consolidate Learner Preferences & Consent behind SQLite:** Move all non-secret learner and routing settings—including **Correction Mode**, speaking speed, push-to-talk toggle, raw audio retention opt-in, cloud opt-in, active provider preference, and per-provider model overrides—into a single SQLite-backed **Learner Preferences** module, isolating the encrypted DPAPI vault strictly to API keys and removing duplicated consent enforcement from frontend hooks.
4. **Hide pass-through speech helpers & stream sentence synthesis:** Make subprocess execution a private adapter detail behind `SpeechToText` and `TextToSpeech`, remove pass-through helper re-exports, and stream synthesized sentence audio chunks straight to playback with sentence-level cancellation so learners hear responses sooner.

## User Stories

1. As a learner, I want my active **Speaking Session** state and transitions (`Idle`, `Initializing`, `Listening`, `Processing`, `Thinking`, `Speaking`, `Paused`, `Error`, `Ended`) enforced in one authoritative place, so that the UI never displays a misleading microphone or paused state.
2. As a learner, I want my default **Correction Mode** (`Natural`) to match across every launch mode, so that grammar coaching does not unexpectedly shift between browser and desktop runs.
3. As a learner, I want every **Conversation Turn**—including both user speech and **Tutor Reply** text, grammar corrections, and follow-up prompts—persisted to local SQLite as turns complete, so that refreshing or reopening the studio preserves my full study record.
4. As a learner opening session history review, I want saved grammar coaching notes and follow-up prompts attached to prior **Tutor Reply** turns, so that I can revisit feedback from previous speaking sessions.
5. As a learner, I want active turns in a **Speaking Session** to read chronological context directly from backend persistence rather than re-uploading full turn arrays on every utterance, so that long sessions stay fast and consistent.
6. As a learner whose backend bridge is temporarily unavailable, I want the frontend adapter to fall back explicitly to offline coaching with a visible warning rather than silently maintaining fake persisted sessions.
7. As a learner toggling push-to-talk when starting a **Speaking Session**, I want the backend **Turn Detector** to honor push-to-talk mode automatically rather than ignoring that flag.
8. As a learner speaking into an active microphone stream, I want captured audio chunks continuously fed into the **Turn Detector** until speech completes or I stop, so that voice activity detection drives real spoken turns automatically.
9. As a learner pressing and releasing push-to-talk, I want the **Turn Detector** interface itself to support press, hold, cap enforcement, and release without requiring callers to bypass the interface.
10. As a learner selecting a specific microphone input device, I want device validation and active device selection owned once by the **Speaking Session** intake flow, so that chosen microphones never diverge from what capture opens.
11. As a learner finishing an **Utterance**, I want the normalized 16 kHz mono speech buffer passed straight to `SpeechToText` without redundant 5-second chunk splitting and re-merging, so that recognition incurs zero unnecessary sample-copy overhead.
12. As a learner whose spoken segment contains only ambient noise or known Whisper artifacts, I want empty or hallucinated audio filtered cleanly before hitting the **Tutor Router**, returning the session straight to `Listening`.
13. As a learner updating **Correction Mode**, speaking speed, push-to-talk, raw-audio recording opt-in, cloud opt-in, active provider, or model selections, I want all preferences persisted reliably in one SQLite store, so that closing and restarting the backend keeps every setting synchronized.
14. As a learner on a fresh installation, I want cloud inference opt-in (`cloud_opt_in`) and raw audio retention (`raw_audio_recording_opt_in`) to default strictly to `false`, so that my voice and transcripts never leave my device or get stored as raw audio without explicit consent.
15. As a learner turning **Cloud AI Opt-In** off while a cloud provider was pinned, I want the backend preferences module to automatically reset my active provider back to `Auto` (`Ollama` local mode) on the server, so that client and backend states never disagree.
16. As a learner configuring free cloud API keys (`Groq`, `Gemini`, `OpenRouter`), I want the DPAPI **Credential Vault** to store only encrypted secrets on disk and never mix non-secret preferences into secret storage or expose keys in HTTP payloads or logs.
17. As a learner trying an invalid speaking speed outside safe bounds, I want validation enforced consistently inside the domain preferences type and rejected at the seam before persisting invalid values.
18. As a learner listening to multi-sentence spoken responses from local speech synthesis, I want playback of the first sentence to start immediately as soon as sentence one is synthesized rather than waiting for all remaining sentences to finish first.
19. As a learner clicking Stop Session or interrupting playback mid-reply, I want sentence-by-sentence synthesis and playback to cancel immediately before spawning subsequent sentence subprocesses, so audio stops promptly and releases resources.
20. As a maintainer, I want all external behavior across **Speaking Session**, **Speech Intake**, **Learner Preferences**, and speech adapters tested through their top-level interfaces with mock/in-memory adapters, so internal helper refactors never break brittle tests.

## Implementation Decisions

- **Authoritative Rust Speaking Session Host (Candidate 01):**
  - Merge the shallow session command pass-through module into the Rust **Speaking Session** orchestrator and wire a session registry backed by SQLite repositories and the shared **Tutor Router** into the localhost HTTP bridge host.
  - Eliminate test-only stage override methods from the production session interface; retain single-owner active session ID verification directly inside the **Speaking Session** module.
  - Honor `push_to_talk` configuration during session startup so the session creates the appropriate **Turn Detector** policy from the start configuration.
- **Unified Frontend Speaking Session Port & Contract Sync (Candidate 01):**
  - Replace the split `ipcClient` plus optional `emulator` UI props with a single narrow **Speaking Session** port (`startSpeakingSession`, `stopSpeakingSession`, `pauseSpeakingSession`, `resumeSpeakingSession`, `submitUtteranceText`, `simulateSampleTurn`, `listAudioInputDevices`, `setAudioInputDevice`, `listSessionHistory`, `getSessionHistory`, `updatePreferences`, `getSessionStatus`, `subscribeVoiceEvents`).
  - Implement a production HTTP adapter backed by localhost session routes (`/api/sessions` lifecycle, turn submission, and history endpoints alongside `/api/providers`), with browser Web Speech synthesis/recognition strictly attached as audio I/O adapters and offline local coaching activated only when backend HTTP calls fail.
  - Delete the duplicated client session state map, artificial sleep delays, client-only state transition bypasses, unused scripted reply arrays, and orphaned turn-pair builder utilities.
  - Include optional `correction` and `followUpPrompt` strings on the frontend **Conversation Turn** model so SQLite-persisted coaching details render cleanly in session history reviews.
- **Deep In-Process Speech Intake & Zero-Churn `SpeechToText` Contract (Candidate 02):**
  - Consolidate audio device validation, energy VAD endpointing, push-to-talk activation/release, single maximum-duration enforcement, bounded 16 kHz mono PCM buffering, and hallucination filtering behind a unified **Speech Intake** module driven by the **Speaking Session** capture loop.
  - Delete the standalone audio device command wrapper and fold device ID validation directly into **Speaking Session** capture startup.
  - Update the `SpeechToText` trait interface so `transcribe` accepts a completed, normalized 16 kHz mono **Utterance** directly, deleting intermediate 5-second chunk re-slicing and duplicate PCM re-normalization inside the Whisper adapter.
  - Pump the live `capture_rx` audio stream inside **Speaking Session** into **Speech Intake** cleanly until a turn completes, errors, or cancels via the session cancellation token.
- **Consolidated SQLite Learner Preferences & Isolated DPAPI Vault (Candidate 03):**
  - Extend the SQLite settings table schema via a forward idempotent migration to persist all non-secret routing preferences (`preferred_provider`, `model_overrides_json`) alongside `correction_mode`, `speaking_speed`, `push_to_talk_enabled`, `cloud_opt_in`, and `raw_audio_recording_opt_in`.
  - Provide a single backend **Learner Preferences** seam (`get_preferences`, `patch_preferences`) consumed by **Speaking Session**, **Tutor Router**, and HTTP preference endpoints; keep privacy defaults (`cloud_opt_in = false`, `raw_audio_recording_opt_in = false`, `correction_mode = Natural`) and server-side auto-reset (`cloud_opt_in = false` resets pinned cloud providers back to `Auto`) inside this module.
  - Remove `PersistedRouterPrefs` JSON storage so the Windows DPAPI **Credential Vault** stores encrypted API secrets exclusively while retaining `Saved (Vault) > Env > None` precedence; remove shallow settings/consent command wrappers and enforce `RawAudioRetentionPolicy` where raw audio retention could occur; validate `SpeakingSpeed` inside domain preferences validation.
  - Simplify the React provider settings hook into a thin adapter forwarding patches and rendering backend preference snapshots without re-implementing consent gating rules client-side.
- **Encapsulated Speech Subprocess Adapters & Streaming Sentence Synthesis (Candidate 04):**
  - Make local CLI subprocess helpers private implementation details of the STT and TTS adapters, deleting public WAV codec pass-through exports and moving all PCM/WAV encoding/decoding solely into the shared audio conversion module.
  - Introduce a narrow internal process-runner trait injected into the local Whisper and Kokoro adapters so adapter behavior (exit codes, malformed/empty output, argument construction) can be exercised deterministically without spawning live binaries.
  - Extend the TTS synthesis seam and playback coordinator so `LocalKokoroTtsAdapter` synthesizes sentence by sentence into a bounded channel consumed concurrently by `AudioOutput`, enabling playback to begin on sentence 1 while checking session cancellation between each sentence instead of stitching one monolithic full-reply PCM chunk upfront.
- **Compliance Constraints:**
  - Preserve existing engineering standards across every edited or new module (`<= 200` lines per source file, `<= 50` lines per React UI module, focused small Rust helpers, zero `unsafe` code, zero secrets logged or exposed over IPC/HTTP).

## Testing Decisions

- **Core Testing Principle:** Write tests exclusively against each deepened module's public seam, asserting on observable outputs, state snapshots, persisted records, and adapter interactions rather than reaching into internal helpers or duplicating unit tests on deleted pass-throughs.
- **Modules Under Test & Prior Art:**
  - **Speaking Session + HTTP Seam (Candidate 01):**
    - Exercise full lifecycle (`start` -> `Listening` -> spoken turn submission -> **Tutor Reply** + coaching persistence -> `pause`/`resume` -> `stop` -> `listSessionHistory`/`getSessionHistory`) against `sqlite::memory:` and mock AI/STT/TTS/audio adapters (mirroring existing Rust end-to-end session tests and localhost bridge loopback tests).
    - Exercise the frontend **Speaking Room** controller and UI against a single in-memory **Speaking Session** port fake (replacing legacy split emulator-only tests while keeping offline HTTP failure recovery covered).
  - **Speech Intake + Capture-to-STT Seam (Candidate 02):**
    - Feed deterministic `AudioChunk` streams through `MockAudioInput` into **Speech Intake** and **Speaking Session**, asserting VAD hangover completion, push-to-talk press/release completion, noise hallucination suppression, max-duration capping, and direct `SpeechToText` handoff without intermediate re-chunking assertions.
  - **Learner Preferences & Consent Seam (Candidate 03):**
    - Test validation errors (`SpeakingSpeed`), defaults, idempotent SQLite schema migration, server-side `cloud_opt_in = false` automatic provider reset, and cross-restart persistence using `sqlite::memory:` paired with **Tutor Router** and isolated temporary DPAPI vaults (consolidating prior hardening/vault preference tests and removing redundant React consent-logic tests).
  - **Local Speech Adapters & Streaming Playback Seam (Candidate 04):**
    - Exercise Whisper STT and Kokoro TTS adapters directly through `SpeechToText` and `TextToSpeech` using a deterministic in-memory process-runner fake (covering multi-sentence streaming playback ordering, mid-reply cancellation stopping later sentences, non-zero CLI failures, and truncated WAV errors), keeping raw WAV header codec tests in audio conversion unit tests and keeping live-binary hardware tests gated (`#[ignore]`).

## Out of Scope

- Changing the core voice-first user journey into a text-first quiz or chat interface.
- Shipping bundled Whisper or Kokoro binary installers or adding GPU/phoneme-level pronunciation scoring.
- Replacing OS-level Windows DPAPI secret encryption inside **Credential Vault** with third-party cloud key vaults or cross-device cloud sync.
- Altering curated 2026 cloud model presets (`Groq`, `Gemini`, `OpenRouter`) or round-robin cloud failover behavior already verified on **Tutor Router**.
- Tauri desktop bundler installer/packaging configuration changes outside the core Rust crate and localhost/Web UI bridge.

## Further Notes

- Execute sequentially from **01 → 04** in focused vertical slices: landing **Candidate 01** first connects the live UI straight to SQLite persistence and unlocks seamless end-to-end wiring for **Candidates 02** and **03**, followed by **Candidate 04** for lower time-to-first-audio and cleaner adapter testability.
- Existing `#[ignore]` hardware/service integration tests (local Ollama, `whisper-cli`, local TTS binaries) remain gated behind explicit environment configuration in offline CI.
- Any new domain terms introduced during implementation (e.g., if **Speech Intake** or **Learner Preferences** crystallize as first-class user-facing domain terms) should be added to the root domain glossary cleanly.
