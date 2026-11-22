# 01: Pipelined LLM Token Stream into TTS Playback

**What to build:** Stream incremental `spokenReply` sentences out of `AiProvider` as LLM tokens arrive, flush them concurrently across `TextToSpeech` + `AudioOutput` playback, and expose SSE turn streaming (`POST /api/sessions/:id/turn/stream`) for the browser emulator so learners hear Sentence 1 before the LLM finishes generating remaining sentences and coaching feedback.

**Status:** ready-for-human

- [x] Add incremental `SpokenJsonStreamParser` (`modules/tutor/`) handling JSON string unescaping (`\"`, `\\`, `\n`, `\uXXXX`), `spokenReply` / `spoken_reply` key detection, plain-text fallback, and sentence boundary segmentation (`.`, `!`, `?` + whitespace/end).
- [x] Extend `AiProvider` (`ai/provider.rs`) with `TutorStreamItem` (`Sentence(String)`, `Final(RoutedTutorReply)`) and `stream_routed_reply` (with a backwards-compatible default adapter wrapping `generate_routed_reply`).
- [x] Add real streaming HTTP transport and SSE/NDJSON decoding across `OllamaAiProvider` (`stream: true`), `OpenAiCompatProvider` (`stream: true` via `curl --no-buffer` / `TcpStream`), `GeminiProvider` (`streamGenerateContent?alt=sse`), and `TutorRouter` (preserving health tracking & failover).
- [x] Pipeline `SpeakingSessionHost::run_spoken_ai_and_tts` & `modules/speech/synthesis_stream.rs` so Sentence 1 synthesizes and plays concurrently while the LLM completes subsequent tokens/corrections, with full cooperative cancellation (`stop_playback`) and graceful text-only degradation.
- [x] Add `POST /api/sessions/:id/turn` (`Accept: text/event-stream`) & `/turn/stream` SSE endpoint in `local_server` and sentence-queued Web Speech playback in `speakingSessionClient.ts` / `speakingSessionStream.ts` with transparent JSON fallback.
- [x] All new/existing unit & integration tests, `cargo fmt`, `cargo clippy -- -D warnings`, `cargo test`, `npm run typecheck`, and `npm test` pass within `rules.md` limits.
