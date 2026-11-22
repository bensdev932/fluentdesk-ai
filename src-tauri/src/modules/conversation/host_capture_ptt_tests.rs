//! Integration tests for `SpeakingSessionHost` live push-to-talk capture pumping
//! and immediate capture cancellation on pause and stop.

use crate::domain::{SessionErrorKind, SessionState};
use crate::modules::conversation::{
    host_capture_tests::{chunk_16k, make_live_host},
    StartSpeakingSessionConfig,
};
use crate::modules::speech::audio_capture::AudioInput;
use crate::stt::mock::MockSttProvider;
use std::sync::Arc;

#[tokio::test]
async fn host_pumps_live_push_to_talk_session_and_completes_on_release() {
    let stt = Arc::new(MockSttProvider::default());
    let (host, _audio_in, feeder, _repo) = make_live_host(vec![], stt.clone()).await;
    let cfg = StartSpeakingSessionConfig {
        push_to_talk: true,
        ..Default::default()
    };
    let snap = host.start_session(cfg).await.unwrap();
    let (h2, s2) = (host.clone(), snap.session_id.clone());
    let pump = tokio::spawn(async move { h2.pump_capture_turn(&s2, 3_000).await });
    tokio::task::yield_now().await;

    assert!(feeder.push(chunk_16k(0.2, 1_600, 0, 100)).await);
    tokio::task::yield_now().await;
    assert!(host
        .set_ptt_held(&snap.session_id, true)
        .await
        .unwrap()
        .is_none());
    assert!(feeder.push(chunk_16k(0.1, 8_000, 100, 500)).await);
    assert!(host
        .set_ptt_held(&snap.session_id, false)
        .await
        .unwrap()
        .is_none());

    let res = pump.await.unwrap().unwrap().expect("completed PTT turn");
    assert_eq!(res.final_state, SessionState::Listening);
    assert_eq!(stt.recorded_utterance_stats(), vec![(8_000, 500)]);
}

#[tokio::test]
async fn host_pause_and_stop_cancel_active_capture_pump_cleanly() {
    let (host, audio_in, feeder, _repo) =
        make_live_host(vec![], Arc::new(MockSttProvider::default())).await;
    let snap = host
        .start_session(StartSpeakingSessionConfig::default())
        .await
        .unwrap();

    let (h1, s1) = (host.clone(), snap.session_id.clone());
    let pump1 = tokio::spawn(async move { h1.pump_capture_turn(&s1, 1_000).await });
    tokio::task::yield_now().await;
    assert!(feeder.push(chunk_16k(0.2, 4_000, 0, 250)).await);
    let paused = host.pause_session(&snap.session_id).await.unwrap();
    assert_eq!(paused.state, SessionState::Paused);
    assert!(!audio_in.is_capturing());
    assert!(pump1.await.unwrap().unwrap().is_none());

    let resumed = host.resume_session(&snap.session_id).await.unwrap();
    assert_eq!(resumed.state, SessionState::Listening);
    assert!(audio_in.is_capturing());

    let (h2, s2) = (host.clone(), snap.session_id.clone());
    let pump2 = tokio::spawn(async move { h2.pump_capture_turn(&s2, 2_000).await });
    tokio::task::yield_now().await;
    let stopped = host.stop_session(&snap.session_id).await.unwrap();
    assert_eq!(stopped.state, SessionState::Ended);
    assert!(!audio_in.is_capturing());
    assert_eq!(
        pump2.await.unwrap().unwrap_err().kind,
        SessionErrorKind::Cancelled
    );
}
