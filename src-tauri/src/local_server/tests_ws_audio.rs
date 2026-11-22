//! Loopback WebSocket integration tests for raw binary 16 kHz PCM audio frames,
//! `AudioFormat`, `PttHold`, `BargeIn` playback stop, and invalid frame errors.

use crate::ai::{mock::MockAiProvider, ollama::OllamaConfig, router::TutorRouter};
use crate::database::{connection::init_sqlite_pool, repository::SessionRepository};
use crate::domain::{AudioChunk, ProviderHealth, SessionError, Transcript};
use crate::local_server::tests_cycling::http_json;
use crate::local_server::tests_ws::{recv_ws_json, ws_connect_with_origin};
use crate::local_server::LocalBridgeServer;
use crate::modules::conversation::{manager::SessionPipelineAdapters, SpeakingSessionHost};
use crate::modules::speech::{audio_capture::MockAudioInput, playback::AudioOutput};
use crate::stt::mock::MockSttProvider;
use crate::tts::mock::MockTtsProvider;
use async_trait::async_trait;
use futures_util::SinkExt;
use serde_json::json;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tokio::sync::Notify;
use tokio_tungstenite::tungstenite::Message;

struct GatedPlayback {
    block: bool,
    started: Arc<Notify>,
    released: Arc<Notify>,
    stops: Arc<AtomicUsize>,
}

#[async_trait]
impl AudioOutput for GatedPlayback {
    async fn play_audio(&self, _: &AudioChunk) -> Result<(), SessionError> {
        self.started.notify_one();
        if self.block {
            self.released.notified().await;
        }
        Ok(())
    }
    async fn stop_playback(&self) -> Result<(), SessionError> {
        self.stops.fetch_add(1, Ordering::SeqCst);
        self.released.notify_waiters();
        Ok(())
    }
    fn is_playing(&self) -> bool {
        false
    }
}

fn pcm16_le_bytes(sample: i16, count: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(count * 2);
    for _ in 0..count {
        out.extend_from_slice(&sample.to_le_bytes());
    }
    out
}

async fn start_audio_bridge(out: Arc<dyn AudioOutput>, stt_texts: Vec<&str>) -> u16 {
    let pool = init_sqlite_pool("sqlite::memory:").await.unwrap();
    let repo = SessionRepository::new(pool.clone());
    let stt_res = stt_texts
        .into_iter()
        .map(|s| Ok(Transcript::new(s, "en", 600, Some(0.95)).unwrap()))
        .collect();
    let adapters = SessionPipelineAdapters {
        audio_input: Arc::new(MockAudioInput::default()),
        stt: Arc::new(MockSttProvider::new(ProviderHealth::Available, stt_res)),
        ai: Arc::new(MockAiProvider::default()),
        tts: Arc::new(MockTtsProvider::default()),
        audio_output: out,
    };
    let host = Arc::new(SpeakingSessionHost::new(adapters, Some(repo)));
    let ol = OllamaConfig::try_new("127.0.0.1", 1, "llama3").unwrap();
    let srv = LocalBridgeServer::bind_with_host(0, Arc::new(TutorRouter::new(ol)), host)
        .await
        .unwrap();
    let port = srv.local_addr().unwrap().port();
    tokio::spawn(async move {
        let _ = srv.serve().await;
    });
    port
}

#[tokio::test]
async fn ws_streams_binary_16khz_pcm_vad_turn_and_rejects_bad_audio_frames() {
    let out = Arc::new(GatedPlayback {
        block: false,
        started: Arc::new(Notify::new()),
        released: Arc::new(Notify::new()),
        stops: Arc::new(AtomicUsize::new(0)),
    });
    let port = start_audio_bridge(out, vec!["I practiced speaking today."]).await;
    let sid = http_json(port, "POST", "/api/sessions", "{}").await.2["sessionId"]
        .as_str()
        .unwrap()
        .to_string();
    let mut ws = ws_connect_with_origin(port, &sid, Some("http://127.0.0.1:3420"))
        .await
        .unwrap();
    let _ = recv_ws_json(&mut ws).await;

    ws.send(Message::Text(json!({"type": "AudioFormat", "sampleRateHz": 100, "channels": 1, "sampleFormat": "PcmI16"}).to_string().into())).await.unwrap();
    let bad_fmt = recv_ws_json(&mut ws).await;
    assert_eq!(
        (bad_fmt["type"].as_str(), bad_fmt["recoverable"].as_bool()),
        (Some("error"), Some(true))
    );

    ws.send(Message::Text(json!({"type": "AudioFormat", "sampleRateHz": 16000, "channels": 1, "sampleFormat": "PcmI16"}).to_string().into())).await.unwrap();
    ws.send(Message::Binary(vec![1u8, 2, 3].into()))
        .await
        .unwrap();
    assert_eq!(recv_ws_json(&mut ws).await["type"], "error");

    ws.send(Message::Binary(pcm16_le_bytes(8_000, 4_800).into()))
        .await
        .unwrap();
    ws.send(Message::Binary(pcm16_le_bytes(0, 17_600).into()))
        .await
        .unwrap();

    let mut saw_tx = false;
    let mut saw_done = false;
    while !saw_done {
        let ev = recv_ws_json(&mut ws).await;
        if ev["type"] == "user_transcript" {
            assert_eq!(ev["turn"]["text"], "I practiced speaking today.");
            saw_tx = true;
        } else if ev["type"] == "turn_complete" {
            saw_done = true;
        }
    }
    assert!(saw_tx);
}

#[tokio::test]
async fn ws_ptt_hold_and_barge_in_stop_active_audio_playback_immediately() {
    let started = Arc::new(Notify::new());
    let released = Arc::new(Notify::new());
    let stops = Arc::new(AtomicUsize::new(0));
    let out = Arc::new(GatedPlayback {
        block: true,
        started: started.clone(),
        released: released.clone(),
        stops: stops.clone(),
    });
    let port = start_audio_bridge(out, vec!["Hold to talk works."]).await;
    let sid = http_json(port, "POST", "/api/sessions", r#"{"pushToTalk":true}"#)
        .await
        .2["sessionId"]
        .as_str()
        .unwrap()
        .to_string();
    let mut ws = ws_connect_with_origin(port, &sid, Some("http://localhost:5173"))
        .await
        .unwrap();
    let _ = recv_ws_json(&mut ws).await;

    ws.send(Message::Text(
        json!({"type": "PttHold", "held": true}).to_string().into(),
    ))
    .await
    .unwrap();
    ws.send(Message::Binary(pcm16_le_bytes(6_000, 6_400).into()))
        .await
        .unwrap();
    ws.send(Message::Text(
        json!({"type": "PttHold", "held": false}).to_string().into(),
    ))
    .await
    .unwrap();

    started.notified().await;
    ws.send(Message::Text(
        json!({"type": "barge_in"}).to_string().into(),
    ))
    .await
    .unwrap();
    loop {
        let ev = recv_ws_json(&mut ws).await;
        if ev["type"] == "state_changed" && ev["state"] == "Listening" {
            break;
        }
    }
    assert!(stops.load(Ordering::SeqCst) >= 1);
}
