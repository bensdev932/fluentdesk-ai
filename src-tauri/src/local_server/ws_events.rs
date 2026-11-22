//! Outbound WebSocket server events carrying active scenario goals, opener/review, and turn updates.

use crate::ai::catalog::ProviderId;
use crate::domain::{
    ConversationTurn, CorrectionMode, PedagogicalScaffoldSnapshot, SessionError, SessionErrorKind,
    SessionId, SessionReview, SessionState, TutorReply,
};
use crate::local_server::session_dto::SessionTurnHttpResponse;
use crate::modules::conversation::{
    turn::TutorTextReadyEvent, SessionStatusSnapshot, TextTurnOutcome,
};
use crate::telemetry::redact_sensitive_log_text;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(super) enum WsServerEvent {
    StateChanged {
        #[serde(rename = "sessionId")]
        session_id: SessionId,
        state: SessionState,
        #[serde(rename = "correctionMode", skip_serializing_if = "Option::is_none")]
        correction_mode: Option<CorrectionMode>,
        #[serde(rename = "pushToTalk", skip_serializing_if = "Option::is_none")]
        push_to_talk: Option<bool>,
        #[serde(flatten, skip_serializing_if = "Option::is_none")]
        scaffold: Option<PedagogicalScaffoldSnapshot>,
        #[serde(rename = "openerReply", skip_serializing_if = "Option::is_none")]
        opener_reply: Option<Box<TutorReply>>,
        #[serde(rename = "sessionReview", skip_serializing_if = "Option::is_none")]
        session_review: Option<Box<SessionReview>>,
    },
    UserTranscript {
        #[serde(rename = "sessionId")]
        session_id: SessionId,
        turn: ConversationTurn,
        #[serde(skip_serializing_if = "Option::is_none")]
        confidence: Option<f32>,
    },
    Sentence {
        #[serde(rename = "sessionId")]
        session_id: SessionId,
        #[serde(rename = "turnId")]
        turn_id: String,
        text: String,
    },
    TurnComplete {
        #[serde(rename = "sessionId")]
        session_id: SessionId,
        #[serde(flatten)]
        payload: Box<WsTurnCompletePayload>,
    },
    Error {
        #[serde(rename = "sessionId")]
        session_id: SessionId,
        #[serde(rename = "turnId", skip_serializing_if = "Option::is_none")]
        turn_id: Option<String>,
        error: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        kind: Option<SessionErrorKind>,
        recoverable: bool,
    },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct WsTurnCompletePayload {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    pub user_turn: ConversationTurn,
    pub tutor_turn: ConversationTurn,
    pub reply: TutorReply,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<ProviderId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_cloud: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failover_note: Option<String>,
    pub state: SessionState,
    #[serde(flatten)]
    pub scaffold: PedagogicalScaffoldSnapshot,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_review: Option<SessionReview>,
}

impl WsServerEvent {
    pub(super) fn from_snapshot(s: &SessionStatusSnapshot) -> Self {
        Self::StateChanged {
            session_id: s.session_id.clone(),
            state: s.state,
            correction_mode: Some(s.correction_mode),
            push_to_talk: Some(s.push_to_talk),
            scaffold: Some(s.scaffold.clone()),
            opener_reply: s.opener_reply.clone().map(Box::new),
            session_review: s.session_review.clone().map(Box::new),
        }
    }

    pub(super) fn state_only(sid: &SessionId, state: SessionState) -> Self {
        Self::StateChanged {
            session_id: sid.clone(),
            state,
            correction_mode: None,
            push_to_talk: None,
            scaffold: None,
            opener_reply: None,
            session_review: None,
        }
    }

    pub(super) fn from_error(sid: &SessionId, err: &SessionError) -> Self {
        Self::from_error_for_turn(sid, err, None)
    }

    pub(super) fn from_error_for_turn(
        sid: &SessionId,
        err: &SessionError,
        turn_id: Option<String>,
    ) -> Self {
        Self::Error {
            session_id: sid.clone(),
            turn_id,
            error: redact_sensitive_log_text(&err.user_message),
            kind: Some(err.kind),
            recoverable: err.recoverable,
        }
    }

    pub(super) fn from_text_outcome(outcome: TextTurnOutcome, turn_id: Option<String>) -> Self {
        let sid = outcome.session_id.clone();
        let r = SessionTurnHttpResponse::from_outcome(outcome);
        Self::TurnComplete {
            session_id: sid,
            payload: Box::new(WsTurnCompletePayload {
                turn_id,
                user_turn: r.user_turn,
                tutor_turn: r.tutor_turn,
                reply: r.reply,
                provider: Some(r.provider),
                model: Some(r.model),
                is_cloud: Some(r.is_cloud),
                failover_note: r.failover_note,
                state: r.state,
                scaffold: r.scaffold,
                session_review: r.session_review,
            }),
        }
    }

    pub(super) fn from_spoken(snap: &SessionStatusSnapshot, ev: TutorTextReadyEvent) -> Self {
        Self::TurnComplete {
            session_id: snap.session_id.clone(),
            payload: Box::new(WsTurnCompletePayload {
                turn_id: None,
                user_turn: ev.user_turn,
                tutor_turn: ev.tutor_turn,
                reply: ev.reply,
                provider: None,
                model: None,
                is_cloud: None,
                failover_note: None,
                state: snap.state,
                scaffold: snap.scaffold.clone(),
                session_review: snap.session_review.clone(),
            }),
        }
    }
}
