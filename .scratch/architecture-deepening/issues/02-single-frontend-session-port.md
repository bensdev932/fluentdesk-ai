# 02: Single Frontend Speaking Session Port & Review Coaching Sync

**What to build:** Connect the speaking studio UI cleanly through a single **Speaking Session** port (backed by the localhost HTTP adapter in production and one in-memory fake for UI tests) while keeping browser speech recognition and speech synthesis as media I/O adapters and rendering SQLite-persisted grammar coaching notes during session review.

**Blocked by:** 01: Authoritative Rust Speaking Session Host & SQLite Persistence

**Status:** ready-for-agent

- [x] UI shell accepts one unified **Speaking Session** port rather than split `ipcClient` plus optional `emulator` properties.
- [x] Browser studio drives start, turn submission, pause, resume, stop, and session history review against the localhost Rust **Speaking Session** endpoints without maintaining a separate client session state map, synthetic sleep delays, client-only state transition shortcuts, or unused scripted reply payloads.
- [x] Explicit offline local coaching fallback engages with a clear recoverable warning only when the localhost backend session port is unreachable.
- [x] Frontend **Conversation Turn** contract includes optional `correction` and `followUpPrompt` fields so saved grammar feedback appears in session review history.
- [x] TypeScript typecheck, Vitest suite (testing against a single fake of the unified port), and Vite build pass within all file/module line limits.
