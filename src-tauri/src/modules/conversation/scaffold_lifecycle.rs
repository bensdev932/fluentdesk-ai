//! Pedagogical scaffold startup configuration and bounded session auto-completion helpers.

use crate::domain::{
    default_scenario, detect_completed_goal_ids, find_scenario, sanitize_untrusted_text,
    ConversationRole, CorrectionMode, MaxSessionTurns, RoleplayScenario, SessionError,
    SessionState, SessionTopic, TutorReply,
};
use crate::modules::conversation::{
    context::TurnReconcileAction, manager::SessionManager, persistence::stage_meta,
    types::StartSpeakingSessionConfig,
};
use crate::modules::speech::intake::SpeechIntake;
use crate::stt::provider::SttPromptContext;

impl SessionManager {
    /// Builds bounded topic, vocabulary, and recent tutor keywords for Whisper `--prompt` biasing.
    pub(crate) fn stt_prompt_context(&self) -> SttPromptContext {
        let recent = self.context.recent_turns();
        SttPromptContext::from_session(
            self.scaffold.topic.as_str(),
            &self.scaffold.target_vocabulary,
            &recent,
        )
    }

    /// Initializes active Roleplay Scenario, CEFR level, topic, vocabulary, and turn limit.
    pub(crate) fn configure_scaffold(
        &mut self,
        cfg: &StartSpeakingSessionConfig,
    ) -> Result<(), SessionError> {
        let scenario = resolve_scenario(cfg.scenario_id.as_deref());
        self.scaffold.scenario_id = scenario.id.clone();
        self.scaffold.goals = scenario.goals.clone();
        self.scaffold.completed_goal_ids.clear();
        self.scaffold.cefr_level = cfg.cefr_level.unwrap_or(scenario.default_cefr);
        self.scaffold.topic = resolve_topic(cfg.topic.as_deref(), &scenario.title)?;
        let turns = cfg.max_turns.unwrap_or(scenario.default_max_turns);
        self.scaffold.max_turns = MaxSessionTurns::try_new(turns)?;
        self.scaffold.target_vocabulary = resolve_vocab(&cfg.target_vocabulary, &scenario);
        self.scaffold.user_turns_completed = 0;
        self.speech_intake =
            SpeechIntake::for_session_with_cefr(self.push_to_talk, self.scaffold.cefr_level);
        Ok(())
    }

    /// Merges LLM-reported and heuristic Communicative Goal completions, then advances turn count.
    pub(crate) fn advance_scaffold_with_reply(&mut self, reply: &TutorReply) {
        let last_user = self
            .context
            .recent_turns()
            .into_iter()
            .rev()
            .find(|t| t.role == ConversationRole::User)
            .map(|t| t.text)
            .unwrap_or_default();
        let heuristic = detect_completed_goal_ids(
            &self.scaffold.goals,
            &last_user,
            &self.scaffold.completed_goal_ids,
        );
        self.scaffold
            .mark_goals_completed(reply.completed_goal_ids.clone());
        self.scaffold.mark_goals_completed(heuristic);
        self.scaffold.advance_after_turn();
    }

    /// Transitions to `Ended` (and persists end timestamp) when bounded turns complete,
    /// otherwise returns cleanly to `Listening`.
    pub(crate) async fn complete_turn_transition(
        &mut self,
        ended_ms: i64,
    ) -> Result<(), SessionError> {
        if self.is_cancelled() {
            return Ok(());
        }
        if self.scaffold.is_completed() {
            self.clear_capture_state(true);
            let _ = self.adapters.audio_input.stop_capture().await;
            if self.state != SessionState::Ended {
                self.transition_to(SessionState::Ended)?;
                self.persist_session_ended(ended_ms).await?;
            }
            return Ok(());
        }
        if matches!(self.state, SessionState::Thinking | SessionState::Speaking) {
            self.transition_to(SessionState::Listening)?;
        }
        Ok(())
    }

    pub(crate) async fn record_tutor_tts_duration(
        &mut self,
        duration_ms: u32,
    ) -> Result<(), SessionError> {
        let Some(turn_id) = self.context.record_last_tutor_tts_duration(duration_ms) else {
            return Ok(());
        };
        if let Some(repo) = &self.repository {
            repo.update_turn_tts_duration(&turn_id, duration_ms).await?;
        }
        Ok(())
    }

    pub(crate) async fn record_interrupted_tutor_turn(
        &mut self,
        heard_text: &str,
        correction: Option<String>,
        ts_ms: i64,
        llm_ms: u32,
        tts_ms: u32,
        mode: CorrectionMode,
    ) -> Result<(), SessionError> {
        let mut meta = stage_meta(None, Some(llm_ms), mode);
        meta.tts_duration_ms = Some(tts_ms);
        self.record_turn(
            ConversationRole::Tutor,
            heard_text,
            correction,
            None,
            ts_ms,
            meta,
        )
        .await
        .map(|_| ())
    }

    pub(crate) async fn reconcile_tutor_turn_playback(
        &mut self,
        heard_text: Option<&str>,
        played_ms: Option<u32>,
    ) -> Result<(), SessionError> {
        let action = self
            .context
            .reconcile_last_tutor_turn(heard_text, played_ms);
        self.persist_reconcile_action(action).await
    }

    async fn persist_reconcile_action(
        &self,
        action: TurnReconcileAction,
    ) -> Result<(), SessionError> {
        let Some(repo) = &self.repository else {
            return Ok(());
        };
        match action {
            TurnReconcileAction::Removed(id) => repo.delete_turn_by_id(&id).await,
            TurnReconcileAction::Truncated(t) => {
                let tts = t.stage_metadata.as_ref().and_then(|m| m.tts_duration_ms);
                repo.update_tutor_turn_playback(&t.id, &t.text, t.follow_up_prompt.as_deref(), tts)
                    .await
            }
            TurnReconcileAction::DurationUpdated(id, ms) => {
                repo.update_turn_tts_duration(&id, ms).await
            }
            TurnReconcileAction::NoChange => Ok(()),
        }
    }
}

fn resolve_scenario(raw: Option<&str>) -> RoleplayScenario {
    raw.and_then(find_scenario).unwrap_or_else(default_scenario)
}

fn resolve_topic(raw: Option<&str>, fallback: &str) -> Result<SessionTopic, SessionError> {
    let trimmed = raw.map(str::trim).filter(|s| !s.is_empty());
    match trimmed {
        Some(t) => Ok(SessionTopic::try_new(t)?),
        None => Ok(SessionTopic::try_new(fallback)?),
    }
}

fn resolve_vocab(words: &[String], scenario: &RoleplayScenario) -> Vec<String> {
    let src = if words.is_empty() {
        &scenario.target_vocabulary
    } else {
        words
    };
    sanitize_vocab(src)
}

fn sanitize_vocab(words: &[String]) -> Vec<String> {
    words
        .iter()
        .map(|w| sanitize_untrusted_text(w))
        .filter(|w| !w.is_empty())
        .take(16)
        .collect()
}
