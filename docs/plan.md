# Implementation Plan: Voice-First Conversation Loop

**Feature ID:** 001  
**Status:** Proposed — validate dependency versions and target OS before coding

## 1. Technical direction

Build a local-first desktop application with Tauri 2, a React + TypeScript UI, and Rust orchestration. Use adapter interfaces for microphone capture, VAD/turn detection, speech recognition, LLM inference, speech synthesis, and persistence.

The first vertical slice is deliberately narrow: one learner, one active session, English speech, end-of-turn detection, one local LLM provider, one local STT provider, one local TTS provider, transcript persistence, and session stop/recovery.

## 2. Proposed stack

| Concern | Candidate | Notes |
|---|---|---|
| Desktop shell | Tauri 2 | Confirm current platform permissions and IPC APIs |
| UI | React + TypeScript | Speaking room, device status, transcript and state display |
| Core | Rust | Session lifecycle and orchestration |
| Async runtime | Tokio | Cancellation-aware background tasks |
| Audio capture | CPAL | Verify target OS device support and callback constraints |
| Audio buffers | Typed PCM structures | Fixed sample format/rate at module boundary |
| VAD | WebRTC VAD or replaceable endpoint detector | Evaluate supported formats and licensing |
| STT | whisper.cpp via a maintained Rust binding or local sidecar | Keep integration behind `SpeechToText` adapter |
| LLM | Ollama local HTTP API initially | Use structured output where reliable; validate all output |
| Alternative LLM | llama.cpp server adapter | Add after first vertical slice |
| TTS | Local TTS adapter; evaluate Kokoro ONNX or another distributable engine | Confirm model and voice licenses before bundling |
| Persistence | SQLite + SQLx | Versioned migrations and parameterized queries |
| Serialization | Serde | Typed IPC and provider contracts |
| Errors | thiserror + anyhow at boundaries | Typed internal errors, user-safe messages |
| Observability | tracing | Never log raw audio, secrets, or unnecessary learner content |

Do not assume all components are pure Rust. A local sidecar process may be preferable for mature native inference engines. Sidecars must be versioned, started/stopped safely, bound to localhost, and included in packaging/licensing review.

## 3. Architecture boundaries

```text
React UI
  └── Tauri commands/events/channels
       └── Rust application service
            ├── SessionManager
            ├── AudioInput adapter (CPAL)
            ├── TurnDetector adapter (VAD / push-to-talk)
            ├── SpeechToText adapter (whisper.cpp)
            ├── AiProvider adapter (Ollama first)
            ├── TextToSpeech adapter (local engine)
            ├── AudioOutput adapter
            └── SessionRepository (SQLite)
```

The UI receives state changes and transcript/response events. The UI should not own model orchestration. Avoid passing large audio payloads as base64 through ordinary Tauri command arguments; use bounded channels or a documented streaming transport.

## 4. Core Rust domain types

Create explicit types before implementing adapters:

- `SessionId`
- `SessionState`: Idle, Initializing, Listening, Processing, Thinking, Speaking, Paused, Error, Ended
- `AudioFormat`: sample rate, channels, sample representation
- `AudioChunk`: bounded PCM data plus timing metadata
- `Transcript`: text, language, duration, optional confidence metadata
- `ConversationTurn`: role, text, timestamp, optional stage metadata
- `TutorReply`: spoken reply text, optional correction, optional follow-up prompt
- `CorrectionMode`: Natural, Gentle, Detailed, Off
- `ProviderHealth`: Available, Unavailable, Misconfigured
- `SessionError`: typed internal error category and safe user-facing description

Avoid exposing provider implementation types to frontend IPC.

## 5. Session orchestration

Use a single owner for session state transitions. Background operations must report events to that owner rather than mutate shared state ad hoc.

Suggested pipeline:

1. `start_session` validates configuration, initializes audio devices and required providers, then transitions to Listening.
2. Audio callback writes bounded chunks to a channel; it must not perform blocking inference or database operations.
3. Turn detector emits a bounded utterance or push-to-talk completion.
4. STT produces a transcript. Empty/low-quality output is handled without an LLM request.
5. Conversation manager builds context and requests a tutor response.
6. Response parser validates the result; malformed structured output falls back to safe plain text when possible.
7. TTS synthesizes the spoken reply.
8. Audio output plays the reply; the transcript/reply pair is persisted even if TTS fails.
9. After playback, transition to Listening if the session has not been cancelled.
10. `stop_session` cancels tasks, stops capture/playback, releases devices, and persists final session state.

