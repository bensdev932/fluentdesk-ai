//! Roleplay Scenarios, Communicative Goals, and deterministic goal matching.

use crate::domain::scenario_presets::{default_preset, preset_list};
use crate::domain::CefrLevel;
use serde::{Deserialize, Serialize};

/// Concrete real-world conversational objective within a Roleplay Scenario.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommunicativeGoal {
    pub id: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keywords: Vec<String>,
}

/// Curated speaking roleplay context with persona, role, vocabulary, and Communicative Goals.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleplayScenario {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub tutor_persona: String,
    pub learner_role: String,
    pub default_cefr: CefrLevel,
    pub default_max_turns: u32,
    pub target_vocabulary: Vec<String>,
    pub goals: Vec<CommunicativeGoal>,
}

pub fn built_in_scenarios() -> Vec<RoleplayScenario> {
    preset_list()
}

pub fn default_scenario() -> RoleplayScenario {
    default_preset()
}

pub fn find_scenario(id: &str) -> Option<RoleplayScenario> {
    let clean = id.trim();
    built_in_scenarios().into_iter().find(|s| s.id == clean)
}

/// Evaluates uncompleted goals against the learner's utterance using keyword matching.
pub fn detect_completed_goal_ids(
    goals: &[CommunicativeGoal],
    learner_text: &str,
    already_done: &[String],
) -> Vec<String> {
    let lower = learner_text.to_ascii_lowercase();
    goals
        .iter()
        .filter(|g| !already_done.iter().any(|id| id == &g.id))
        .filter(|g| goal_matched(&lower, g))
        .map(|g| g.id.clone())
        .collect()
}

fn goal_matched(lower_text: &str, goal: &CommunicativeGoal) -> bool {
    !goal.keywords.is_empty()
        && goal
            .keywords
            .iter()
            .any(|kw| lower_text.contains(&kw.to_ascii_lowercase()))
}

impl Default for crate::domain::PedagogicalScaffold {
    fn default() -> Self {
        let sc = default_scenario();
        Self {
            cefr_level: CefrLevel::default(),
            topic: crate::domain::SessionTopic::default(),
            target_vocabulary: Vec::new(),
            max_turns: crate::domain::MaxSessionTurns::default(),
            user_turns_completed: 0,
            scenario_id: sc.id,
            goals: sc.goals,
            completed_goal_ids: Vec::new(),
            learner_facts: Vec::new(),
        }
    }
}

impl crate::domain::PedagogicalScaffold {
    pub fn mark_goals_completed(&mut self, ids: impl IntoIterator<Item = String>) {
        for id in ids {
            let clean = id.trim();
            let valid = self.goals.iter().any(|g| g.id == clean);
            if valid && !self.completed_goal_ids.iter().any(|done| done == clean) {
                self.completed_goal_ids.push(clean.to_string());
            }
        }
    }
}
