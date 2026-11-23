//! Deterministic, persona-tailored pure-English opening `TutorReply` builder for `SessionPhase::Opener`.

use crate::domain::{CefrLevel, TutorReply};

/// Builds a zero-wait, in-character English opening `TutorReply` for the active `RoleplayScenario`.
pub fn build_scenario_opener(
    scenario_id: &str,
    topic: &str,
    learner_facts: &[String],
) -> TutorReply {
    build_scenario_opener_for_cefr(scenario_id, topic, learner_facts, CefrLevel::B1)
}

/// Builds a CEFR-adapted, warm native-English opening `TutorReply` with starter suggestions.
pub fn build_scenario_opener_for_cefr(
    scenario_id: &str,
    topic: &str,
    learner_facts: &[String],
    cefr: CefrLevel,
) -> TutorReply {
    let basic = matches!(cefr, CefrLevel::A1 | CefrLevel::A2);
    let (spoken, prompt, replies): (&str, &str, &[&str]) = match (scenario_id, basic) {
        ("cafe_order", true) => (
            "Hi! Welcome to Sunrise Cafe. What can I get you today?",
            "Hot coffee, tea, or a snack?",
            &["Hi! Can I get a coffee, please?", "One iced latte, please.", "Just a tea, thanks."],
        ),
        ("cafe_order", false) => (
            "Morning! Welcome to Sunrise Cafe. What can I get started for you?",
            "What drink or size sounds good today?",
            &["Could I get a medium iced latte, please?", "I'll grab a hot oat cappuccino.", "What pastries do you have?"],
        ),
        ("job_interview", true) => (
            "Hi, nice to meet you! Thanks for coming in. Can you tell me a little about yourself?",
            "Where did you work before?",
            &["Hi! Nice to meet you too.", "I'm a web developer with two years of experience.", "I used to work in customer support."],
        ),
        ("job_interview", false) => (
            "Hi, thanks so much for hopping on today! To kick things off, could you walk me through your background?",
            "What kind of work have you been focused on lately?",
            &["Thanks for having me! Lately I've been...", "Sure, I've spent three years working on...", "I'm excited to tell you about my last project."],
        ),
        ("polite_decline", true) => (
            "Hey! I'm having a dinner party this Sunday. Can you come?",
            "How can you say no nicely?",
            &["Thanks for inviting me, but I'm busy Sunday.", "That sounds fun, sorry I can't make it!", "I'd love to, maybe next time?"],
        ),
        ("polite_decline", false) => (
            "Hey! I'm throwing a dinner party this Sunday night and would love for you to come. Can you make it?",
            "How can you turn it down warmly?",
            &["Thanks so much for the invite, though I already have plans.", "I really appreciate you asking, but I won't be able to make it.", "Wish I could, I'm tied up Sunday night!"],
        ),
        ("hotel_checkin", true) => (
            "Hi! Welcome to the Grand Harbor Hotel. Checking in?",
            "What's your name, please?",
            &["Hi! Yes, I'm checking in.", "I have a booking under my name.", "Is breakfast included?"],
        ),
        ("hotel_checkin", false) => (
            "Good evening, welcome to the Grand Harbor Hotel! Are you checking in with us tonight?",
            "Could I grab the name on your reservation?",
            &["Hi there, yes! I have a reservation under...", "I'm checking in for two nights.", "Could I ask for a quiet room if possible?"],
        ),
        ("restaurant_dining", true) => (
            "Hi! Welcome in. Here's your menu. Would you like some water first?",
            "What would you like to drink or eat?",
            &["Yes, just water please.", "What's good today?", "Can I have the chicken, please?"],
        ),
        ("restaurant_dining", false) => (
            "Welcome to The Olive Table! Here's a menu for you. Can I start you off with something to drink or tell you today's specials?",
            "What would you like to ask your server?",
            &["I'd love to hear today's specials!", "Could we start with sparkling water?", "Does the pasta dish have nuts?"],
        ),
        ("doctor_pharmacy", true) => (
            "Hi there. How are you feeling today?",
            "What hurts or feels wrong?",
            &["I have a bad headache.", "I've had a cough since yesterday.", "Do you have something for a cold?"],
        ),
        ("doctor_pharmacy", false) => (
            "Hi there, welcome to the pharmacy. What brings you in today—how have you been feeling?",
            "What symptoms are bothering you most?",
            &["I've had a sore throat and fever since yesterday.", "Could you recommend something for a cough?", "How often should I take this?"],
        ),
        ("team_standup", true) => (
            "Morning! Quick check-in: what did you do yesterday, and what's next today?",
            "What are you working on today?",
            &["Yesterday I fixed a bug. Today I'm testing.", "I finished the login page.", "I need help with one task."],
        ),
        ("team_standup", false) => (
            "Morning everyone! Let's jump into standup. How did things go yesterday, and what's on your plate today?",
            "Any updates or blockers to flag?",
            &["Yesterday I wrapped up the API changes, and today I'm writing tests.", "I'm mostly unblocked, just waiting on a review.", "I hit a snag with auth and could use a second pair of eyes."],
        ),
        _ => return build_free_conversation_opener(topic, learner_facts, basic),
    };
    build_reply(spoken, prompt, replies)
}

fn build_free_conversation_opener(
    topic: &str,
    learner_facts: &[String],
    basic: bool,
) -> TutorReply {
    let clean_topic = topic.trim();
    let has_custom = !clean_topic.is_empty()
        && !clean_topic.eq_ignore_ascii_case("Free Conversation")
        && !clean_topic.eq_ignore_ascii_case("Everyday Conversation");
    let (spoken, prompt, replies): (String, &str, &[&str]) = if has_custom {
        (
            format!(
                "Hey! Let's chat about {clean_topic}. What's the first thing that comes to mind?"
            ),
            "What do you think about it?",
            &[
                "I really like talking about this.",
                "Actually, I did something fun recently.",
                "Can you ask me an easy question first?",
            ],
        )
    } else if let Some(fact) = learner_facts.first() {
        (
            format!(
                "Hey, good to see you again! Earlier you mentioned {fact}. How's your day going?"
            ),
            "How's your day been so far?",
            &[
                "Pretty good, thanks! How about you?",
                "Busy day, actually!",
                "I just had some free time to practice.",
            ],
        )
    } else if basic {
        (
            "Hi! Nice to meet you. How's your day going?".into(),
            "How are you today?",
            &[
                "I'm good, thanks! And you?",
                "I'm a little tired today.",
                "It's going well!",
            ],
        )
    } else {
        (
            "Hey there! Glad we're chatting today. How's your day going so far?".into(),
            "What's been the highlight of your day?",
            &[
                "Pretty good so far! I've been...",
                "Honest answer? I really needed coffee this morning.",
                "Let's talk about my weekend plans.",
            ],
        )
    };
    build_reply(&spoken, prompt, replies)
}

fn build_reply(spoken: &str, prompt: &str, suggestions: &[&str]) -> TutorReply {
    let suggested = suggestions
        .iter()
        .map(|s| (*s).to_string())
        .collect::<Vec<_>>();
    TutorReply::new_validated(spoken, None, Some(prompt.to_string()))
        .map(|r| r.with_suggested_replies(suggested.clone()))
        .unwrap_or_else(|_| TutorReply {
            spoken_reply: spoken.to_string(),
            correction: None,
            proper_way: None,
            why: None,
            follow_up_prompt: Some(prompt.to_string()),
            completed_goal_ids: Vec::new(),
            suggested_replies: suggested,
        })
}
