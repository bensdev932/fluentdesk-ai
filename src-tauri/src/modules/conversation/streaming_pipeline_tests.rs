//! Concurrent LLM-to-TTS streaming pipeline verification tests (`voice-research-upgrades` #01).

use crate::domain::{SessionErrorKind, SessionState};
use crate::modules::conversation::streaming_pipeline_helpers::make_streaming_case;
use crate::modules::speech::{
    synthesis::{PlaybackLifecycleStage, TutorPlaybackOutcome},
    utterance::CompletedUtterance,
};
use std::sync::atomic::Ordering;
use std::time::Duration;

#[tokio::test]
async fn spoken_turn_plays_first_sentence_before_llm_stream_completes() {
    let c = make_streaming_case(false).await;
    let utt = CompletedUtterance::try_new(vec![0.1; 8_000], 500, false).unwrap();
    let (hc, sc) = (c.host.clone(), c.sid.clone());
    let task = tokio::spawn(async move { hc.process_spoken_utterance(&sc, &utt, 5_000).await });

    tokio::time::timeout(Duration::from_secs(2), c.first_played.notified())
        .await
        .expect("sentence 1 must start audio playback before LLM finishes");
    assert!(!c.llm_done.load(Ordering::SeqCst));
    assert_eq!(
        c.host.get_status(&c.sid).await.unwrap().state,
        SessionState::Speaking
    );

    let _ = c.gate_tx.send(true);
    let res = task.await.unwrap().expect("turn should complete");
    assert_eq!(res.final_state, SessionState::Listening);
    assert_eq!(
        res.tutor_text_event.unwrap().reply.spoken_reply,
        "First sentence. Second sentence."
    );
    assert_eq!(c.played.lock().expect("lock").len(), 2);
    assert_eq!(
        c.repo.list_turns_chronological(&c.sid).await.unwrap().len(),
        2
    );
}

#[tokio::test]
async fn cancelling_mid_stream_aborts_tts_and_calls_stop_playback() {
    let c = make_streaming_case(false).await;
    let utt = CompletedUtterance::try_new(vec![0.1; 8_000], 500, false).unwrap();
    let (hc, sc) = (c.host.clone(), c.sid.clone());
    let task = tokio::spawn(async move { hc.process_spoken_utterance(&sc, &utt, 6_000).await });

    tokio::time::timeout(Duration::from_secs(2), c.first_played.notified())
        .await
        .expect("sentence 1 plays");
    c.host.stop_session(&c.sid).await.unwrap();
    assert_eq!(
        task.await.unwrap().unwrap_err().kind,
        SessionErrorKind::Cancelled
    );
    assert!(c.stops.load(Ordering::SeqCst) >= 1);
}

#[tokio::test]
async fn mid_stream_audio_failure_degrades_gracefully_without_losing_reply() {
    let c = make_streaming_case(true).await;
    let utt = CompletedUtterance::try_new(vec![0.1; 8_000], 500, false).unwrap();
    let _ = c.gate_tx.send(true);
    let res = c
        .host
        .process_spoken_utterance(&c.sid, &utt, 7_000)
        .await
        .unwrap();
    let pb = res.playback_event.unwrap();
    assert!(pb
        .stages
        .contains(&PlaybackLifecycleStage::DegradedTextOnly));
    assert!(matches!(
        pb.outcome,
        TutorPlaybackOutcome::TextOnlyDegraded { .. }
    ));
    assert_eq!(
        c.repo.list_turns_chronological(&c.sid).await.unwrap().len(),
        2
    );
}
