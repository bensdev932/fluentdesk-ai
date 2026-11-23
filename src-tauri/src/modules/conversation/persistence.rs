//! Internal SQLite turn & session persistence helpers for `SessionManager`.

use crate::domain::{
    AudioFormat, ConversationRole, ConversationTurn, CorrectionMode, ProviderHealth, SessionError,
    SessionErrorKind, SessionState, TurnStageMetadata, TutorReply,
};
pub(crate) use crate::modules::conversation::persistence_helpers::{
    build_turn, current_unix_ms, invalid_transition_err, stage_meta, TurnCoaching,
};
use crate::modules::conversation::{
    manager::SessionManager,
    review_engine::{build_session_review, generate_and_persist},
};

impl SessionManager {
    pub(crate) async fn ensure_session_persisted(
        &mut self,
        user_id: &str,
        now_ms: i64,
    ) -> Result<(), SessionError> {
        if let Some(repo) = &self.repository {
            if !self.conversation_persisted {
                repo.ensure_default_user(user_id, "Learner", now_ms).await?;
                repo.create_conversation_with_scenario(
                    &self.session_id,
                    user_id,
                    &self.scaffold.scenario_id,
                    self.correction_mode,
                    now_ms,
                )
                .await?;
                self.conversation_persisted = true;
                self.context_loaded = true;
            }
        }
        Ok(())
    }

    pub(crate) async fn persist_session_ended(
        &mut self,
        ended_at_unix_ms: i64,
    ) -> Result<(), SessionError> {
        if let Some(repo) = &self.repository {
            if self.conversation_persisted {
                repo.mark_conversation_ended(&self.session_id, ended_at_unix_ms)
                    .await?;
                if let Ok(review) = generate_and_persist(
                    repo,
                    &self.user_id,
                    &self.session_id,
                    &self.scaffold,
                    ended_at_unix_ms,
                )
                .await
                {
                    self.latest_review = Some(review);
                    return Ok(());
                }
            }
        }
        let turns = self.context.recent_turns();
        self.latest_review = Some(build_session_review(
            &self.session_id,
            &self.scaffold,
            &turns,
            ended_at_unix_ms,
        ));
        Ok(())
    }

    pub(crate) async fn record_user_turn(
        &mut self,
        text: &str,
        ts_ms: i64,
        stt_ms: u32,
        mode: CorrectionMode,
    ) -> Result<ConversationTurn, SessionError> {
        let meta = stage_meta(Some(stt_ms), None, mode);
        self.record_turn(
            ConversationRole::User,
            text,
            TurnCoaching::default(),
            None,
            ts_ms,
            meta,
        )
        .await
    }

    pub(crate) async fn record_tutor_turn(
        &mut self,
        reply: &TutorReply,
        ts_ms: i64,
        llm_ms: u32,
        mode: CorrectionMode,
    ) -> Result<ConversationTurn, SessionError> {
        let meta = stage_meta(None, Some(llm_ms), mode);
        let coaching = TurnCoaching::from_reply(reply);
        let turn = self
            .record_turn(
                ConversationRole::Tutor,
                &reply.spoken_reply,
                coaching,
                reply.follow_up_prompt.clone(),
                ts_ms,
                meta,
            )
            .await?;
        self.advance_scaffold_with_reply(reply);
        Ok(turn)
    }

    pub(crate) async fn record_turn(
        &mut self,
        role: ConversationRole,
        text: &str,
        coaching: TurnCoaching,
        follow_up_prompt: Option<String>,
        ts_ms: i64,
        meta: TurnStageMetadata,
    ) -> Result<ConversationTurn, SessionError> {
        let seq = self.next_sequence_number;
        self.next_sequence_number += 1;
        let turn = build_turn(
            &self.session_id,
            seq,
            role,
            text,
            coaching,
            follow_up_prompt,
            ts_ms,
            meta,
        );
        if let Some(repo) = &self.repository {
            repo.append_turn(&turn, seq).await?;
        }
        self.context.push_turn(turn.clone());
        Ok(turn)
    }

    pub(crate) async fn load_context_if_empty(&mut self) -> Result<(), SessionError> {
        if self.context_loaded || self.context.turn_count() > 0 {
            return Ok(());
        }
        let Some(repo) = &self.repository else {
            self.context_loaded = true;
            return Ok(());
        };
        let turns = repo.list_turns_chronological(&self.session_id).await?;
        let count = u32::try_from(turns.len()).unwrap_or(u32::MAX);
        self.next_sequence_number = self.next_sequence_number.max(count);
        for turn in turns {
            self.context.push_turn(turn);
        }
        self.context_loaded = true;
        Ok(())
    }

    pub(crate) async fn verify_required_providers(&mut self) -> Result<(), SessionError> {
        let missing = if self.adapters.stt.health_check().await != ProviderHealth::Available {
            Some(SessionErrorKind::SttUnavailable)
        } else if self.adapters.ai.health_check().await != ProviderHealth::Available {
            Some(SessionErrorKind::AiProviderUnavailable)
        } else {
            None
        };
        if let Some(kind) = missing {
            let _ = self.transition_to(SessionState::Error);
            let msg = "Required local speech or AI service is unavailable.";
            return Err(SessionError::new(kind, msg, true, None));
        }
        Ok(())
    }

    pub(crate) async fn start_microphone_stream(&mut self) -> Result<(), SessionError> {
        let fmt = AudioFormat::WHISPER_16KHZ_MONO_F32;
        let rx = self
            .adapters
            .audio_input
            .start_capture(self.active_input_device.as_ref().map(|id| id.as_str()), fmt)
            .await
            .inspect_err(|_| {
                let _ = self.transition_to(SessionState::Error);
            })?;
        self.capture_rx = Some(rx);
        Ok(())
    }
}
