//! Spoken turn execution and context management on `SessionManager` (`T034`–`T035`, `T048`–`T051`).

use crate::ai::provider::{
    open_turn_stream_with_cancellation, recv_tutor_stream_item, TutorStreamItem, TutorTurnRequest,
};
use crate::domain::{
    ConversationTurn, CorrectionMode, PedagogicalScaffold, SessionError, SessionErrorKind,
    SessionId, SessionState, Transcript, TutorReply,
};
use crate::modules::conversation::{
    cancellation::SessionCancellationToken, manager::SessionManager,
};
use crate::modules::speech::{
    synthesis::{synthesize_and_play_tutor_reply, TutorPlaybackEvent, TutorPlaybackOutcome},
    transcription::{transcribe_completed_utterance_with_context, UserTranscriptReadyEvent},
    utterance::CompletedUtterance,
};
use crate::tts::provider::SpeakingSpeed;
use serde::{Deserialize, Serialize};
use std::time::Instant;

/// Event emitted when the AI tutor response text is validated and persisted (`T034`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TutorTextReadyEvent {
    pub session_id: SessionId,
    pub user_turn: ConversationTurn,
    pub tutor_turn: ConversationTurn,
    pub reply: TutorReply,
}

/// Complete outcome of a single spoken turn across STT, LLM, and TTS (`T050`).
#[derive(Debug, Clone)]
pub struct EndToEndTurnResult {
    pub transcript_event: UserTranscriptReadyEvent,
    pub tutor_text_event: Option<TutorTextReadyEvent>,
    pub playback_event: Option<TutorPlaybackEvent>,
    pub final_state: SessionState,
}

impl SessionManager {
    pub fn context_turns(&self) -> Vec<ConversationTurn> {
        self.context.recent_turns()
    }

    pub async fn process_spoken_utterance(
        &mut self,
        utterance: &CompletedUtterance,
        now_unix_ms: i64,
    ) -> Result<EndToEndTurnResult, SessionError> {
        let tx_event = self.transcribe_utterance(utterance).await?;
        if tx_event.is_empty_or_hallucinated {
            return Ok(EndToEndTurnResult {
                transcript_event: tx_event,
                tutor_text_event: None,
                playback_event: None,
                final_state: self.state,
            });
        }
        let tutor_ev = self
            .process_user_transcript(&tx_event.transcript, now_unix_ms)
            .await?;
        let pb_ev = self
            .speak_tutor_reply(&tutor_ev, self.speaking_speed)
            .await?;
        if let TutorPlaybackOutcome::PlayedAudibly { duration_ms } = pb_ev.outcome {
            self.record_tutor_tts_duration(duration_ms).await?;
        }
        Ok(EndToEndTurnResult {
            transcript_event: tx_event,
            tutor_text_event: Some(tutor_ev),
            playback_event: Some(pb_ev),
            final_state: self.state,
        })
    }

    pub async fn transcribe_utterance(
        &mut self,
        utt: &CompletedUtterance,
    ) -> Result<UserTranscriptReadyEvent, SessionError> {
        self.transition_to(SessionState::Processing)?;
        let ctx = self.stt_prompt_context();
        let res = transcribe_completed_utterance_with_context(
            self.adapters.stt.as_ref(),
            utt,
            Some(&ctx),
        )
        .await;
        let ev = self.fail_on_err(res)?;
        let next = if ev.is_empty_or_hallucinated {
            SessionState::Listening
        } else {
            SessionState::Thinking
        };
        self.transition_to(next)?;
        Ok(ev)
    }

    pub async fn process_user_transcript(
        &mut self,
        tx: &Transcript,
        now_ms: i64,
    ) -> Result<TutorTextReadyEvent, SessionError> {
        let (history, mode) = (self.context.recent_turns(), self.correction_mode);
        let u_turn = self
            .record_user_turn(&tx.text, now_ms, tx.duration_ms, mode)
            .await?;
        let (ai, mut cancel) = (self.adapters.ai.clone(), self.cancellation.clone());
        let llm_start = Instant::now();
        let res =
            collect_scaffolded_reply(ai.as_ref(), &history, tx, mode, &self.scaffold, &mut cancel)
                .await;
        let reply = self.fail_on_err(res)?;
        let llm_ms = u32::try_from(llm_start.elapsed().as_millis()).unwrap_or(u32::MAX);
        let t_turn = self
            .record_tutor_turn(&reply, now_ms + 1, llm_ms, mode)
            .await?;
        Ok(TutorTextReadyEvent {
            session_id: self.session_id.clone(),
            user_turn: u_turn,
            tutor_turn: t_turn,
            reply,
        })
    }

    pub async fn speak_tutor_reply(
        &mut self,
        ev: &TutorTextReadyEvent,
        speed: SpeakingSpeed,
    ) -> Result<TutorPlaybackEvent, SessionError> {
        self.transition_to(SessionState::Speaking)?;
        let (tts, out, mut tok) = (
            self.adapters.tts.clone(),
            self.adapters.audio_output.clone(),
            self.cancellation.clone(),
        );
        let res = synthesize_and_play_tutor_reply(
            tts.as_ref(),
            out.as_ref(),
            &ev.session_id,
            &ev.reply,
            speed,
            &mut tok,
        )
        .await;
        let pb = self.fail_on_err(res)?;
        if self.state == SessionState::Speaking && !self.is_cancelled() {
            self.transition_to(SessionState::Listening)?;
        }
        Ok(pb)
    }

    fn fail_on_err<T>(&mut self, res: Result<T, SessionError>) -> Result<T, SessionError> {
        if res.is_err() {
            let _ = self.transition_to(SessionState::Error);
        }
        res
    }
}

async fn collect_scaffolded_reply(
    ai: &dyn crate::ai::provider::AiProvider,
    history: &[ConversationTurn],
    tx: &Transcript,
    mode: CorrectionMode,
    scaffold: &PedagogicalScaffold,
    cancel: &mut SessionCancellationToken,
) -> Result<TutorReply, SessionError> {
    let req = TutorTurnRequest::new(history, tx, mode, Some(scaffold));
    let mut stream = open_turn_stream_with_cancellation(ai, &req, cancel).await?;
    while let Some(item) = recv_tutor_stream_item(&mut stream, cancel).await? {
        if let TutorStreamItem::Final(routed) = item {
            return Ok(routed.reply);
        }
    }
    Err(SessionError::new(
        SessionErrorKind::AiInferenceFailed,
        "Tutor stream ended without final reply.",
        true,
        None,
    ))
}
