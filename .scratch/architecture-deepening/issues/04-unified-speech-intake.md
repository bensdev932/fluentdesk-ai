# 04: Unified Speech Intake, Live Capture Pump & Direct SpeechToText Utterance

**What to build:** Consolidate VAD endpointing, push-to-talk press/release, bounded 16 kHz mono sample buffering, microphone device validation, and noise/hallucination filtering into one deep in-process **Speech Intake** module pumped actively from the live capture stream inside **Speaking Session** and passed straight into `SpeechToText` without intermediate chunk re-slicing.

**Blocked by:** 01: Authoritative Rust Speaking Session Host & SQLite Persistence

**Status:** ready-for-agent

- [x] Device ID validation and capture configuration live directly in **Speaking Session** / audio intake startup without an orphaned device command module.
- [x] One unified **Speech Intake** / **Turn Detector** seam supports both automatic energy VAD hangover and explicit push-to-talk press/release with a single maximum-duration enforcement policy.
- [x] Active **Speaking Session** capture pumps incoming audio chunks into **Speech Intake**, filters empty/hallucinated Whisper noise phrases back to `Listening`, and cancels cleanly on stop or pause.
- [x] `SpeechToText` accepts normalized 16 kHz mono **Utterance** payloads directly without intermediate 5-second chunk slicing (`into_bounded_chunks`) or redundant re-normalization (`collect_16khz_samples`).
- [x] Pipeline tests feed scripted audio chunks through `MockAudioInput` into **Speaking Session** / **Speech Intake** exercising VAD, push-to-talk, bounds, and noise suppression.
