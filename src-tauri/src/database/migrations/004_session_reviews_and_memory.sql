-- 004_session_reviews_and_memory.sql
-- Post-session pedagogical reviews (explicitly labeled as AI Estimate) and text-only Learner Facts.

CREATE TABLE IF NOT EXISTS session_reviews (
    session_id TEXT PRIMARY KEY NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    scenario_id TEXT NOT NULL,
    estimate_label TEXT NOT NULL DEFAULT 'AI Estimate',
    summary TEXT NOT NULL,
    cefr_signal TEXT NOT NULL CHECK (cefr_signal IN ('A1', 'A2', 'B1', 'B2', 'C1', 'C2')),
    completed_goal_ids_json TEXT NOT NULL DEFAULT '[]',
    grammar_focuses_json TEXT NOT NULL DEFAULT '[]',
    vocabulary_used_json TEXT NOT NULL DEFAULT '[]',
    turn_count INTEGER NOT NULL DEFAULT 0,
    learner_word_count INTEGER NOT NULL DEFAULT 0,
    created_at_unix_ms INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS learner_facts (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    fact_text TEXT NOT NULL,
    source_session_id TEXT REFERENCES conversations(id) ON DELETE SET NULL,
    created_at_unix_ms INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_learner_facts_user_created
    ON learner_facts(user_id, created_at_unix_ms DESC);
