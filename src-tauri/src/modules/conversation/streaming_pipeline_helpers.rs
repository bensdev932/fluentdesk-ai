//! Shared gated test doubles for concurrent LLM-to-TTS streaming tests.

use crate::ai::{
    catalog::ProviderId,
    provider::{AiProvider, TutorReplyStream, TutorStreamItem},
    router_types::RoutedTutorReply,
};
use crate::database::repository::SessionRepository;
use crate::domain::{
    AudioChunk, ConversationTurn, CorrectionMode, ProviderHealth, SessionError, SessionErrorKind,
    SessionId, Transcript, TutorReply,
};
use crate::modules::conversation::{
    pipeline_tests::make_host, SpeakingSessionHost, StartSpeakingSessionConfig,
};
use crate::modules::speech::playback::AudioOutput;
use crate::stt::mock::MockSttProvider;
use async_trait::async_trait;
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc, Mutex,
};
use tokio::sync::{mpsc, watch, Notify};

pub(super) struct GatedStreamingAi {
    pub gate: watch::Receiver<bool>,
    pub llm_completed: Arc<AtomicBool>,
}

fn sample_reply() -> TutorReply {
    TutorReply::new_validated(
        "First sentence. Second sentence.",
        Some("Keep tense consistent.".into()),
        Some("Ready to try again?".into()),
    )
    .expect("valid reply")
}

#[async_trait]
impl AiProvider for GatedStreamingAi {
    async fn health_check(&self) -> ProviderHealth {
        ProviderHealth::Available
    }

    async fn generate_tutor_reply(
        &self,
        _: &[ConversationTurn],
        _: &Transcript,
        _: CorrectionMode,
    ) -> Result<TutorReply, SessionError> {
        let mut gate = self.gate.clone();
        let _ = gate.wait_for(|open| *open).await;
        Ok(sample_reply())
    }

    async fn stream_routed_reply(
        &self,
        _: &[ConversationTurn],
        _: &Transcript,
        _: CorrectionMode,
    ) -> Result<TutorReplyStream, SessionError> {
        let (tx, rx) = mpsc::channel(8);
        let (mut gate, completed) = (self.gate.clone(), self.llm_completed.clone());
        tokio::spawn(async move {
            let _ = tx
                .send(Ok(TutorStreamItem::Sentence("First sentence.".into())))
                .await;
            let _ = gate.wait_for(|open| *open).await;
            let _ = tx
                .send(Ok(TutorStreamItem::Sentence("Second sentence.".into())))
                .await;
            completed.store(true, Ordering::SeqCst);
            let _ = tx
                .send(Ok(TutorStreamItem::Final(RoutedTutorReply {
                    reply: sample_reply(),
                    provider: ProviderId::Ollama,
                    model: "stream-test".into(),
                    is_cloud: false,
                    failover_note: None,
                })))
                .await;
        });
        Ok(rx)
    }
}

pub(super) struct CountingAudioOutput {
    pub first_played: Arc<Notify>,
    pub played_durations: Arc<Mutex<Vec<u32>>>,
    pub stop_calls: Arc<AtomicUsize>,
    pub fail_on_play: bool,
}

#[async_trait]
impl AudioOutput for CountingAudioOutput {
    async fn play_audio(&self, chunk: &AudioChunk) -> Result<(), SessionError> {
        self.played_durations
            .lock()
            .expect("lock")
            .push(chunk.duration_ms);
        self.first_played.notify_waiters();
        if self.fail_on_play {
            return Err(SessionError::new(
                SessionErrorKind::PlaybackFailed,
                "Speaker busy.",
                true,
                None,
            ));
        }
        Ok(())
    }

    async fn stop_playback(&self) -> Result<(), SessionError> {
        self.stop_calls.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn is_playing(&self) -> bool {
        false
    }
}

pub(super) struct StreamingCase {
    pub host: Arc<SpeakingSessionHost>,
    pub sid: SessionId,
    pub gate_tx: watch::Sender<bool>,
    pub llm_done: Arc<AtomicBool>,
    pub first_played: Arc<Notify>,
    pub played: Arc<Mutex<Vec<u32>>>,
    pub stops: Arc<AtomicUsize>,
    pub repo: SessionRepository,
}

pub(super) async fn make_streaming_case(fail_on_play: bool) -> StreamingCase {
    let (gate_tx, gate_rx) = watch::channel(false);
    let first_played = Arc::new(Notify::new());
    let (llm_done, stops) = (
        Arc::new(AtomicBool::new(false)),
        Arc::new(AtomicUsize::new(0)),
    );
    let played = Arc::new(Mutex::new(Vec::new()));
    let ai = Arc::new(GatedStreamingAi {
        gate: gate_rx,
        llm_completed: llm_done.clone(),
    });
    let out = Arc::new(CountingAudioOutput {
        first_played: first_played.clone(),
        played_durations: played.clone(),
        stop_calls: stops.clone(),
        fail_on_play,
    });
    let (host, _, repo) = make_host(Arc::new(MockSttProvider::default()), ai, out).await;
    let sid = host
        .start_session(StartSpeakingSessionConfig::default())
        .await
        .unwrap()
        .session_id;
    StreamingCase {
        host,
        sid,
        gate_tx,
        llm_done,
        first_played,
        played,
        stops,
        repo,
    }
}
