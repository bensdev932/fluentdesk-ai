//! HTTP JSON DTOs for `/api/sessions` speaking session lifecycle and turn endpoints.

use crate::ai::catalog::ProviderId;
use crate::domain::{
    CefrLevel, ConversationTurn, CorrectionMode, LearnerPreferences, PedagogicalScaffoldSnapshot,
    SessionError, SessionErrorKind, SessionReview, SessionState, TutorReply,
};
use crate::local_server::dto::error_response;
use crate::local_server::http::HttpResponse;
use crate::modules::conversation::{StartSpeakingSessionConfig, TextTurnOutcome};
use crate::telemetry::redact_sensitive_log_text;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct StartSessionHttpRequest {
    #[serde(default)]
    pub correction_mode: Option<CorrectionMode>,
    #[serde(default)]
    pub push_to_talk: Option<bool>,
    #[serde(default)]
    pub input_device_id: Option<String>,
    #[serde(default)]
    pub cefr_level: Option<CefrLevel>,
    #[serde(default)]
    pub scenario_id: Option<String>,
    #[serde(default)]
    pub topic: Option<String>,
    #[serde(default)]
    pub target_vocabulary: Vec<String>,
    #[serde(default)]
    pub max_turns: Option<u32>,
}

impl StartSessionHttpRequest {
    pub(super) fn into_config(self, prefs: LearnerPreferences) -> StartSpeakingSessionConfig {
        let explicit_level = self.cefr_level;
        StartSpeakingSessionConfig {
            correction_mode: self.correction_mode.unwrap_or(prefs.correction_mode),
            push_to_talk: self.push_to_talk.unwrap_or(prefs.push_to_talk_enabled),
            input_device_id: self.input_device_id,
            cefr_level: explicit_level.or(self.scenario_id.is_none().then_some(prefs.cefr_level)),
            scenario_id: self.scenario_id,
            topic: self.topic,
            target_vocabulary: self.target_vocabulary,
            max_turns: self.max_turns,
        }
    }
}

pub(super) fn map_session_http_error(err: &SessionError) -> HttpResponse {
    let safe_msg = redact_sensitive_log_text(&err.user_message);
    match err.kind {
        SessionErrorKind::InvalidStateTransition | SessionErrorKind::Cancelled => {
            error_response(409, "Conflict", safe_msg)
        }
        SessionErrorKind::AiProviderUnavailable
        | SessionErrorKind::SttUnavailable
        | SessionErrorKind::AudioDeviceUnavailable
        | SessionErrorKind::MicrophonePermissionDenied => {
            error_response(503, "Service Unavailable", safe_msg)
        }
        SessionErrorKind::PersistenceFailed => {
            error_response(500, "Internal Server Error", safe_msg)
        }
        SessionErrorKind::AiInferenceFailed
        | SessionErrorKind::SttFailed
        | SessionErrorKind::TtsUnavailable
        | SessionErrorKind::TtsFailed
        | SessionErrorKind::PlaybackFailed => error_response(502, "Bad Gateway", safe_msg),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionTurnHttpRequest {
    #[serde(default)]
    pub user_text: String,
    #[serde(default)]
    pub correction_mode: Option<CorrectionMode>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionTurnHttpResponse {
    pub user_turn: ConversationTurn,
    pub tutor_turn: ConversationTurn,
    pub reply: TutorReply,
    pub provider: ProviderId,
    pub model: String,
    pub is_cloud: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failover_note: Option<String>,
    pub state: SessionState,
    #[serde(flatten)]
    pub scaffold: PedagogicalScaffoldSnapshot,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_review: Option<SessionReview>,
}

impl SessionTurnHttpResponse {
    pub fn from_outcome(outcome: TextTurnOutcome) -> Self {
        let r = outcome.routed;
        Self {
            user_turn: outcome.user_turn,
            tutor_turn: outcome.tutor_turn,
            reply: r.reply,
            provider: r.provider,
            model: r.model,
            is_cloud: r.is_cloud,
            failover_note: r.failover_note.as_deref().map(redact_sensitive_log_text),
            state: outcome.state,
            scaffold: outcome.scaffold,
            session_review: outcome.session_review,
        }
    }
}
