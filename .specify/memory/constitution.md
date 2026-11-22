# FluentDesk AI Constitution

**Version:** 1.0.0  
**Status:** Binding project principles

These principles apply to every specification, design, implementation, test, code review, and release. If a task conflicts with this constitution, stop and propose an explicit amendment rather than silently violating it.

## 1. Voice-first learning is the product

- The core user journey MUST be spoken conversation, not text chat, worksheets, fill-in-the-blank exercises, or multiple-choice quizzes.
- A learner MUST be able to speak, receive a spoken AI reply, and continue the conversation without typing each turn.
- Text transcripts and correction cards are supporting accessibility and learning aids.
- New features MUST reinforce speaking confidence, listening comprehension, conversational fluency, or useful feedback.

## 2. Local-first, free-first architecture

- The complete core voice loop MUST be capable of running locally after required models and dependencies are installed.
- No paid API account or cloud subscription may be mandatory for core speaking functionality.
- Cloud providers MAY be offered as optional adapters, with explicit user opt-in and clear indication when data leaves the device.
- Provider-specific logic MUST be isolated behind interfaces. The application MUST NOT depend on a single model vendor.
- “Free” MUST NOT be represented as zero total cost: document model download size, device requirements, storage, licensing, and operational constraints.

## 3. Privacy and user control

- Microphone use MUST be visible and controllable.
- The user MUST be able to stop microphone capture immediately.
- Raw audio MUST NOT be retained by default. Persist transcripts and session metadata only as required for the feature.
- Recording retention MUST be a separate explicit opt-in with an understandable deletion path.
- Cloud processing MUST require clear consent before the first transmission and remain configurable.
- Secrets MUST never be committed, hard-coded, or written to logs.

## 4. Honest learning feedback

- The tutor MUST be respectful, supportive, and non-shaming.
- Corrections MUST preserve the learner's intended meaning and be appropriate to their proficiency level.
- The default conversation mode MUST avoid interrupting every sentence with a grammar lecture.
- A speech transcript MUST NOT be presented as an objective pronunciation score.
- Pronunciation scoring MAY be added only when a documented acoustic evaluation method and its limitations are implemented.
- AI-generated evaluations MUST be described as estimates, not authoritative language certifications.

## 5. Rust-first modular engineering

- Core orchestration, business logic, persistence, and provider adapters SHOULD be implemented in Rust.
- Desktop UI SHOULD use Tauri 2 with a TypeScript frontend unless an approved ADR changes the decision.
- Audio capture, VAD, STT, LLM, and TTS MUST be separate modules with explicit interfaces.
- External engines MAY run as local processes or local services when a reliable Rust-native implementation is not practical. This must be documented and tested.
- Avoid arbitrary shell execution, dynamic code execution, and broad IPC capabilities.

## 6. Test-first development

- Every user-visible requirement MUST have at least one acceptance scenario.
- New behavior MUST be developed test-first where practical: write or update a failing test, implement the smallest change, then refactor.
- Business logic MUST have deterministic unit tests.
- Provider integrations MUST have mock-based tests and separately gated integration tests.
- Audio tests MUST include recorded fixtures and must not require a physical microphone in normal CI.
- No task is complete solely because code compiles. The relevant tests, acceptance criteria, and manual checks must pass.

## 7. Reliability and graceful degradation

- Missing microphone, unavailable models, failed inference, invalid audio, and TTS failures MUST produce recoverable, user-understandable errors.
- The application MUST not lose completed transcript turns because a later turn fails.
- A failed optional provider MUST not corrupt local learning data.
- Audio resources and background tasks MUST be cancelled and released when a session ends.
- The UI MUST communicate states such as Listening, Processing, AI thinking, Speaking, Paused, and Error.

## 8. Accessibility and usability

- Voice interaction MUST have a push-to-talk fallback.
- Important controls MUST also be keyboard accessible.
- The UI MUST offer transcript visibility and clear microphone status.
- Users MUST be able to adjust speaking speed and correction intensity.
- Avoid visualizing a control as active when the microphone or audio pipeline is not actually active.

## 9. Performance and evidence

- Performance targets are hypotheses until measured on named hardware profiles.
- Do not hard-code a universal AI response-time guarantee.
- Track audio latency by stage: capture, endpointing, transcription, LLM first token, TTS first audio, and playback.
- Avoid unbounded audio buffers, unbounded conversation context, and memory growth across long sessions.

## 10. Documentation and change control

- Requirements describe observable behavior; plans describe implementation; tasks describe executable work.
- Every material architecture change MUST have an ADR.
- Every implementation change MUST trace to a requirement or an approved maintenance task.
- Update specs and tests when behavior changes.
- The AI coding agent MUST inspect the repository before editing and MUST NOT overwrite user work.
- The agent MUST report what it changed, what it tested, what it could not test, and any remaining risks.
