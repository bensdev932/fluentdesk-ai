# 01: Authoritative Rust Speaking Session Host & SQLite Persistence

**What to build:** Establish one authoritative Rust **Speaking Session** host wired to local SQLite persistence and the **Tutor Router**, exposed over localhost session endpoints (`start`, `turn`, `pause`, `resume`, `stop`, and `history`), while removing the shallow Rust session pass-through wrapper (`force_stage_for_test`) and stateless per-turn history uploads.

**Blocked by:** None (can start immediately)

**Status:** ready-for-agent

- [x] Lifecycle states (`Idle`, `Initializing`, `Listening`, `Processing`, `Thinking`, `Speaking`, `Paused`, `Error`, `Ended`) and active session ID verification live inside one **Speaking Session** module without any test-only stage override hook.
- [x] Starting a **Speaking Session** defaults **Correction Mode** to `Natural`, honors explicit correction mode overrides, opens persistent SQLite storage (when configured with a DB pool or loopback host), and stores `push_to_talk` session configuration accurately.
- [x] Localhost HTTP bridge exposes persistent session lifecycle, spoken text turn submission (reading prior turns from backend session/SQLite state rather than requiring 50-turn history payloads), and chronological history retrieval (including saved `correction` and `followUpPrompt` metadata).
- [x] Failing AI inference or TTS playback does not discard completed **Conversation Turn** transcripts, and `stop` cancels in-flight inference immediately.
- [x] End-to-end Rust tests verify session lifecycle, persistence across turns, and localhost HTTP session endpoints against `sqlite::memory:` and mock AI/STT/TTS adapters.
