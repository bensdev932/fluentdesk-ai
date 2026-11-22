//! Tests for incremental sentence extraction across streaming LLM tutor tokens.

use crate::modules::tutor::stream_parser::SpokenJsonStreamParser;

#[test]
fn extracts_sentences_incrementally_as_json_tokens_stream() {
    let mut parser = SpokenJsonStreamParser::new();
    assert!(parser.push_token("{\"spoken").is_empty());
    assert!(parser.push_token("Reply\":\"Hi ").is_empty());
    assert!(parser.push_token("Ben!").is_empty());
    assert_eq!(parser.push_token(" You").as_slice(), ["Hi Ben!"]);
    assert_eq!(
        parser.push_token(" sounded clear. ").as_slice(),
        ["You sounded clear."]
    );
    assert_eq!(parser.push_token("What next?\"").as_slice(), ["What next?"]);
    assert!(parser
        .push_token(",\"correction\":null,\"followUpPrompt\":\"What next?\"}")
        .is_empty());
    let (tail, raw) = parser.finish();
    assert!(tail.is_empty());
    assert!(raw.contains("\"correction\":null"));
}

#[test]
fn handles_escaped_quotes_newlines_and_unicode_inside_spoken_reply() {
    let mut parser = SpokenJsonStreamParser::new();
    let chunks = [
        "```json\n{\n  \"spoken_reply\": \"He said \\\"Hi!\\\" ",
        "\\u2014 really?\\nYes!\", \"correction\": null}\n```",
    ];
    let mut emitted = Vec::new();
    for chunk in chunks {
        emitted.extend(parser.push_token(chunk));
    }
    let (tail, _) = parser.finish();
    emitted.extend(tail);
    assert_eq!(emitted, vec!["He said \"Hi!\" \u{2014} really?", "Yes!"]);
}

#[test]
fn ignores_correction_when_spoken_reply_comes_second() {
    let mut parser = SpokenJsonStreamParser::new();
    let mut emitted = parser.push_token(
        "{\"correction\":\"Use went instead of go.\",\"spokenReply\":\"Got it! Where did you go?\"}",
    );
    let (tail, _) = parser.finish();
    emitted.extend(tail);
    assert_eq!(emitted, vec!["Got it!", "Where did you go?"]);
}

#[test]
fn falls_back_to_plain_text_streaming_when_model_drops_json() {
    let mut parser = SpokenJsonStreamParser::new();
    assert!(parser.push_token("Sounds delicious").is_empty());
    assert_eq!(
        parser.push_token("! What did").as_slice(),
        ["Sounds delicious!"]
    );
    assert!(parser.push_token(" you cook?").is_empty());
    let (tail, raw) = parser.finish();
    assert_eq!(tail, vec!["What did you cook?"]);
    assert_eq!(raw, "Sounds delicious! What did you cook?");
}

#[test]
fn strips_think_blocks_and_ignores_literal_spoken_reply_value() {
    let mut parser = SpokenJsonStreamParser::new();
    let mut emitted = parser.push_token("<think>internal notes.</think>\n");
    emitted.extend(
        parser.push_token("{\"correction\":\"spokenReply\",\"spokenReply\":\"Nice job!\"}"),
    );
    let (tail, _) = parser.finish();
    emitted.extend(tail);
    assert_eq!(emitted, vec!["Nice job!"]);
}
