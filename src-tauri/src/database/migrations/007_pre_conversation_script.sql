ALTER TABLE session_settings
    ADD COLUMN pre_conversation_script_enabled INTEGER NOT NULL DEFAULT 0 CHECK (pre_conversation_script_enabled IN (0, 1));
