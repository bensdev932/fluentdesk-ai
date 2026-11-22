//! Deterministic tests for `SpeakingSessionHost::barge_in_session` cancellation races.

use crate::ai::mock::MockAiProvider;
use crate::ai::provider::{AiProvider, TutorReplyStream};
use crate::domain::{
    AudioFormat, ConversationTurn, CorrectionMode, PedagogicalScaffold, ProviderHealth,
    SessionError, SessionErrorKind, SessionState, Transcript, TutorReply,
};
use crate::modules::conversation::{pipeline_tests::make_host, StartSpeakingSessionConfig};
use crate::modules::speech::{playback::MockAudioOutput, utterance::CompletedUtterance};
use crate::stt::provider::SpeechToText;
use async_trait::async_trait;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tokio::sync::Notify;

struct GatedStt {
    entered: Arc<Notify>,
    release: Arc<Notify>,
}

#[async_trait]
impl SpeechToText for GatedStt {
    async fn health_check(&self) -> ProviderHealth {
        ProviderHealth::Available
    }
    async fn transcribe(&self, _: &CompletedUtterance) -> Result<Transcript, SessionError> {
        self.entered.notify_one();
        self.release.notified().await;
        Ok(Transcript::new("Spoken before barge in.", "en", 500, Some(0.95)).unwrap())
    }
}

struct CountingAi {
    calls: Arc<AtomicUsize>,
    inner: MockAiProvider,
}

#[async_trait]
impl AiProvider for CountingAi {
    async fn health_check(&self) -> ProviderHealth {
        ProviderHealth::Available
    }
    async fn generate_tutor_reply(
        &self,
        h: &[ConversationTurn],
        t: &Transcript,
        m: CorrectionMode,
    ) -> Result<TutorReply, SessionError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.inner.generate_tutor_reply(h, t, m).await
    }
    async fn stream_scaffolded_reply(
        &self,
        h: &[ConversationTurn],
        t: &Transcript,
        m: CorrectionMode,
        s: Option<&PedagogicalScaffold>,
    ) -> Result<TutorReplyStream, SessionError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.inner.stream_scaffolded_reply(h, t, m, s).await
    }
}

#[tokio::test]
async fn barge_in_during_stt_stage_never_invokes_ai_or_persists_turns() {
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let ai_calls = Arc::new(AtomicUsize::new(0));
    let stt = Arc::new(GatedStt {
        entered: entered.clone(),
        release: release.clone(),
    });
    let ai = Arc::new(CountingAi {
        calls: ai_calls.clone(),
        inner: MockAiProvider::default(),
    });
    let (host, _, repo) = make_host(stt, ai, Arc::new(MockAudioOutput::new())).await;
    let snap = host
        .start_session(StartSpeakingSessionConfig::default())
        .await
        .unwrap();
    let utt = CompletedUtterance::try_new(vec![0.2; 1_600], 100, false).unwrap();
    let (h2, s2) = (host.clone(), snap.session_id.clone());
    let turn_task =
        tokio::spawn(async move { h2.process_spoken_utterance(&s2, &utt, 1_000).await });

    entered.notified().await;
    let barged = host.barge_in_session(&snap.session_id).await.unwrap();
    release.notify_one();
    assert_eq!(barged.state, SessionState::Listening);

    let err = turn_task.await.unwrap().unwrap_err();
    assert_eq!(err.kind, SessionErrorKind::Cancelled);
    assert_eq!(ai_calls.load(Ordering::SeqCst), 0);
    assert!(repo
        .list_turns_chronological(&snap.session_id)
        .await
        .unwrap()
        .is_empty());

    let tx = Transcript::new("Clean next turn.", "en", 0, None).unwrap();
    let ok = host
        .submit_text_turn(&snap.session_id, &tx, None, 2_000)
        .await
        .unwrap();
    assert_eq!(ok.state, SessionState::Listening);
    assert_eq!(AudioFormat::WHISPER_16KHZ_MONO_F32.channels, 1);
}

#[tokio::test]
async fn barge_in_with_progress_reconciles_tutor_turn_and_records_tts_ms() {
    let ai = Arc::new(MockAiProvider::new(
        ProviderHealth::Available,
        vec![Ok(TutorReply::new_validated(
            "First sentence! Second sentence? Third sentence.",
            None,
            Some("What else?".into()),
        )
        .unwrap())],
    ));
    let (host, _, repo) = make_host(
        Arc::new(crate::stt::mock::MockSttProvider::default()),
        ai,
        Arc::new(MockAudioOutput::new()),
    )
    .await;
    let snap = host
        .start_session(StartSpeakingSessionConfig::default())
        .await
        .unwrap();
    let tx = Transcript::new("Hello tutor.", "en", 0, None).unwrap();
    host.submit_text_turn(&snap.session_id, &tx, None, 1_000)
        .await
        .unwrap();

    host.barge_in_with_progress(
        &snap.session_id,
        Some(&["First sentence!".into()]),
        Some(415),
    )
    .await
    .unwrap();

    let turns = repo
        .list_turns_chronological(&snap.session_id)
        .await
        .unwrap();
    assert_eq!(turns.len(), 2);
    assert_eq!(turns[1].text, "First sentence!");
    assert_eq!(turns[1].follow_up_prompt, None);
    assert_eq!(
        turns[1]
            .stage_metadata
            .as_ref()
            .and_then(|m| m.tts_duration_ms),
        Some(415)
    );
}

#[tokio::test]
async fn completed_spoken_turn_records_measured_tts_duration_ms() {
    let (host, _, repo) = make_host(
        Arc::new(crate::stt::mock::MockSttProvider::default()),
        Arc::new(MockAiProvider::default()),
        Arc::new(MockAudioOutput::new()),
    )
    .await;
    let snap = host
        .start_session(StartSpeakingSessionConfig::default())
        .await
        .unwrap();
    let utt = CompletedUtterance::try_new(vec![0.2; 1_600], 100, false).unwrap();
    let res = host
        .process_spoken_utterance(&snap.session_id, &utt, 2_000)
        .await
        .unwrap();
    let recorded_tts = res
        .tutor_text_event
        .unwrap()
        .tutor_turn
        .stage_metadata
        .unwrap()
        .tts_duration_ms;
    assert!(recorded_tts.unwrap_or(0) > 0);
    let persisted = repo
        .list_turns_chronological(&snap.session_id)
        .await
        .unwrap();
    assert_eq!(
        persisted[1]
            .stage_metadata
            .as_ref()
            .and_then(|m| m.tts_duration_ms),
        recorded_tts
    );
}
