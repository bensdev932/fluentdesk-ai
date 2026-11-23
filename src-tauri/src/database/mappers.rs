//! SQLite row-to-domain mappers for `SessionRepository`.

use crate::domain::{
    CefrLevel, ConversationRole, ConversationTurn, CorrectionMode, LearnerPreferences, ProviderId,
    ProviderPreference, SessionError, SessionErrorKind, SessionId, SessionSummary, SpeakingSpeed,
    TurnStageMetadata,
};
use sqlx::{sqlite::SqliteRow, Row};
use std::collections::{BTreeMap, HashMap};
use std::str::FromStr;

pub fn persistence_err(msg: &'static str, err: impl std::fmt::Display) -> SessionError {
    SessionError::new(
        SessionErrorKind::PersistenceFailed,
        msg,
        true,
        Some(err.to_string()),
    )
}

pub fn map_preferences_row(row: &SqliteRow) -> Result<LearnerPreferences, SessionError> {
    let mode_str: String = row.get("correction_mode");
    let speed_raw: f64 = row.get("speaking_speed");
    let pref_str: String = row.get("preferred_provider");
    let overrides_json: String = row.get("model_overrides_json");
    let cefr_str: String = row
        .try_get("cefr_level")
        .unwrap_or_else(|_| "B1".to_string());
    let voice_raw: String = row
        .try_get("english_voice_id")
        .unwrap_or_else(|_| "en-US".to_string());
    let english_voice_id =
        crate::domain::validate_english_voice_id(&voice_raw).unwrap_or_else(|_| "en-US".into());
    let mut prefs = LearnerPreferences {
        correction_mode: parse_correction_mode(&mode_str),
        speaking_speed: SpeakingSpeed::try_new(speed_raw as f32)?,
        push_to_talk_enabled: row.get::<i64, _>("push_to_talk_enabled") != 0,
        cloud_opt_in: row.get::<i64, _>("cloud_opt_in") != 0,
        raw_audio_recording_opt_in: row.get::<i64, _>("raw_audio_recording_opt_in") != 0,
        preferred_provider: ProviderPreference::from_str(&pref_str).unwrap_or_default(),
        model_overrides: parse_model_overrides(&overrides_json),
        cefr_level: CefrLevel::from_str(&cefr_str).unwrap_or_default(),
        english_voice_id,
    };
    prefs.enforce_cloud_consent_invariant();
    Ok(prefs)
}

pub fn parse_model_overrides(raw_json: &str) -> HashMap<ProviderId, String> {
    let parsed: HashMap<String, String> = serde_json::from_str(raw_json).unwrap_or_default();
    parsed
        .into_iter()
        .filter_map(|(k, v)| {
            let id = ProviderId::from_str(&k).ok()?;
            let trimmed = v.trim();
            (!trimmed.is_empty()).then(|| (id, trimmed.to_string()))
        })
        .collect()
}

pub fn format_model_overrides(overrides: &HashMap<ProviderId, String>) -> String {
    let clean: BTreeMap<&str, &str> = overrides
        .iter()
        .filter_map(|(id, model)| {
            let trimmed = model.trim();
            (!trimmed.is_empty()).then_some((id.as_str(), trimmed))
        })
        .collect();
    serde_json::to_string(&clean).unwrap_or_else(|_| "{}".to_string())
}

pub fn map_turn_row(row: &SqliteRow) -> Result<ConversationTurn, SessionError> {
    let conv_id: String = row.get("conversation_id");
    let session_id =
        SessionId::new(conv_id).map_err(|e| persistence_err("Invalid session ID in DB.", e))?;
    let role_str: String = row.get("role");
    let role = if role_str == "Tutor" {
        ConversationRole::Tutor
    } else {
        ConversationRole::User
    };
    Ok(ConversationTurn {
        id: row.get("id"),
        session_id,
        role,
        text: row.get("text"),
        correction: row.get("correction_text"),
        proper_way: row.try_get("proper_way").ok().flatten(),
        why: row.try_get("feedback_why").ok().flatten(),
        follow_up_prompt: row.get("follow_up_prompt"),
        timestamp_unix_ms: row.get("timestamp_unix_ms"),
        stage_metadata: map_stage_metadata(row),
    })
}

fn map_stage_metadata(row: &SqliteRow) -> Option<TurnStageMetadata> {
    let stt = opt_u32(row, "stt_duration_ms");
    let llm = opt_u32(row, "llm_duration_ms");
    let tts = opt_u32(row, "tts_duration_ms");
    let mode: Option<String> = row.try_get("conversation_correction_mode").ok().flatten();
    if stt.is_some() || llm.is_some() || tts.is_some() || mode.is_some() {
        Some(TurnStageMetadata {
            stt_duration_ms: stt,
            llm_duration_ms: llm,
            tts_duration_ms: tts,
            correction_mode: mode.as_deref().map(parse_correction_mode),
        })
    } else {
        None
    }
}

fn opt_u32(row: &SqliteRow, col: &str) -> Option<u32> {
    row.get::<Option<i64>, _>(col)
        .and_then(|v| u32::try_from(v).ok())
}

pub fn format_role(role: ConversationRole) -> &'static str {
    match role {
        ConversationRole::User => "User",
        ConversationRole::Tutor => "Tutor",
    }
}

pub fn format_correction_mode(mode: CorrectionMode) -> &'static str {
    match mode {
        CorrectionMode::Natural => "Natural",
        CorrectionMode::Gentle => "Gentle",
        CorrectionMode::Detailed => "Detailed",
        CorrectionMode::Off => "Off",
    }
}

pub fn parse_correction_mode(raw: &str) -> CorrectionMode {
    match raw {
        "Natural" => CorrectionMode::Natural,
        "Detailed" => CorrectionMode::Detailed,
        "Off" => CorrectionMode::Off,
        _ => CorrectionMode::Gentle,
    }
}

pub fn map_session_summary_row(row: &SqliteRow) -> Result<SessionSummary, SessionError> {
    let raw_id: String = row.get("id");
    let session_id =
        SessionId::new(raw_id).map_err(|e| persistence_err("Invalid session ID in DB.", e))?;
    let mode_str: String = row.get("correction_mode");
    let count: i64 = row.get("turn_count");
    Ok(SessionSummary {
        session_id,
        started_at_unix_ms: row.get("started_at_unix_ms"),
        ended_at_unix_ms: row.get("ended_at_unix_ms"),
        correction_mode: parse_correction_mode(&mode_str),
        turn_count: u32::try_from(count).unwrap_or(0),
    })
}
