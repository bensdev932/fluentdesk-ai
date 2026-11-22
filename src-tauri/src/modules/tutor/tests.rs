//! Unit tests for context ordering, context limits, correction modes, and tutor response parsing (`T030`, `T032`, `T033`).

use crate::domain::{ConversationRole, ConversationTurn, CorrectionMode, SessionId, Transcript};
use crate::modules::conversation::context::{BoundedConversationContext, MaxContextTurns};
use crate::modules::tutor::{
    feedback::{extract_follow_up_prompt, parse_tutor_response_or_fallback},
    prompts::assemble_voice_tutor_prompt,
};

fn sample_turn(id: &str, role: ConversationRole, text: &str, ts: i64) -> ConversationTurn {
    ConversationTurn {
        id: id.to_string(),
        session_id: SessionId::new("s-tutor-test").expect("valid session id"),
        role,
        text: text.to_string(),
        correction: None,
        follow_up_prompt: None,
        timestamp_unix_ms: ts,
        stage_metadata: None,
    }
}

fn sample_transcript(text: &str) -> Transcript {
    Transcript::new(text, "en", 1500, Some(0.95)).expect("valid transcript")
}

#[test]
fn max_context_turns_rejects_zero_and_excessive_values() {
    assert!(MaxContextTurns::try_new(0).is_err());
    assert!(MaxContextTurns::try_new(51).is_err());
    assert_eq!(MaxContextTurns::try_new(8).expect("valid").as_usize(), 8);
}

#[test]
fn bounded_context_preserves_chronological_order_and_evicts_oldest() {
    let limit = MaxContextTurns::try_new(3).expect("valid limit");
    let mut ctx = BoundedConversationContext::new(limit);
    ctx.push_turn(sample_turn("t1", ConversationRole::User, "Hello", 100));
    ctx.push_turn(sample_turn("t2", ConversationRole::Tutor, "Hi there!", 200));
    ctx.push_turn(sample_turn(
        "t3",
        ConversationRole::User,
        "I go yesterday",
        300,
    ));
    ctx.push_turn(sample_turn(
        "t4",
        ConversationRole::Tutor,
        "Oh, where did you go?",
        400,
    ));
    let ids: Vec<String> = ctx.recent_turns().into_iter().map(|t| t.id).collect();
    assert_eq!(ids, vec!["t2", "t3", "t4"]);
}

#[test]
fn prompt_assembly_includes_chronological_history_and_correction_mode_rules() {
    let history = vec![
        sample_turn("t1", ConversationRole::User, "I like coffee.", 100),
        sample_turn("t2", ConversationRole::Tutor, "How do you take it?", 200),
    ];
    let transcript = sample_transcript("Yesterday I drinked two cups.");
    let gentle = assemble_voice_tutor_prompt(&history, &transcript, CorrectionMode::Gentle);
    let off = assemble_voice_tutor_prompt(&history, &transcript, CorrectionMode::Off);
    assert!(gentle
        .system_instruction()
        .contains("brief gentle correction"));
    assert!(off
        .system_instruction()
        .contains("Do NOT provide grammar corrections"));
    assert!(gentle.user_prompt().contains("User: I like coffee."));
    assert!(gentle
        .user_prompt()
        .contains("Yesterday I drinked two cups."));
}

#[test]
fn parse_tutor_response_handles_structured_json_and_code_fences() {
    let fenced = "```json\n{\"spokenReply\":\"I drank two cups too! Do you prefer hot or iced?\",\"correction\":\"Say 'I drank' instead of 'I drinked'.\",\"followUpPrompt\":\"Do you prefer hot or iced?\"}\n```";
    let reply = parse_tutor_response_or_fallback(fenced, CorrectionMode::Gentle)
        .expect("fenced JSON should parse");
    assert_eq!(
        reply.spoken_reply,
        "I drank two cups too! Do you prefer hot or iced?"
    );
    assert_eq!(
        reply.correction.as_deref(),
        Some("Say 'I drank' instead of 'I drinked'.")
    );
    assert_eq!(
        reply.follow_up_prompt.as_deref(),
        Some("Do you prefer hot or iced?")
    );
}

