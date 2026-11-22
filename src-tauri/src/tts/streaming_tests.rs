//! Streaming sentence-by-sentence TTS playback, inter-sentence cancellation,
//! and mid-turn degradation tests through `TextToSpeech` and `synthesize_and_play_tutor_reply`.

use crate::domain::{SessionErrorKind, SessionId, TutorReply};
use crate::modules::conversation::cancellation::SessionCancellationToken;
use crate::modules::speech::synthesis::{
    synthesize_and_play_tutor_reply, PlaybackLifecycleStage, TutorPlaybackOutcome,
};
use crate::tts::{
    kokoro::LocalKokoroTtsAdapter,
    provider::{SpeakingSpeed, TextToSpeech},
    streaming_helpers::{test_kokoro_config, GatedRunner, SpyAudioOutput},
};
use std::sync::{mpsc as std_mpsc, Arc, Mutex};
use tokio::sync::{mpsc, Notify};

#[tokio::test]
async fn sentence_one_plays_immediately_before_sentence_two_finishes() {
    let (gate_tx, gate_rx) = std_mpsc::channel();
    let (started_tx, mut started_rx) = mpsc::unbounded_channel();
    let runner = Arc::new(GatedRunner {
        calls: Arc::new(Mutex::new(Vec::new())),
        gate_sentence_idx: Some(2),
        gate_rx: Mutex::new(Some(gate_rx)),
        fail_sentence_idx: None,
        started_tx: Some(started_tx),
    });
    let adapter = LocalKokoroTtsAdapter::new_with_runner(test_kokoro_config(), runner);
    let (play_tx, mut play_rx) = mpsc::unbounded_channel();
    let played = Arc::new(Mutex::new(Vec::new()));
    let output = SpyAudioOutput {
        played: played.clone(),
        stopped: Arc::new(Mutex::new(0)),
        on_play_tx: Some(play_tx),
        block_chunk_idx: None,
        block_notify: None,
        fail_chunk_idx: None,
    };
    let sid = SessionId::new("sess-stream-1").unwrap();
    let reply = TutorReply::new_validated("First sentence. Second sentence.", None, None).unwrap();
    let mut cancel = SessionCancellationToken::new();
    let handle = tokio::spawn(async move {
        let speed = SpeakingSpeed::try_new(1.0).unwrap();
        synthesize_and_play_tutor_reply(&adapter, &output, &sid, &reply, speed, &mut cancel).await
    });

    assert_eq!(started_rx.recv().await, Some(1));
    assert_eq!(play_rx.recv().await, Some(1));
    assert_eq!(played.lock().expect("lock").len(), 1);
    assert_eq!(started_rx.recv().await, Some(2));
    gate_tx.send(()).unwrap();
    assert_eq!(play_rx.recv().await, Some(2));

    let ev = handle.await.unwrap().expect("completed playback");
    assert_eq!(played.lock().expect("lock").len(), 2);
    assert_eq!(
        ev.stages,
        vec![
            PlaybackLifecycleStage::Synthesizing,
            PlaybackLifecycleStage::PlaybackStarted,
            PlaybackLifecycleStage::PlaybackCompleted,
        ]
    );
    assert_eq!(
        ev.outcome,
        TutorPlaybackOutcome::PlayedAudibly { duration_ms: 200 }
    );
}

