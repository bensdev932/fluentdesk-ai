# AI Coding Agent Instructions — FluentDesk AI

## Before doing work

1. Read `.specify/memory/constitution.md`.
2. Read the active feature's `spec.md`, `plan.md`, `tasks.md`, and checklists.
3. Inspect the existing repository and current git diff before editing. Never overwrite unrelated user work.
4. Identify the exact task IDs and acceptance scenarios being implemented.
5. If a requirement is ambiguous, contradictory, unsafe, or technically unsupported, stop and report the issue or update the specification through the agreed workflow. Do not silently invent product behavior.

## Product guardrail

FluentDesk is a **voice-first English speaking tutor**. Do not turn it into a text-first chatbot or quiz app. The primary loop is spoken input → speech recognition → contextual AI response → spoken output → next speaking turn.

## Implementation rules

- Implement only the requested task scope. Do not build speculative features.
- Adhere strictly to the engineering rules and code smell guards in [rules.md](../rules.md) (SOLID principles, Value Objects for domain primitives, Law of Demeter, Tell Don't Ask, <10 lines/method, <50 lines/component).
- Prefer small, reviewable vertical slices.
- Keep audio capture, VAD, STT, LLM, TTS, session orchestration, and persistence behind separate interfaces.
- Keep provider-specific SDKs and endpoints inside adapters.
- Do not use cloud providers by default; ask for opt-in before transmission.
- Do not store raw audio unless a requirement explicitly enables user-consented recording.
- Keep Tauri IPC commands narrow, typed, validated, and documented.
- Never expose arbitrary shell execution to the frontend.
- Use structured types for data crossing module boundaries.
- Treat model output as untrusted input. Validate structured responses and safely render text.
- Use bounded channels and cancellation-aware tasks for streaming audio and AI work.
- Ensure stop-session cancels capture, inference where possible, synthesis, playback, and background tasks.
- Pin or document dependency versions when the project is bootstrapped. Verify current API documentation rather than assuming methods from memory.

## TDD rules

For each behavior change:

1. Add or update a test that expresses the expected behavior.
2. Run the focused test and confirm it fails for the expected reason.
3. Implement the smallest change to pass.
4. Run focused tests, then relevant broader tests.
5. Run formatting and lint checks.
6. Review the diff for unrelated edits, secrets, unsafe IPC, data leaks, unbounded buffers, and missed cleanup.
7. Update task checkboxes only when the task's stated evidence exists.

Use deterministic mocks for LLM/STT/TTS interfaces in normal unit tests. Keep hardware-dependent audio tests and model-backed tests behind explicit integration-test flags or documented environment prerequisites.

## Required completion report

At the end of each task group, report:

- Task IDs completed.
- Files changed.
- Tests run and exact outcomes.
- Tests not run and why.
- Acceptance criteria satisfied or not satisfied.
- Known limitations and follow-up tasks.

Never claim a feature works offline, is low latency, is accurate, or is secure unless the corresponding behavior has been tested or the claim is clearly qualified.
