//! Streaming text-turn execution helper (`submit_text_turn_stream`) on `SpeakingSessionHost`.

use crate::ai::provider::{
    open_turn_stream_with_cancellation, recv_tutor_stream_item, TutorReplyStream, TutorStreamItem,
    TutorTurnRequest,
};
use crate::ai::router_types::RoutedTutorReply;
use crate::domain::{CorrectionMode, SessionError, SessionErrorKind, SessionId, Transcript};
use crate::modules::conversation::{
    cancellation::SessionCancellationToken,
    host::SpeakingSessionHost,
    host_turns::{elapsed_ms, PreparedThinking},
    types::TextTurnOutcome,
};
use std::time::Instant;
use tokio::sync::mpsc;

impl SpeakingSessionHost {
    /// Executes a text turn while streaming spoken sentences into `sentence_tx` as they form.
    pub async fn submit_text_turn_stream(
        &self,
        session_id: &SessionId,
        transcript: &Transcript,
        correction_mode: Option<CorrectionMode>,
        now_unix_ms: i64,
        sentence_tx: mpsc::Sender<String>,
    ) -> Result<TextTurnOutcome, SessionError> {
        let mut prep = self
            .prepare_thinking_turn(
                session_id,
                transcript,
                correction_mode,
                now_unix_ms,
                true,
                None,
            )
            .await?;
        let llm_started = Instant::now();
        let stream_res = open_turn_stream(&mut prep, transcript).await;
        let res = drain_turn_stream(stream_res, sentence_tx, &mut prep.cancel).await;
        self.finish_text_turn(
            session_id,
            prep.user_turn,
            prep.mode,
            now_unix_ms,
            elapsed_ms(llm_started),
            &prep.cancel,
            res,
        )
        .await
    }
}

async fn open_turn_stream(
    prep: &mut PreparedThinking,
    transcript: &Transcript,
) -> Result<TutorReplyStream, SessionError> {
    let req = TutorTurnRequest::new(&prep.history, transcript, prep.mode, Some(&prep.scaffold));
    open_turn_stream_with_cancellation(prep.ai.as_ref(), &req, &mut prep.cancel).await
}

async fn drain_turn_stream(
    stream_res: Result<TutorReplyStream, SessionError>,
    sentence_tx: mpsc::Sender<String>,
    cancel: &mut SessionCancellationToken,
) -> Result<RoutedTutorReply, SessionError> {
    let stream = stream_res?;
    drain_text_turn_stream(stream, sentence_tx, cancel).await
}

async fn drain_text_turn_stream(
    mut stream: TutorReplyStream,
    sentence_tx: mpsc::Sender<String>,
    cancel: &mut SessionCancellationToken,
) -> Result<RoutedTutorReply, SessionError> {
    let mut emitted_any = false;
    while let Some(item) = recv_tutor_stream_item(&mut stream, cancel).await? {
        if let Some(routed) = step_stream_item(item, &sentence_tx, &mut emitted_any).await {
            return Ok(routed);
        }
    }
    Err(closed_without_final_err())
}

async fn step_stream_item(
    item: TutorStreamItem,
    sentence_tx: &mpsc::Sender<String>,
    emitted_any: &mut bool,
) -> Option<RoutedTutorReply> {
    match item {
        TutorStreamItem::Sentence(s) => {
            *emitted_any = true;
            let _ = sentence_tx.send(s).await;
            None
        }
        TutorStreamItem::Final(routed) => {
            if !*emitted_any {
                let _ = sentence_tx.send(routed.reply.spoken_reply.clone()).await;
            }
            Some(routed)
        }
    }
}

fn closed_without_final_err() -> SessionError {
    SessionError::new(
        SessionErrorKind::AiInferenceFailed,
        "Tutor stream closed without a final reply.",
        true,
        None,
    )
}
