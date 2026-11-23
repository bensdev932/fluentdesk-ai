//! Shared SSE token streaming adapters and error mapper for OpenAI-compat and Gemini providers.

use crate::ai::{
    catalog::ProviderId,
    cloud_http::{CloudFailure, CloudHttpError},
    cloud_http_retry::{retry_gemini, retry_openai_compat},
    cloud_http_stream::{post_sse_stream, SseLineStream},
    cloud_stream_extract::{
        extract_gemini_delta, extract_openai_delta, extract_sse_error, strip_sse_data,
    },
    cloud_stream_retry::{
        emit_cloud_final, try_reopen_sse, OpenAiSseRetry, SseOutTx, SsePumpCtx, SseStop,
    },
    cloud_ws_adapters::{is_ws_url, stream_gemini_ws, stream_openai_compat_ws},
    gemini::GeminiProvider,
    openai_compat::OpenAiCompatProvider,
    provider::TutorReplyStream,
};
use crate::domain::{
    ConversationTurn, CorrectionMode, PedagogicalScaffold, SessionError, Transcript,
};
use crate::modules::tutor::prompts::assemble_scaffolded_tutor_prompt;
use std::ops::ControlFlow;
use tokio::sync::mpsc;

pub(crate) async fn stream_openai_compat(
    provider: &OpenAiCompatProvider,
    history: &[ConversationTurn],
    user_transcript: &Transcript,
    mode: CorrectionMode,
    scaffold: Option<&PedagogicalScaffold>,
) -> Result<TutorReplyStream, (SessionError, CloudFailure)> {
    if is_ws_url(provider.endpoint_url()) {
        return stream_openai_compat_ws(provider, history, user_transcript, mode, scaffold).await;
    }
    let (pid, model) = (provider.provider_id(), provider.model_name());
    let prompt = assemble_scaffolded_tutor_prompt(history, user_transcript, mode, scaffold);
    let (sse, accepted) = retry_openai_compat(pid, provider.endpoint_url(), model, |cm| {
        post_sse_stream(provider.build_request(&prompt, cm, true))
    })
    .await?;
    let ctx = SsePumpCtx::new(pid, model).with_learner_text(&user_transcript.text);
    let retry = Some(OpenAiSseRetry::new(provider.clone(), prompt, accepted));
    Ok(spawn_sse_pump(sse, ctx, mode, extract_openai_delta, retry))
}

pub(crate) async fn stream_gemini(
    provider: &GeminiProvider,
    history: &[ConversationTurn],
    user_transcript: &Transcript,
    mode: CorrectionMode,
    scaffold: Option<&PedagogicalScaffold>,
) -> Result<TutorReplyStream, (SessionError, CloudFailure)> {
    if is_ws_url(provider.base_url()) {
        return stream_gemini_ws(provider, history, user_transcript, mode, scaffold).await;
    }
    let model = provider.model_name();
    let prompt = assemble_scaffolded_tutor_prompt(history, user_transcript, mode, scaffold);
    let sse = retry_gemini(provider.base_url(), model, |tm| {
        post_sse_stream(provider.build_stream_request(&prompt, tm))
    })
    .await?;
    let ctx = SsePumpCtx::new(ProviderId::Gemini, model).with_learner_text(&user_transcript.text);
    Ok(spawn_sse_pump(sse, ctx, mode, extract_gemini_delta, None))
}

fn spawn_sse_pump(
    sse_rx: SseLineStream,
    ctx: SsePumpCtx,
    mode: CorrectionMode,
    extract_delta: fn(&str) -> Option<String>,
    retry: Option<OpenAiSseRetry>,
) -> TutorReplyStream {
    let (tx, rx) = mpsc::channel(16);
    tokio::spawn(async move {
        pump_sse_lines(sse_rx, &tx, ctx, mode, extract_delta, retry).await;
    });
    rx
}

async fn pump_sse_lines(
    mut sse_rx: SseLineStream,
    tx: &SseOutTx,
    mut ctx: SsePumpCtx,
    mode: CorrectionMode,
    extract_delta: fn(&str) -> Option<String>,
    mut retry: Option<OpenAiSseRetry>,
) {
    loop {
        match run_sse_pass(&mut sse_rx, &mut ctx, extract_delta, tx).await {
            SseStop::Done => break,
            SseStop::Aborted => return,
            SseStop::PayloadErr(err) => match try_reopen_sse(retry.take(), err, &ctx, tx).await {
                Some(next_rx) => {
                    ctx.reset_for_retry();
                    sse_rx = next_rx;
                }
                None => return,
            },
        }
    }
    if flush_raw_fallback(&mut ctx, extract_delta, tx).await {
        emit_cloud_final(ctx, tx, mode).await;
    }
}

async fn run_sse_pass(
    sse_rx: &mut SseLineStream,
    ctx: &mut SsePumpCtx,
    extract_delta: fn(&str) -> Option<String>,
    tx: &SseOutTx,
) -> SseStop {
    while let Some(item) = sse_rx.recv().await {
        if let ControlFlow::Break(stop) = step_sse_item(item, ctx, extract_delta, tx).await {
            return stop;
        }
    }
    SseStop::Done
}

async fn flush_raw_fallback(
    ctx: &mut SsePumpCtx,
    extract_delta: fn(&str) -> Option<String>,
    tx: &SseOutTx,
) -> bool {
    if ctx.saw_sse {
        return true;
    }
    let raw = std::mem::take(&mut ctx.raw);
    if let Some(err) = extract_sse_error(raw.trim()) {
        ctx.abort_payload_err(tx, err.status, &err.message).await;
        return false;
    }
    ctx.emit_delta(raw.trim(), extract_delta, tx).await
}

async fn step_sse_item(
    item: Result<String, CloudHttpError>,
    ctx: &mut SsePumpCtx,
    extract_delta: fn(&str) -> Option<String>,
    tx: &SseOutTx,
) -> ControlFlow<SseStop> {
    let Ok(line) = item else {
        ctx.abort_err(tx, item.unwrap_err()).await;
        return ControlFlow::Break(SseStop::Aborted);
    };
    let Some(data) = strip_sse_data(&line) else {
        if !ctx.saw_sse {
            ctx.raw.extend([line.as_str(), "\n"]);
        }
        return ControlFlow::Continue(());
    };
    ctx.saw_sse = true;
    ctx.raw.clear();
    if data == "[DONE]" {
        return ControlFlow::Break(SseStop::Done);
    }
    if let Some(err) = extract_sse_error(data) {
        return ControlFlow::Break(SseStop::PayloadErr(err));
    }
    if ctx.emit_delta(data, extract_delta, tx).await {
        ControlFlow::Continue(())
    } else {
        ControlFlow::Break(SseStop::Aborted)
    }
}
