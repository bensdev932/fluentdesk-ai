//! Curated 8 built-in `RoleplayScenario` presets and their Communicative Goals.

use crate::domain::scenario::{CommunicativeGoal, RoleplayScenario};
use crate::domain::scenario_presets_extra::{
    doctor_pharmacy_scenario, restaurant_dining_scenario, team_standup_scenario,
};
use crate::domain::CefrLevel;

pub(super) fn preset_list() -> Vec<RoleplayScenario> {
    vec![
        free_conversation(),
        cafe_order(),
        job_interview(),
        polite_decline(),
        hotel_checkin(),
        restaurant_dining_scenario(),
        doctor_pharmacy_scenario(),
        team_standup_scenario(),
    ]
}

pub(super) fn default_preset() -> RoleplayScenario {
    free_conversation()
}

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

fn free_conversation() -> RoleplayScenario {
    RoleplayScenario {
        id: "free_conversation".into(),
        title: "Everyday Conversation".into(),
        summary: "Natural warm chat about your day, interests, and routines.".into(),
        tutor_persona: "Warm, patient conversational English partner".into(),
        learner_role: "Friendly speaker practicing everyday topics".into(),
        default_cefr: CefrLevel::B1,
        default_max_turns: 6,
        target_vocabulary: vec![],
        goals: vec![
            goal(
                "share_update",
                "Share what happened today or a personal plan",
                "today,yesterday,tomorrow,plan,work,weekend",
            ),
            goal(
                "ask_partner",
                "Ask your partner a natural follow-up question",
                "how about you,what about you,do you,have you,why",
            ),
        ],
    }
}

fn cafe_order() -> RoleplayScenario {
    RoleplayScenario {
        id: "cafe_order".into(),
        title: "Ordering at a Coffee Shop".into(),
        summary: "Order a customized drink, ask about snacks, and pay politely.".into(),
        tutor_persona: "Friendly specialty cafe barista".into(),
        learner_role: "Customer ordering drinks and pastries".into(),
        default_cefr: CefrLevel::A2,
        default_max_turns: 6,
        target_vocabulary: csv("espresso,oat milk,for here,to go,receipt,pastry"),
        goals: vec![
            goal(
                "order_drink",
                "Order a drink with size or milk preference",
                "latte,coffee,tea,espresso,oat milk,cup,size,iced",
            ),
            goal(
                "ask_option",
                "Ask about ingredients, flavors, or food items",
                "pastry,muffin,croissant,gluten,recommend,sugar,have",
            ),
            goal(
                "checkout_choice",
                "Say for here or to go and complete checkout",
                "for here,to go,takeaway,card,cash,receipt,pay",
            ),
        ],
    }
}

fn job_interview() -> RoleplayScenario {
    RoleplayScenario {
        id: "job_interview".into(),
        title: "Job Interview Practice".into(),
        summary: "Present strengths, explain a challenge, and ask thoughtful questions.".into(),
        tutor_persona: "Constructive hiring manager running a behavioral interview".into(),
        learner_role: "Candidate interviewing for a collaborative role".into(),
        default_cefr: CefrLevel::B2,
        default_max_turns: 6,
        target_vocabulary: csv("collaborate,deadline,challenge,impact,strength,feedback"),
        goals: vec![
            goal(
                "intro_strength",
                "Introduce your background and one strength",
                "experience,strength,background,years,worked,good at",
            ),
            goal(
                "past_challenge",
                "Describe how you handled a challenge or deadline",
                "challenge,problem,deadline,solved,team,result",
            ),
            goal(
                "ask_role",
                "Ask the interviewer a question about the team or role",
                "team,culture,day-to-day,expectation,opportunity,role",
            ),
        ],
    }
}

fn polite_decline() -> RoleplayScenario {
    RoleplayScenario {
        id: "polite_decline".into(),
        title: "Declining Politely".into(),
        summary: "Say no diplomatically while keeping the relationship warm.".into(),
        tutor_persona: "Warm colleague inviting you to an extra commitment".into(),
        learner_role: "Colleague declining graciously with an alternative".into(),
        default_cefr: CefrLevel::B1,
        default_max_turns: 6,
        target_vocabulary: csv(
            "appreciate,schedule,unfortunately,another time,rain check,grateful",
        ),
        goals: vec![
            goal(
                "thank_warmly",
                "Thank them sincerely before declining",
                "thank,appreciate,grateful,kind",
            ),
            goal(
                "give_reason",
                "Give a concise, polite reason",
                "unfortunately,schedule,busy,already,commitment,conflict",
            ),
            goal(
                "offer_alternative",
                "Suggest an alternative time or future option",
                "next week,another time,rain check,instead,later,tomorrow",
            ),
        ],
    }
}

fn hotel_checkin() -> RoleplayScenario {
    RoleplayScenario {
        id: "hotel_checkin".into(),
        title: "Hotel Check-In".into(),
        summary: "Confirm your reservation, ask about amenities, and make a request.".into(),
        tutor_persona: "Welcoming front-desk hotel receptionist".into(),
        learner_role: "Arriving guest checking into a hotel".into(),
        default_cefr: CefrLevel::A2,
        default_max_turns: 6,
        target_vocabulary: csv("reservation,confirmation,passport,check-out,breakfast,key card"),
        goals: vec![
            goal(
                "confirm_booking",
                "State your reservation and check-in details",
                "reservation,booking,booked,name,confirmation,nights",
            ),
            goal(
                "ask_amenity",
                "Ask about breakfast, Wi-Fi, or check-out hours",
                "breakfast,wifi,wi-fi,check-out,checkout,included",
            ),
            goal(
                "polite_request",
                "Make a polite room preference or request",
                "quiet,floor,pillow,towel,view,late checkout,could i",
            ),
        ],
    }
}
