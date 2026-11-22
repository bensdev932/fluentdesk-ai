//! Shared non-blocking turn helpers and `submit_text_turn` on `SpeakingSessionHost`.

use crate::ai::provider::AiProvider;
use crate::ai::router_types::RoutedTutorReply;
use crate::domain::{
    ConversationTurn, CorrectionMode, PedagogicalScaffold, PedagogicalScaffoldSnapshot,
    SessionError, SessionId, SessionState, Transcript,
};
pub(crate) use crate::modules::conversation::host_turn_guards::{
    cancel_mgr_turn, cancelled_turn_err, elapsed_ms, fail_if_err, require_state,
    verify_still_active, verify_turn_active,
};
use crate::modules::conversation::{
    cancellation::SessionCancellationToken,
    host::{verify_active_mut, SpeakingSessionHost},
    manager::SessionManager,
    types::TextTurnOutcome,
};
use std::sync::Arc;

pub(crate) struct PreparedThinking {
    pub(crate) user_turn: ConversationTurn,
    pub(crate) history: Vec<ConversationTurn>,
    pub(crate) mode: CorrectionMode,
    pub(crate) scaffold: PedagogicalScaffold,
    pub(crate) ai: Arc<dyn AiProvider>,
    pub(crate) cancel: SessionCancellationToken,
}

impl SpeakingSessionHost {
    pub async fn submit_text_turn(
        &self,
        session_id: &SessionId,
        transcript: &Transcript,
        correction_mode: Option<CorrectionMode>,
        now_unix_ms: i64,
    ) -> Result<TextTurnOutcome, SessionError> {
        let (tx, rx) = tokio::sync::mpsc::channel(1);
        drop(rx);
        self.submit_text_turn_stream(session_id, transcript, correction_mode, now_unix_ms, tx)
            .await
    }

    pub(crate) async fn prepare_thinking_turn(
        &self,
        sid: &SessionId,
        tx: &Transcript,
        mode_override: Option<CorrectionMode>,
        now_ms: i64,
        from_listening: bool,
        prior_cancel: Option<&SessionCancellationToken>,
    ) -> Result<PreparedThinking, SessionError> {
        let mut guard = self.active.lock().await;
        if prior_cancel.is_some_and(SessionCancellationToken::is_cancelled) {
            return Err(cancelled_turn_err());
        }
        let mgr = verify_active_mut(guard.as_mut(), sid)?;
        prepare_mgr_for_thinking(mgr, mode_override, from_listening)?;
        let loaded = mgr.load_context_if_empty().await;
        fail_if_err(mgr, loaded)?;
        build_prepared_thinking(mgr, tx, now_ms).await
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn finish_text_turn(
        &self,
        sid: &SessionId,
        user_turn: ConversationTurn,
        mode: CorrectionMode,
        now_ms: i64,
        llm_ms: u32,
        cancel: &SessionCancellationToken,
        res: Result<RoutedTutorReply, SessionError>,
    ) -> Result<TextTurnOutcome, SessionError> {
        let mut guard = self.active.lock().await;
        let mgr = verify_turn_active(guard.as_mut(), sid, cancel)?;
        let routed = fail_if_err(mgr, res)?;
        let t_res = mgr
            .record_tutor_turn(&routed.reply, now_ms + 1, llm_ms, mode)
            .await;
        let tutor_turn = fail_if_err(mgr, t_res)?;
        let trans = mgr.complete_turn_transition(now_ms + 1).await;
        fail_if_err(mgr, trans)?;
        Ok(build_text_outcome(sid, user_turn, tutor_turn, routed, mgr))
    }
}

fn build_text_outcome(
    sid: &SessionId,
    user_turn: ConversationTurn,
    tutor_turn: ConversationTurn,
    routed: RoutedTutorReply,
    mgr: &SessionManager,
) -> TextTurnOutcome {
    TextTurnOutcome {
        session_id: sid.clone(),
        user_turn,
        tutor_turn,
        routed,
        state: mgr.state,
        scaffold: PedagogicalScaffoldSnapshot::from_scaffold(&mgr.scaffold),
        session_review: mgr.latest_review.clone(),
    }
}

fn prepare_mgr_for_thinking(
    mgr: &mut SessionManager,
    mode_override: Option<CorrectionMode>,
    from_listening: bool,
) -> Result<(), SessionError> {
    mgr.speech_intake.reset();
    if from_listening {
        require_state(mgr, SessionState::Listening, SessionState::Processing)?;
        mgr.transition_to(SessionState::Processing)?;
        mgr.transition_to(SessionState::Thinking)?;
    }
    if let Some(m) = mode_override {
        mgr.correction_mode = m;
    }
    Ok(())
}

async fn build_prepared_thinking(
    mgr: &mut SessionManager,
    tx: &Transcript,
    now_ms: i64,
) -> Result<PreparedThinking, SessionError> {
    let (history, mode) = (mgr.context.recent_turns(), mgr.correction_mode);
    let u_res = mgr
        .record_user_turn(&tx.text, now_ms, tx.duration_ms, mode)
        .await;
    let user_turn = fail_if_err(mgr, u_res)?;
    Ok(PreparedThinking {
        user_turn,
        history,
        mode,
        scaffold: mgr.scaffold.clone(),
        ai: mgr.adapters.ai.clone(),
        cancel: mgr.cancellation.clone(),
    })
}
