//! Additional built-in pure-English Roleplay Scenario presets (Restaurant, Doctor/Pharmacy, Team Standup).

use crate::domain::pedagogy::{CefrLevel, DEFAULT_MAX_SESSION_TURNS};
use crate::domain::scenario::{CommunicativeGoal, RoleplayScenario};

fn csv(items: &str) -> Vec<String> {
    items
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn goal(id: &str, description: &str, keywords: &str) -> CommunicativeGoal {
    CommunicativeGoal {
        id: id.into(),
        description: description.into(),
        keywords: csv(keywords),
    }
}

pub(super) fn restaurant_dining_scenario() -> RoleplayScenario {
    RoleplayScenario {
        id: "restaurant_dining".to_string(),
        title: "Restaurant Dining".to_string(),
        summary: "Order a meal in English, ask about ingredients, and request the bill."
            .to_string(),
        tutor_persona: "Friendly restaurant server taking dinner orders in English".to_string(),
        learner_role: "Diner ordering a main course and asking about menu options".to_string(),
        default_cefr: CefrLevel::A2,
        default_max_turns: DEFAULT_MAX_SESSION_TURNS,
        target_vocabulary: csv("recommend, special, side dish, allergic, bill"),
        goals: vec![
            goal(
                "ask_recommendation",
                "Ask the server for a recommendation or daily special",
                "recommend, special, popular, suggest",
            ),
            goal(
                "order_main_and_side",
                "Order a main dish with a side or dietary preference",
                "order, side, salad, allergic, chicken, pasta, steak",
            ),
            goal(
                "request_bill",
                "Ask for the bill or payment method",
                "bill, check, card, pay",
            ),
        ],
    }
}

pub(super) fn doctor_pharmacy_scenario() -> RoleplayScenario {
    RoleplayScenario {
        id: "doctor_pharmacy".to_string(),
        title: "Doctor or Pharmacy Visit".to_string(),
        summary: "Describe symptoms clearly in English and confirm medicine dosage.".to_string(),
        tutor_persona: "Caring clinic pharmacist helping a patient in English".to_string(),
        learner_role: "Patient explaining symptoms and asking how to take medication".to_string(),
        default_cefr: CefrLevel::B1,
        default_max_turns: DEFAULT_MAX_SESSION_TURNS,
        target_vocabulary: csv("symptoms, dosage, twice a day, side effects, relief"),
        goals: vec![
            goal(
                "describe_symptoms",
                "Describe your symptoms and how long you have had them",
                "symptom, headache, throat, cough, fever, since, days",
            ),
            goal(
                "confirm_dosage",
                "Ask how often or when to take the medicine",
                "dosage, often, twice, meal, take, hours",
            ),
            goal(
                "check_side_effects",
                "Ask about possible side effects or precautions",
                "side effect, drowsy, avoid, safe",
            ),
        ],
    }
}

pub(super) fn team_standup_scenario() -> RoleplayScenario {
    RoleplayScenario {
        id: "team_standup".to_string(),
        title: "Team Standup & Workplace Small Talk".to_string(),
        summary: "Share work progress, flag a blocker, and align on next steps in English."
            .to_string(),
        tutor_persona: "Supportive engineering lead running a daily standup in English".to_string(),
        learner_role: "Teammate sharing yesterday's progress, today's priority, and a blocker"
            .to_string(),
        default_cefr: CefrLevel::B2,
        default_max_turns: DEFAULT_MAX_SESSION_TURNS,
        target_vocabulary: csv("progress, blocker, priority, follow up, deadline"),
        goals: vec![
            goal(
                "share_progress",
                "Summarize what you completed recently",
                "completed, finished, progress, worked on, shipped",
            ),
            goal(
                "flag_blocker",
                "Explain a current blocker or dependency clearly",
                "blocker, blocked, waiting, review, issue",
            ),
            goal(
                "align_next_priority",
                "State your next priority or follow-up plan",
                "priority, next, follow up, deadline, focus",
            ),
        ],
    }
}
