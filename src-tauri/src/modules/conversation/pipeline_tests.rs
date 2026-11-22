//! Integration tests for `SpeakingSessionHost` lifecycle and immediate cancellation
//! across real pipeline stages (`Listening`, `Processing`, `Thinking`, `Speaking`).

use crate::ai::{mock::MockAiProvider, provider::AiProvider};
use crate::database::{connection::init_sqlite_pool, repository::SessionRepository};
use crate::domain::{
    AudioChunk, AudioFormat, ConversationTurn, CorrectionMode, ProviderHealth, SessionError,
    SessionErrorKind, SessionState, Transcript, TutorReply,
};
use crate::modules::conversation::{
    host::SpeakingSessionHost, manager::SessionPipelineAdapters, StartSpeakingSessionConfig,
};
use crate::modules::speech::{
    audio_capture::{AudioInput, MockAudioInput},
    playback::{AudioOutput, MockAudioOutput},
    utterance::CompletedUtterance,
};
use crate::stt::{mock::MockSttProvider, provider::SpeechToText};
use crate::tts::mock::MockTtsProvider;
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::Notify;

struct BlockingAi(Arc<Notify>, Arc<Notify>);
#[async_trait]
impl AiProvider for BlockingAi {
    async fn health_check(&self) -> ProviderHealth {
        ProviderHealth::Available
    }
    async fn generate_tutor_reply(
        &self,
        _: &[ConversationTurn],
        _: &Transcript,
        _: CorrectionMode,
    ) -> Result<TutorReply, SessionError> {
        self.0.notify_one();
        self.1.notified().await;
        TutorReply::new_validated("Late reply.", None, None).map_err(|_| unreachable!())
    }
}

struct BlockingStt(Arc<Notify>, Arc<Notify>);
#[async_trait]
impl SpeechToText for BlockingStt {
    async fn health_check(&self) -> ProviderHealth {
        ProviderHealth::Available
    }
    async fn transcribe(&self, _: &CompletedUtterance) -> Result<Transcript, SessionError> {
        self.0.notify_one();
        self.1.notified().await;
        Ok(Transcript::new("Delayed transcript", "en", 500, Some(0.9)).unwrap())
    }
}

struct BlockingOutput(Arc<Notify>, Arc<Notify>);
#[async_trait]
impl AudioOutput for BlockingOutput {
    async fn play_audio(&self, _: &AudioChunk) -> Result<(), SessionError> {
        self.0.notify_one();
        self.1.notified().await;
        Ok(())
    }
    async fn stop_playback(&self) -> Result<(), SessionError> {
        self.1.notify_waiters();
        Ok(())
    }
    fn is_playing(&self) -> bool {
        false
    }
}

pub(super) async fn make_host(
    stt: Arc<dyn SpeechToText>,
    ai: Arc<dyn AiProvider>,
    out: Arc<dyn AudioOutput>,
) -> (
    Arc<SpeakingSessionHost>,
    Arc<MockAudioInput>,
    SessionRepository,
) {
    let repo = SessionRepository::new(init_sqlite_pool("sqlite::memory:").await.unwrap());
    let chunk = AudioChunk::new_bounded(AudioFormat::WHISPER_16KHZ_MONO_F32, vec![0.1; 160], 0, 10)
        .unwrap();
    let audio_in = Arc::new(MockAudioInput::new(vec![], vec![chunk]));
    let adapters = SessionPipelineAdapters {
        audio_input: audio_in.clone(),
        stt,
        ai,
        tts: Arc::new(MockTtsProvider::default()),
        audio_output: out,
    };
    let host = Arc::new(SpeakingSessionHost::new(adapters, Some(repo.clone())));
    (host, audio_in, repo)
}

#[tokio::test]
async fn stop_cancels_in_flight_thinking_processing_and_speaking_immediately() {
    let (e1, r1) = (Arc::new(Notify::new()), Arc::new(Notify::new()));
    let (h1, audio1, repo1) = make_host(
        Arc::new(MockSttProvider::default()),
        Arc::new(BlockingAi(e1.clone(), r1)),
        Arc::new(MockAudioOutput::new()),
    )
    .await;
    let sid1 = h1
        .start_session(StartSpeakingSessionConfig::default())
        .await
        .unwrap()
        .session_id;
    let (h1c, s1c) = (h1.clone(), sid1.clone());
    let tx = Transcript::new("Thinking out loud.", "en", 1000, Some(0.95)).unwrap();
    let t1 = tokio::spawn(async move { h1c.submit_text_turn(&s1c, &tx, None, 2_000).await });
    e1.notified().await;
    assert_eq!(
        h1.stop_session(&sid1).await.unwrap().state,
        SessionState::Ended
    );
    assert!(!audio1.is_capturing());
    assert_eq!(
        t1.await.unwrap().unwrap_err().kind,
        SessionErrorKind::Cancelled
    );
    assert_eq!(
        repo1.list_turns_chronological(&sid1).await.unwrap().len(),
        1
    );

    let (e2, r2) = (Arc::new(Notify::new()), Arc::new(Notify::new()));
    let (h2, _, _) = make_host(
        Arc::new(BlockingStt(e2.clone(), r2)),
        Arc::new(MockAiProvider::default()),
        Arc::new(MockAudioOutput::new()),
    )
    .await;
    let sid2 = h2
        .start_session(StartSpeakingSessionConfig::default())
        .await
        .unwrap()
        .session_id;
    let (h2c, s2c) = (h2.clone(), sid2.clone());
    let u2 = CompletedUtterance::try_new(vec![0.1; 8_000], 500, false).unwrap();
    let t2 = tokio::spawn(async move { h2c.process_spoken_utterance(&s2c, &u2, 3_000).await });
    e2.notified().await;
    assert_eq!(
        h2.stop_session(&sid2).await.unwrap().state,
        SessionState::Ended
    );
    assert_eq!(
        t2.await.unwrap().unwrap_err().kind,
        SessionErrorKind::Cancelled
    );

    let (e3, r3) = (Arc::new(Notify::new()), Arc::new(Notify::new()));
    let (h3, _, _) = make_host(
        Arc::new(MockSttProvider::default()),
        Arc::new(MockAiProvider::default()),
        Arc::new(BlockingOutput(e3.clone(), r3)),
    )
    .await;
    let sid3 = h3
        .start_session(StartSpeakingSessionConfig::default())
        .await
        .unwrap()
        .session_id;
    let (h3c, s3c) = (h3.clone(), sid3.clone());
    let u3 = CompletedUtterance::try_new(vec![0.1; 8_000], 500, false).unwrap();
    let t3 = tokio::spawn(async move { h3c.process_spoken_utterance(&s3c, &u3, 4_000).await });
    e3.notified().await;
    assert_eq!(
        h3.stop_session(&sid3).await.unwrap().state,
        SessionState::Ended
    );
    assert_eq!(
        t3.await.unwrap().unwrap_err().kind,
        SessionErrorKind::Cancelled
    );
}
