# 03: Keyterm Vocabulary Biasing (`--prompt`) for Local Whisper STT

**What to build:** Bias Whisper's decoder beam search toward active lesson target vocabulary and conversational keywords via `--prompt <TEXT>` across the `SpeechToText` adapter seam, reducing non-native recognition drift without modifying model weights.

**Blocked by:** 02: Bound Speaking Sessions with 4-Phase Pedagogical Scaffold

**Status:** ready-for-human

- [x] Add `SttPromptContext` value object (`stt/provider.rs` or `domain/audio.rs`) encapsulating sanitized `topic` and bounded `keyterms` formatted into a safe Whisper initial prompt (`<= 224` chars, control characters stripped).
- [x] Extend `SpeechToText` trait with `transcribe_with_context(&self, utterance: &CompletedUtterance, context: Option<&SttPromptContext>)` (with default fallback to `transcribe` for legacy mocks).
- [x] Update `LocalWhisperAdapter` (`stt/whisper.rs`, `stt/sidecar.rs`) to pass `["--prompt", text]` to `whisper-cli` when `SttPromptContext` yields a non-empty prompt, verified via `FakeWhisperRunner`.
- [x] Wire active **Speaking Session** `topic`, `target_vocabulary`, and recent tutor turn terms from `SpeakingSessionHost` into **Speech Intake** -> `SpeechToText::transcribe_with_context`.
- [x] Verify all Rust & frontend tests, formatting, clippy (`-D warnings`), and builds pass within `rules.md` bounds.
