-- 003_pedagogical_scaffolding.sql
-- Persist learner default CEFR proficiency level alongside voice settings in SQLite.

ALTER TABLE session_settings
    ADD COLUMN cefr_level TEXT NOT NULL DEFAULT 'B1'
    CHECK (cefr_level IN ('A1', 'A2', 'B1', 'B2', 'C1', 'C2'));
