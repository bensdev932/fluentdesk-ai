# 05: Private Speech Process Adapters & Streaming Sentence Synthesis Playback

**What to build:** Encapsulate Whisper/Kokoro subprocess execution behind private, testable adapter seams within the STT and TTS modules (deleting public pass-through codec wrappers) and stream multi-sentence **Tutor Reply** synthesis sentence-by-sentence directly into audio playback with prompt inter-sentence cancellation.

**Blocked by:** 04: Unified Speech Intake, Live Capture Pump & Direct SpeechToText Utterance

**Status:** ready-for-agent

- [x] Public CLI helper modules are hidden as internal implementation details with redundant PCM/WAV pass-through re-exports removed; all WAV encoding/decoding stays in the audio conversion module.
- [x] Local Whisper and Kokoro adapters depend on a private injectable process-runner trait so deterministic unit tests verify exit errors, malformed output, and CLI arguments directly through `SpeechToText` and `TextToSpeech`.
- [x] Multi-sentence **Tutor Reply** synthesis streams each sentence `AudioChunk` sequentially into playback as soon as it is ready rather than blocking to concatenate a full-reply waveform before first playback sample.
- [x] Session cancellation between sentences aborts remaining sentence subprocess calls immediately while preserving graceful text-only degradation if playback/synthesis fails mid-turn.
- [x] Rust formatting, clippy (`-D warnings`), full offline test suite, and binary build pass within all `rules.md` line and complexity bounds.