#[test]
fn parse_tutor_response_suppresses_correction_when_mode_is_off() {
    let raw = "{\"spokenReply\":\"That sounds great! What else did you do?\",\"correction\":\"Unused tip\"}";
    let reply = parse_tutor_response_or_fallback(raw, CorrectionMode::Off).expect("valid JSON");
    assert_eq!(
        reply.spoken_reply,
        "That sounds great! What else did you do?"
    );
    assert_eq!(reply.correction, None);
}

#[test]
fn parse_tutor_response_falls_back_to_plain_text_on_malformed_json() {
    let malformed = "That sounds fun! Where did you travel last summer?";
    let reply = parse_tutor_response_or_fallback(malformed, CorrectionMode::Natural)
        .expect("plain text fallback should succeed");
    assert_eq!(
        reply.spoken_reply,
        "That sounds fun! Where did you travel last summer?"
    );
    assert_eq!(reply.correction, None);
    assert_eq!(
        reply.follow_up_prompt.as_deref(),
        Some("Where did you travel last summer?")
    );
    assert!(parse_tutor_response_or_fallback("   \n\t  ", CorrectionMode::Natural).is_err());
}

#[test]
fn prompt_requires_reacting_to_user_details_and_bans_canned_filler() {
    let history = vec![
        sample_turn(
            "t1",
            ConversationRole::User,
            "I adopted a beagle named Milo.",
            100,
        ),
        sample_turn(
            "t2",
            ConversationRole::Tutor,
            "Milo sounds adorable! How old is he?",
            200,
        ),
    ];
    let tx = sample_transcript("He is eight months old and chewed my running shoes.");
    let prompt = assemble_voice_tutor_prompt(&history, &tx, CorrectionMode::Natural);
    let sys = prompt.system_instruction();
    assert!(sys.contains("React directly to the specific details"));
    assert!(sys.contains("Never use canned filler"));
    assert!(sys.contains("Thanks for sharing that naturally"));
    assert!(prompt
        .user_prompt()
        .contains("User: I adopted a beagle named Milo."));
    assert!(prompt
        .user_prompt()
        .contains("Tutor: Milo sounds adorable! How old is he?"));
    assert!(prompt.user_prompt().contains("chewed my running shoes."));
}

#[test]
fn feedback_extracts_follow_up_dynamically_and_returns_none_without_question() {
    let json_no_followup =
        "{\"spokenReply\":\"Oh no, poor shoes! Was it a brand-new pair?\",\"correction\":\"null\"}";
    let r1 = parse_tutor_response_or_fallback(json_no_followup, CorrectionMode::Gentle).unwrap();
    assert_eq!(r1.correction, None);
    assert_eq!(
        r1.follow_up_prompt.as_deref(),
        Some("Was it a brand-new pair?")
    );

    let with_think = "<think>User talked about tea at $4.50.</think>\n```json\n{\"spokenReply\":\"Paying $4.50 for matcha is steep. Did it taste worth it?\"}\n```";
    let r2 = parse_tutor_response_or_fallback(with_think, CorrectionMode::Natural).unwrap();
    assert_eq!(
        r2.follow_up_prompt.as_deref(),
        Some("Did it taste worth it?")
    );

    let statement_only = "{\"spokenReply\":\"That sounds like a relaxing evening with Milo.\"}";
    let r3 = parse_tutor_response_or_fallback(statement_only, CorrectionMode::Natural).unwrap();
    assert_eq!(r3.follow_up_prompt, None);
    let dup = "{\"spokenReply\":\"Where did you go?\",\"followUpPrompt\":\"Where did you go?\"}";
    let r4 = parse_tutor_response_or_fallback(dup, CorrectionMode::Natural).unwrap();
    assert_eq!(r4.follow_up_prompt, None);
    let upper_think = "<THINK>hidden</THINK>{\"spokenReply\":\"Great run! Did it rain?\"}\n";
    let r5 = parse_tutor_response_or_fallback(upper_think, CorrectionMode::Off).unwrap();
    assert_eq!(r5.follow_up_prompt.as_deref(), Some("Did it rain?"));
    assert_eq!(
        extract_follow_up_prompt("First question? Second question?"),
        Some("Second question?".into())
    );
}