#[tokio::test]
async fn cancelling_mid_playback_stops_output_and_aborts_remaining_sentences() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let runner = Arc::new(GatedRunner {
        calls: calls.clone(),
        gate_sentence_idx: None,
        gate_rx: Mutex::new(None),
        fail_sentence_idx: None,
        started_tx: None,
    });
    let adapter = LocalKokoroTtsAdapter::new_with_runner(test_kokoro_config(), runner);
    let (play_tx, mut play_rx) = mpsc::unbounded_channel();
    let stopped = Arc::new(Mutex::new(0));
    let output = SpyAudioOutput {
        played: Arc::new(Mutex::new(Vec::new())),
        stopped: stopped.clone(),
        on_play_tx: Some(play_tx),
        block_chunk_idx: Some(1),
        block_notify: Some(Arc::new(Notify::new())),
        fail_chunk_idx: None,
    };
    let sid = SessionId::new("sess-cancel-1").unwrap();
    let reply =
        TutorReply::new_validated("Sentence one. Sentence two. Sentence three.", None, None)
            .unwrap();
    let cancel = SessionCancellationToken::new();
    let mut task_cancel = cancel.clone();
    let handle = tokio::spawn(async move {
        let speed = SpeakingSpeed::try_new(1.0).unwrap();
        synthesize_and_play_tutor_reply(&adapter, &output, &sid, &reply, speed, &mut task_cancel)
            .await
    });

    assert_eq!(play_rx.recv().await, Some(1));
    cancel.cancel();
    assert_eq!(
        handle.await.unwrap().unwrap_err().kind,
        SessionErrorKind::Cancelled
    );
    assert!(*stopped.lock().expect("lock stopped") >= 1);
    tokio::task::yield_now().await;
    assert!(!calls
        .lock()
        .expect("lock")
        .iter()
        .any(|s| s == "Sentence three."));
}

#[tokio::test]
async fn dropping_synthesize_stream_stops_subsequent_sentences() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let (gate_tx, gate_rx) = std_mpsc::channel();
    let (started_tx, mut started_rx) = mpsc::unbounded_channel();
    let runner = Arc::new(GatedRunner {
        calls: calls.clone(),
        gate_sentence_idx: Some(1),
        gate_rx: Mutex::new(Some(gate_rx)),
        fail_sentence_idx: None,
        started_tx: Some(started_tx),
    });
    let adapter = LocalKokoroTtsAdapter::new_with_runner(test_kokoro_config(), runner);
    let rx = adapter
        .synthesize_stream("Sentence one. Sentence two.", 1.0)
        .await
        .expect("stream");
    assert_eq!(started_rx.recv().await, Some(1));
    drop(rx);
    gate_tx.send(()).unwrap();
    tokio::task::yield_now().await;
    assert_eq!(calls.lock().expect("lock").as_slice(), ["Sentence one."]);
}

#[tokio::test]
async fn mid_turn_synthesis_and_playback_failures_degrade_gracefully_to_text_only() {
    for (fail_synth, fail_play, expected_calls) in [(Some(2), None, 2), (None, Some(2), 3)] {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let runner = Arc::new(GatedRunner {
            calls: calls.clone(),
            gate_sentence_idx: None,
            gate_rx: Mutex::new(None),
            fail_sentence_idx: fail_synth,
            started_tx: None,
        });
        let adapter = LocalKokoroTtsAdapter::new_with_runner(test_kokoro_config(), runner);
        let stopped = Arc::new(Mutex::new(0));
        let played = Arc::new(Mutex::new(Vec::new()));
        let output = SpyAudioOutput {
            played: played.clone(),
            stopped: stopped.clone(),
            on_play_tx: None,
            block_chunk_idx: None,
            block_notify: None,
            fail_chunk_idx: fail_play,
        };
        let sid = SessionId::new("sess-degrade-1").unwrap();
        let reply =
            TutorReply::new_validated("First works. Second fails. Third skipped.", None, None)
                .unwrap();
        let mut cancel = SessionCancellationToken::new();
        let speed = SpeakingSpeed::try_new(1.0).unwrap();
        let ev =
            synthesize_and_play_tutor_reply(&adapter, &output, &sid, &reply, speed, &mut cancel)
                .await
                .expect("graceful degrade");

        assert!(matches!(
            ev.outcome,
            TutorPlaybackOutcome::TextOnlyDegraded { .. }
        ));
        assert_eq!(
            ev.stages,
            vec![
                PlaybackLifecycleStage::Synthesizing,
                PlaybackLifecycleStage::PlaybackStarted,
                PlaybackLifecycleStage::DegradedTextOnly,
            ]
        );
        assert!(calls.lock().expect("lock").len() <= expected_calls);
        assert!(*stopped.lock().expect("lock") >= 1);
    }
}
