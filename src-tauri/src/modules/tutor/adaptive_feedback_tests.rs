//! Unit tests for adaptive roleplay prompt rules and structured feedback guardrails.

use crate::domain::{
    CefrLevel, CorrectionMode, MaxSessionTurns, PedagogicalScaffold, SessionTopic, Transcript,
};
use crate::modules::tutor::{
    feedback::parse_tutor_response_for_utterance, prompts::assemble_scaffolded_tutor_prompt,
};

fn tx(text: &str) -> Transcript {
    Transcript::new(text, "en", 1200, Some(0.95)).expect("valid transcript")
}

fn scaffold(level: CefrLevel) -> PedagogicalScaffold {
    PedagogicalScaffold {
        cefr_level: level,
        topic: SessionTopic::try_new("Cafe Ordering").unwrap(),
        max_turns: MaxSessionTurns::try_new(4).unwrap(),
        ..Default::default()
    }
}

#[test]
fn adaptive_prompt_encodes_cefr_stt_edge_cases_and_four_mode_tiers() {
    let sc = scaffold(CefrLevel::A2);
    let gentle = assemble_scaffolded_tutor_prompt(
        &[],
        &tx("Give me coffee."),
        CorrectionMode::Gentle,
        Some(&sc),
    );
    let sys = gentle.system_instruction();
    assert!(sys.contains("CEFR A2"));
    assert!(sys.contains("Grammar mistakes"));
    assert!(sys.contains("Awkward, unnatural, or literal phrasing"));
    assert!(sys.contains("speech-to-text"));
    assert!(sys.contains("short natural acknowledgements"));
    assert!(sys.contains("another language"));
    assert!(sys.contains("\"spokenReply\" first"));
    assert!(sys.contains("do NOT nitpick"));

    let natural =
        assemble_scaffolded_tutor_prompt(&[], &tx("Hi"), CorrectionMode::Natural, Some(&sc));
    let detailed =
        assemble_scaffolded_tutor_prompt(&[], &tx("Hi"), CorrectionMode::Detailed, Some(&sc));
    let off = assemble_scaffolded_tutor_prompt(&[], &tx("Hi"), CorrectionMode::Off, Some(&sc));
    assert!(natural
        .system_instruction()
        .contains("major clarity blockers"));
    assert!(detailed
        .system_instruction()
        .contains("more fluent/idiomatic"));
    assert!(off
        .system_instruction()
        .contains("\"needsImprovement\": false"));
}

#[test]
fn structured_feedback_parses_camel_and_snake_case_and_populates_summary() {
    let raw = r#"{"spokenReply":"One medium latte coming right up! Hot or iced?","needsImprovement":true,"properWay":"Could I get a medium latte, please?","why":"In a cafe, 'Could I get...' sounds warmer than 'Give me...'.","followUpPrompt":"Hot or iced?"}"#;
    let reply = parse_tutor_response_for_utterance(
        raw,
        CorrectionMode::Gentle,
        Some("Give me medium latte"),
    )
    .unwrap();
    assert_eq!(
        reply.proper_way.as_deref(),
        Some("Could I get a medium latte, please?")
    );
    assert!(reply.why.as_deref().unwrap().contains("Could I get"));
    assert_eq!(
        reply.correction.as_deref(),
        Some("Could I get a medium latte, please? — In a cafe, 'Could I get...' sounds warmer than 'Give me...'.")
    );

    let snake = r#"{"spoken_reply":"Sure!","needs_improvement":true,"proper_way":"Could I have the receipt?","feedback_why":"Add 'the' before 'receipt'."}"#;
    let reply_snake = parse_tutor_response_for_utterance(
        snake,
        CorrectionMode::Detailed,
        Some("Can I have receipt"),
    )
    .unwrap();
    assert_eq!(
        reply_snake.proper_way.as_deref(),
        Some("Could I have the receipt?")
    );
    assert_eq!(
        reply_snake.why.as_deref(),
        Some("Add 'the' before 'receipt'.")
    );
}

#[test]
fn guardrails_suppress_false_flags_placeholders_noop_rewrites_and_off_mode() {
    let false_flag = r#"{"spokenReply":"Sounds great!","needsImprovement":false,"properWay":"Sounds good!","why":"Natural phrasing."}"#;
    let r1 = parse_tutor_response_for_utterance(false_flag, CorrectionMode::Gentle, Some("Okay"))
        .unwrap();
    assert_eq!((r1.proper_way, r1.why, r1.correction), (None, None, None));

    let none_literal =
        r#"{"spokenReply":"Sure thing!","needsImprovement":true,"properWay":"NONE","why":"NONE"}"#;
    let r2 = parse_tutor_response_for_utterance(none_literal, CorrectionMode::Gentle, Some("Sure"))
        .unwrap();
    assert_eq!((r2.proper_way, r2.why, r2.correction), (None, None, None));

    let noop = r#"{"spokenReply":"Great choice!","needsImprovement":true,"properWay":"I would like an iced latte.","why":"Sounds natural."}"#;
    let r3 = parse_tutor_response_for_utterance(
        noop,
        CorrectionMode::Gentle,
        Some("i would like an iced latte"),
    )
    .unwrap();
    assert_eq!((r3.proper_way, r3.why, r3.correction), (None, None, None));

    let off = r#"{"spokenReply":"No problem!","needsImprovement":true,"properWay":"I went yesterday.","why":"Use past tense."}"#;
    let r4 = parse_tutor_response_for_utterance(off, CorrectionMode::Off, Some("I go yesterday."))
        .unwrap();
    assert_eq!((r4.proper_way, r4.why, r4.correction), (None, None, None));
}
