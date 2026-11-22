//! Tests for Three-Character Paradigm & 4-phase pedagogical prompt scaffolding (`voice-research-upgrades` #02).

use crate::domain::{
    CefrLevel, CorrectionMode, MaxSessionTurns, PedagogicalScaffold, SessionTopic, Transcript,
};
use crate::modules::tutor::prompts::assemble_scaffolded_tutor_prompt;

fn sample_tx() -> Transcript {
    Transcript::new("I have my bag here.", "en", 1200, Some(0.95)).unwrap()
}

fn make_scaffold(completed: u32, max_turns: u32) -> PedagogicalScaffold {
    PedagogicalScaffold {
        cefr_level: CefrLevel::A2,
        topic: SessionTopic::try_new("Airport Check-in").unwrap(),
        target_vocabulary: vec!["boarding pass".into(), "luggage".into()],
        max_turns: MaxSessionTurns::try_new(max_turns).unwrap(),
        user_turns_completed: completed,
        ..Default::default()
    }
}

#[test]
fn scaffolded_prompt_injects_cefr_topic_vocab_and_all_four_phases() {
    let opener = assemble_scaffolded_tutor_prompt(
        &[],
        &sample_tx(),
        CorrectionMode::Natural,
        Some(&make_scaffold(0, 4)),
    );
    let sys0 = opener.system_instruction();
    assert!(sys0.contains("CEFR A2"));
    assert!(sys0.contains("Airport Check-in"));
    assert!(sys0.contains("boarding pass"));
    assert!(sys0.contains("luggage"));
    assert!(sys0.contains("Opener"));
    assert!(sys0.contains("confused"));

    let q1 = assemble_scaffolded_tutor_prompt(
        &[],
        &sample_tx(),
        CorrectionMode::Natural,
        Some(&make_scaffold(1, 4)),
    );
    assert!(q1.system_instruction().contains("First Question"));

    let mid = assemble_scaffolded_tutor_prompt(
        &[],
        &sample_tx(),
        CorrectionMode::Natural,
        Some(&make_scaffold(2, 4)),
    );
    assert!(mid.system_instruction().contains("Topic Exchange"));
    assert!(mid.system_instruction().contains("2 turns left"));

    let closer = assemble_scaffolded_tutor_prompt(
        &[],
        &sample_tx(),
        CorrectionMode::Natural,
        Some(&make_scaffold(3, 4)),
    );
    assert!(closer.system_instruction().contains("Closer"));
    assert!(closer
        .system_instruction()
        .contains("Set followUpPrompt to null"));
}
