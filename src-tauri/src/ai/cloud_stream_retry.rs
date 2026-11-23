//! SSE pump context, `json_validate_failed` retry helper, and sentence emission helpers.

use crate::ai::catalog::ProviderId;
use crate::ai::cloud_error::{map_cloud_err, store_compat_mode, CompatMode};
use crate::ai::cloud_http::{err_msg, CloudHttpError};
use crate::ai::cloud_http_stream::{post_sse_stream, SseLineStream};
use crate::ai::cloud_stream_extract::SsePayloadError;
use crate::ai::openai_compat::OpenAiCompatProvider;
use crate::ai::provider::TutorStreamItem;
use crate::ai::router_types::RoutedTutorReply;
use crate::domain::{CorrectionMode, SessionError};
use crate::modules::tutor::{
    feedback::parse_tutor_response_for_utterance, prompts::TutorPrompt,
    stream_parser::SpokenJsonStreamParser,
};
use tokio::sync::mpsc;

pub(crate) type SseOutTx = mpsc::Sender<Result<TutorStreamItem, SessionError>>;

pub(crate) enum SseStop {
    Done,
    Aborted,
    PayloadErr(SsePayloadError),
}

pub(crate) struct SsePumpCtx {
    pub parser: SpokenJsonStreamParser,
    pub saw_sse: bool,
    pub raw: String,
    pub sent_sentence: bool,
    provider: ProviderId,
    model: String,
    learner_text: Option<String>,
}

impl SsePumpCtx {
    pub(crate) fn new(provider: ProviderId, model: impl Into<String>) -> Self {
        Self {
            parser: SpokenJsonStreamParser::new(),
            saw_sse: false,
            raw: String::new(),
            sent_sentence: false,
            provider,
            model: model.into(),
            learner_text: None,
        }
    }

    pub(crate) fn with_learner_text(mut self, text: impl Into<String>) -> Self {
        self.learner_text = Some(text.into());
        self
    }

    pub(crate) async fn abort_err(&self, tx: &SseOutTx, err: CloudHttpError) {
        let _ = tx.send(Err(map_cloud_err(self.provider, err).0)).await;
    }

    pub(crate) async fn abort_payload_err(&self, tx: &SseOutTx, status: Option<u16>, msg: &str) {
        self.abort_err(tx, err_msg(status, msg, false)).await;
    }

    pub(crate) async fn emit_delta(
        &mut self,
        payload: &str,
        extract_delta: fn(&str) -> Option<String>,
        tx: &SseOutTx,
    ) -> bool {
        let Some(delta) = extract_delta(payload) else {
            return true;
        };
        let sentences = self.parser.push_token(&delta);
        send_sentences(&mut self.sent_sentence, sentences, tx).await
    }

    pub(crate) fn reset_for_retry(&mut self) {
        self.parser = SpokenJsonStreamParser::new();
        self.saw_sse = false;
        self.raw.clear();
    }
}

pub(crate) struct OpenAiSseRetry {
    provider: OpenAiCompatProvider,
    prompt: TutorPrompt,
    mode: CompatMode,
}

impl OpenAiSseRetry {
    pub(crate) fn new(
        provider: OpenAiCompatProvider,
        prompt: TutorPrompt,
        mode: CompatMode,
    ) -> Self {
        Self {
            provider,
            prompt,
            mode,
        }
    }

    pub(crate) async fn reopen_without_json(
        self,
        code: Option<&str>,
        msg: &str,
        sent_sentence: bool,
    ) -> Result<SseLineStream, Option<CloudHttpError>> {
        let next = self
            .next_no_json_mode(code, msg, sent_sentence)
            .ok_or(None)?;
        let (pid, url, model) = (
            self.provider.provider_id(),
            self.provider.endpoint_url(),
            self.provider.model_name(),
        );
        store_compat_mode(pid, url, model, next);
        post_sse_stream(self.provider.build_request(&self.prompt, next, true))
            .await
            .map_err(Some)
    }

    fn next_no_json_mode(
        &self,
        code: Option<&str>,
        msg: &str,
        sent_sentence: bool,
    ) -> Option<CompatMode> {
        let is_jvf = code == Some("json_validate_failed") || msg.contains("json_validate_failed");
        if !is_jvf || sent_sentence {
            return None;
        }
        self.mode
            .without_json(self.provider.provider_id(), self.provider.model_name())
    }
}

pub(crate) async fn try_reopen_sse(
    retry: Option<OpenAiSseRetry>,
    err: SsePayloadError,
    ctx: &SsePumpCtx,
    tx: &SseOutTx,
) -> Option<SseLineStream> {
    let Some(r) = retry else {
        ctx.abort_payload_err(tx, err.status, &err.message).await;
        return None;
    };
    match r
        .reopen_without_json(err.code.as_deref(), &err.message, ctx.sent_sentence)
        .await
    {
        Ok(rx) => Some(rx),
        Err(Some(http_err)) => {
            ctx.abort_err(tx, http_err).await;
            None
        }
        Err(None) => {
            ctx.abort_payload_err(tx, err.status, &err.message).await;
            None
        }
    }
}

pub(crate) async fn emit_cloud_final(mut ctx: SsePumpCtx, tx: &SseOutTx, mode: CorrectionMode) {
    let (tail, raw) = ctx.parser.finish();
    if send_sentences(&mut ctx.sent_sentence, tail, tx).await {
        let reply = parse_tutor_response_for_utterance(&raw, mode, ctx.learner_text.as_deref());
        let item = reply.map(|r| {
            TutorStreamItem::Final(RoutedTutorReply {
                reply: r,
                provider: ctx.provider,
                model: ctx.model,
                is_cloud: ctx.provider.is_cloud(),
                failover_note: None,
            })
        });
        let _ = tx.send(item).await;
    }
}

async fn send_sentences(sent_sentence: &mut bool, sentences: Vec<String>, tx: &SseOutTx) -> bool {
    for s in sentences {
        *sent_sentence = true;
        if tx.send(Ok(TutorStreamItem::Sentence(s))).await.is_err() {
            return false;
        }
    }
    true
}
