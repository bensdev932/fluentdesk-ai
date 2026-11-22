# 03: Consolidated SQLite Learner Preferences & Secrets-Only Vault

**What to build:** Consolidate all non-secret learner and routing settings (**Correction Mode**, speaking speed, push-to-talk, raw-audio opt-in, cloud opt-in, active provider, and model overrides) behind one SQLite-backed **Learner Preferences** seam shared across **Speaking Session**, **Tutor Router**, and HTTP settings handlers—isolating Windows DPAPI **Credential Vault** strictly to API keys and removing duplicated policy/hook logic.

**Blocked by:** None (can start immediately)

- [x] Forward idempotent SQLite migration extends the settings schema cleanly for routing preferences (`preferred_provider`, `model_overrides_json`) alongside learner settings with privacy defaults (`cloud_opt_in = false`, `raw_audio_recording_opt_in = false`, `correction_mode = Natural`).
- [x] Backend validates speaking speed within bounds, automatically resets any pinned cloud provider to `Auto` whenever `cloud_opt_in` is disabled, and exposes unified `get`/`patch` preference synchronization without redundant policy/command wrappers.
- [x] **Credential Vault** persists only DPAPI-encrypted cloud API secrets (`Saved > Env > None`), deleting separate JSON preference persistence while preserving cross-restart secret resolution and zero secret leakage.
- [x] React provider settings hook becomes a thin adapter rendering backend snapshots and forwarding patches without duplicating server consent rules; Rust and frontend tests verify validation, migration, consent reset, and cross-restart vault + SQLite behavior.
