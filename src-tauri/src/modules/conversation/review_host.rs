//! Host entry points for post-session AI-estimate review retrieval and Learner Fact deletion.

use crate::domain::{
    default_scenario, find_scenario, LearnerFact, LearnerProgress, PedagogicalScaffold,
    SessionError, SessionId, SessionReview,
};
use crate::modules::conversation::{
    host::SpeakingSessionHost, persistence::current_unix_ms, review_engine::generate_and_persist,
};

impl SpeakingSessionHost {
    pub async fn get_learner_progress(&self) -> Result<LearnerProgress, SessionError> {
        match &self.repository {
            Some(repo) => {
                repo.compute_learner_progress(&self.user_id, current_unix_ms())
                    .await
            }
            None => Ok(LearnerProgress::default()),
        }
    }

    pub async fn get_or_generate_session_review(
        &self,
        session_id: &SessionId,
    ) -> Result<Option<SessionReview>, SessionError> {
        let Some(repo) = &self.repository else {
            return Ok(None);
        };
        if let Some(existing) = repo.find_session_review(session_id).await? {
            return Ok(Some(existing));
        }
        let Some((user_id, scenario_id)) = repo.find_session_meta(session_id).await? else {
            return Ok(None);
        };
        let scaffold = self
            .active_or_scenario_scaffold(session_id, &scenario_id)
            .await;
        generate_and_persist(repo, &user_id, session_id, &scaffold, current_unix_ms())
            .await
            .map(Some)
    }

    pub async fn list_learner_facts(&self) -> Result<Vec<LearnerFact>, SessionError> {
        match &self.repository {
            Some(repo) => repo.list_learner_facts(&self.user_id, 20).await,
            None => Ok(Vec::new()),
        }
    }

    pub async fn delete_learner_fact(&self, fact_id: &str) -> Result<bool, SessionError> {
        match &self.repository {
            Some(repo) => repo.delete_learner_fact(&self.user_id, fact_id).await,
            None => Ok(false),
        }
    }

    pub async fn clear_learner_facts(&self) -> Result<u64, SessionError> {
        match &self.repository {
            Some(repo) => repo.clear_learner_facts(&self.user_id).await,
            None => Ok(0),
        }
    }

    async fn active_or_scenario_scaffold(
        &self,
        sid: &SessionId,
        scenario_id: &str,
    ) -> PedagogicalScaffold {
        let guard = self.active.lock().await;
        if let Some(mgr) = guard.as_ref().filter(|m| m.session_id() == sid) {
            return mgr.scaffold.clone();
        }
        let sc = find_scenario(scenario_id).unwrap_or_else(default_scenario);
        PedagogicalScaffold {
            scenario_id: sc.id,
            cefr_level: sc.default_cefr,
            goals: sc.goals,
            target_vocabulary: sc.target_vocabulary,
            ..Default::default()
        }
    }
}
