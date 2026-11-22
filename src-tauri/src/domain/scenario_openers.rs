//! Deterministic, persona-tailored pure-English opening `TutorReply` builder for `SessionPhase::Opener`.

use crate::domain::TutorReply;

/// Builds a zero-wait, in-character English opening `TutorReply` for the active `RoleplayScenario`.
pub fn build_scenario_opener(
    scenario_id: &str,
    topic: &str,
    learner_facts: &[String],
) -> TutorReply {
    let (spoken, prompt) = match scenario_id {
        "cafe_order" => (
            "Good morning! Welcome to Sunrise Cafe. What can I get started for you today?",
            "What drink or size would you like to order?",
        ),
        "job_interview" => (
            "Hello and welcome! Thank you for joining this interview today. Could you start by introducing yourself and your background?",
            "How would you introduce your recent experience?",
        ),
        "polite_decline" => (
            "Hey! I am hosting a late dinner party this Sunday night and would love for you to come. Can you make it?",
            "How can you thank them and politely decline?",
        ),
        "hotel_checkin" => (
            "Good evening, welcome to the Grand Harbor Hotel! How may I help you check in today?",
            "Could you share your reservation details?",
        ),
        "restaurant_dining" => (
            "Welcome to The Olive Table! Here is your menu. Would you like to hear today's specials before you order?",
            "What would you like to ask the server first?",
        ),
        "doctor_pharmacy" => (
            "Hello! Welcome to the clinic pharmacy. Tell me how you are feeling today and how I can help.",
            "What symptoms have you been experiencing?",
        ),
        "team_standup" => (
            "Good morning! Let's kick off our daily standup. How did your work go yesterday, and what is your main priority today?",
            "What progress or blocker would you like to share?",
        ),
        _ => return build_free_conversation_opener(topic, learner_facts),
    };
    TutorReply::new_validated(spoken, None, Some(prompt.to_string())).unwrap_or_else(|_| {
        TutorReply {
            spoken_reply: spoken.to_string(),
            correction: None,
            follow_up_prompt: Some(prompt.to_string()),
            completed_goal_ids: Vec::new(),
        }
    })
}

fn build_free_conversation_opener(topic: &str, learner_facts: &[String]) -> TutorReply {
    let clean_topic = topic.trim();
    let has_custom_topic = !clean_topic.is_empty()
        && !clean_topic.eq_ignore_ascii_case("Free Conversation")
        && !clean_topic.eq_ignore_ascii_case("Everyday Conversation");
    let spoken = if has_custom_topic {
        format!("Hi there! Let's practice speaking English about {clean_topic}. What comes to mind first?")
    } else if let Some(fact) = learner_facts.first() {
        format!(
            "Welcome back! I remember you mentioned: {fact}. How has your day been going so far?"
        )
    } else {
        "Hi there! I am your English speaking tutor. How is your day going, and what would you like to talk about?".to_string()
    };
    let prompt = "How is your day going so far?".to_string();
    TutorReply::new_validated(&spoken, None, Some(prompt.clone())).unwrap_or(TutorReply {
        spoken_reply: spoken,
        correction: None,
        follow_up_prompt: Some(prompt),
        completed_goal_ids: Vec::new(),
    })
}