## 6. IPC and events

Commands should be small and explicit:

- `start_speaking_session(config)`
- `stop_speaking_session(session_id)`
- `pause_speaking_session(session_id)`
- `resume_speaking_session(session_id)`
- `list_audio_input_devices()`
- `set_audio_input_device(device_id)`
- `get_session_history(session_id)`
- `get_session_status(session_id)`

Events/channels should report:

- `session_state_changed`
- `user_transcript_ready`
- `tutor_text_ready`
- `audio_playback_started`
- `audio_playback_finished`
- `session_error`

Do not create a broad command that accepts arbitrary shell commands, file paths, or provider URLs from the frontend.

## 7. Persistence

Initial tables:

- `users`
- `conversations`
- `conversation_turns`
- `session_settings`

Persist user transcript and tutor response text, role, timestamps, and optional non-sensitive duration metadata. Do not persist raw audio by default. Add schema migrations and indexes for conversation history.

## 8. Testing strategy

### Unit tests

- Session state transition rules.
- Audio format validation and conversion.
- Turn detector behavior with deterministic audio fixtures.
- Empty transcript filtering.
- Prompt/context assembly.
- Structured tutor response parsing and fallback.
- Cancellation and cleanup semantics.
- Repository ordering and persistence.

### Adapter contract tests

Run every mock and real adapter against a shared contract where feasible. Mock adapters are mandatory in regular CI. Real model-backed tests are opt-in and require documented model paths/services.

### Integration tests

- Audio fixture → VAD → mocked STT result.
- Mock STT → mocked LLM → mocked TTS → session state transitions.
- Ollama adapter against a locally available service.
- SQLite migration and repository behavior.

### Manual tests

- Real microphone and speaker/headset on each supported OS.
- Microphone permission denial.
- Device disconnect during a session.
- Stop during transcription, inference, synthesis, and playback.
- A 30-minute session to inspect memory/resource behavior.

## 9. Delivery phases

### Phase A — Application and domain foundation

Tauri shell, Rust modules, typed state machine, error model, tracing, SQLite migration, mock providers.

### Phase B — Audio capture and turn segmentation

Device enumeration, audio capture, bounded buffering, push-to-talk, then VAD if it can be delivered reliably.

### Phase C — Speech recognition

Whisper adapter, fixture tests, transcript UI, provider failure handling.

### Phase D — AI conversation

Ollama adapter, context management, tutor prompt, structured response validation, conversation persistence.

### Phase E — Speech synthesis and playback

Local TTS adapter, audio output, interruption/stop cleanup, return-to-listening loop.

### Phase F — Session review and hardening

History UI, privacy checks, packaging, offline verification, performance measurements.

## 10. Risks and mitigations

| Risk | Mitigation |
|---|---|
| Local model too slow on minimum hardware | Define hardware profiles; benchmark small quantized models; expose model selection |
| Native speech dependencies differ by OS | Keep adapters isolated; test target OS early; package sidecars deliberately |
| VAD cuts off slow learners | Configurable silence threshold; push-to-talk fallback |
| TTS engine distribution/license issue | Make TTS replaceable; review model/voice licenses before bundling |
| AI returns malformed structure | Validate schema; safe text fallback; adapter tests |
| Audio callback blocks | Callback only pushes bounded data; no network, database, or model work in callback |
| User audio leaks | No raw-audio persistence by default; explicit cloud consent; no audio logging |
| Long conversations exceed context | Summarize older context with testable policy; retain recent turns |

## 11. Definition of ready

Implementation may begin when:

- Open questions in `spec.md` are resolved or explicitly accepted as defaults.
- Target OS and minimum hardware profile are agreed.
- STT, LLM, and TTS candidates are checked for current compatibility and licensing.
- Each MVP requirement maps to tasks and tests.
- The repository's Tauri and Rust toolchain versions are pinned or documented.
