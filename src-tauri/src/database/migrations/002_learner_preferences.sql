-- 002_learner_preferences.sql
-- Consolidate non-secret AI routing preferences alongside learner voice settings in SQLite.

ALTER TABLE session_settings
    ADD COLUMN preferred_provider TEXT NOT NULL DEFAULT 'auto'
    CHECK (preferred_provider IN ('auto', 'ollama', 'groq', 'gemini', 'openrouter'));

ALTER TABLE session_settings
    ADD COLUMN model_overrides_json TEXT NOT NULL DEFAULT '{}';
