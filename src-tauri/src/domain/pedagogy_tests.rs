//! Unit tests for CEFR proficiency, session topic, turn bounds, and 4-phase scaffold transitions.

use crate::domain::{CefrLevel, MaxSessionTurns, PedagogicalScaffold, SessionPhase, SessionTopic};
use std::str::FromStr;

#[test]
fn max_session_turns_enforces_safe_bounds() {
    assert!(MaxSessionTurns::try_new(1).is_err());
    assert!(MaxSessionTurns::try_new(21).is_err());
    assert_eq!(MaxSessionTurns::try_new(4).unwrap().as_u32(), 4);
}

#[test]
fn session_topic_sanitizes_control_chars_and_rejects_empty() {
    assert!(SessionTopic::try_new("   \n\t ").is_err());
    let topic = SessionTopic::try_new("  Ordering\u{0007} Coffee  ").unwrap();
    assert_eq!(topic.as_str(), "Ordering Coffee");
}

#[test]
fn cefr_level_roundtrips_case_insensitively() {
    assert_eq!(CefrLevel::from_str("a2").unwrap(), CefrLevel::A2);
    assert_eq!(CefrLevel::C1.as_str(), "C1");
    assert!(CefrLevel::from_str("Z9").is_err());
}

#[test]
fn pedagogical_scaffold_walks_all_four_phases_then_completes() {
    let mut s = PedagogicalScaffold {
        cefr_level: CefrLevel::A2,
        topic: SessionTopic::try_new("Airport Check-in").unwrap(),
        target_vocabulary: vec!["boarding pass".into(), "luggage".into()],
        max_turns: MaxSessionTurns::try_new(4).unwrap(),
        user_turns_completed: 0,
        ..Default::default()
    };
    assert_eq!(s.current_phase(), SessionPhase::Opener);
    assert_eq!(s.remaining_turns(), 4);
    assert!(!s.is_completed());

    assert_eq!(s.advance_after_turn(), SessionPhase::FirstQuestion);
    assert_eq!(s.advance_after_turn(), SessionPhase::TopicExchange);
    assert_eq!(s.advance_after_turn(), SessionPhase::Closer);
    assert_eq!(s.remaining_turns(), 1);
    assert_eq!(s.advance_after_turn(), SessionPhase::Completed);
    assert_eq!(s.remaining_turns(), 0);
    assert!(s.is_completed());
}

#[test]
fn built_in_scenarios_cover_eight_pure_english_contexts_with_openers() {
    let scenarios = crate::domain::built_in_scenarios();
    assert_eq!(scenarios.len(), 8);
    for sc in &scenarios {
        let opener = crate::domain::build_scenario_opener(&sc.id, &sc.title, &[]);
        assert!(!opener.spoken_reply.trim().is_empty());
        assert!(opener.follow_up_prompt.is_some());
    }
}
