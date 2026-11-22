-- 001_initial_schema.sql
-- FluentDesk AI initial SQLite schema (`T005`, `plan.md` Section 7, `software-design.md` Section 8).
-- Privacy invariant: No raw audio BLOB or file path column exists in default schema.

CREATE TABLE IF NOT EXISTS users (
    id TEXT PRIMARY KEY NOT NULL,
    display_name TEXT NOT NULL,
    proficiency_level TEXT NOT NULL DEFAULT 'B1' CHECK (proficiency_level IN ('A1', 'A2', 'B1', 'B2', 'C1', 'C2')),
    target_goal TEXT NOT NULL DEFAULT 'general_conversation',
    created_at_unix_ms INTEGER NOT NULL,
    updated_at_unix_ms INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS session_settings (
    user_id TEXT PRIMARY KEY NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    correction_mode TEXT NOT NULL DEFAULT 'Natural' CHECK (correction_mode IN ('Natural', 'Gentle', 'Detailed', 'Off')),
    speaking_speed REAL NOT NULL DEFAULT 1.0 CHECK (speaking_speed >= 0.5 AND speaking_speed <= 2.0),
    push_to_talk_enabled INTEGER NOT NULL DEFAULT 0 CHECK (push_to_talk_enabled IN (0, 1)),
    cloud_opt_in INTEGER NOT NULL DEFAULT 0 CHECK (cloud_opt_in IN (0, 1)),
    raw_audio_recording_opt_in INTEGER NOT NULL DEFAULT 0 CHECK (raw_audio_recording_opt_in IN (0, 1)),
    updated_at_unix_ms INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS conversations (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    scenario_mode TEXT NOT NULL DEFAULT 'free_conversation',
    correction_mode TEXT NOT NULL DEFAULT 'Natural' CHECK (correction_mode IN ('Natural', 'Gentle', 'Detailed', 'Off')),
    state TEXT NOT NULL DEFAULT 'Idle',
    started_at_unix_ms INTEGER NOT NULL,
    ended_at_unix_ms INTEGER
);

CREATE TABLE IF NOT EXISTS conversation_turns (
    id TEXT PRIMARY KEY NOT NULL,
    conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    sequence_number INTEGER NOT NULL CHECK (sequence_number >= 0),
    role TEXT NOT NULL CHECK (role IN ('User', 'Tutor')),
    text TEXT NOT NULL,
    correction_text TEXT,
    follow_up_prompt TEXT,
    stt_duration_ms INTEGER,
    llm_duration_ms INTEGER,
    tts_duration_ms INTEGER,
    timestamp_unix_ms INTEGER NOT NULL,
    UNIQUE (conversation_id, sequence_number)
);

CREATE INDEX IF NOT EXISTS idx_conversations_user_started
    ON conversations(user_id, started_at_unix_ms DESC);

CREATE INDEX IF NOT EXISTS idx_conversation_turns_chronological
    ON conversation_turns(conversation_id, sequence_number ASC, timestamp_unix_ms ASC);
