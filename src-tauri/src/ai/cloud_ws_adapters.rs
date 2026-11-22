//! WebSocket (`wss://` / loopback `ws://`) streaming adapters for OpenAI-compat and Gemini.

use crate::ai::{
    catalog::ProviderId,
    cloud_error::ThinkingMode,
    cloud_http::{CloudFailure, CloudHttpError},
    cloud_http_retry::{retry_gemini, retry_openai_compat},
    cloud_stream_extract::{
        extract_gemini_ws_delta, extract_openai_ws_delta, extract_sse_error, is_ws_turn_complete,
    },
    cloud_stream_retry::{emit_cloud_final, SseOutTx, SsePumpCtx},
    cloud_ws::{connect_ws_stream, CloudWsRequest, WsTextStream},
    gemini::{serialize_gemini_body, GeminiProvider},
    openai_compat::OpenAiCompatProvider,
    provider::{TutorReplyStream, TutorStreamItem},
};
use crate::domain::{
    ConversationTurn, CorrectionMode, PedagogicalScaffold, SessionError, SessionErrorKind,
    Transcript, TutorReply,
};
use crate::modules::tutor::prompts::{assemble_scaffolded_tutor_prompt, TutorPrompt};
use serde_json::{json, Value};
use tokio::sync::mpsc;

pub(crate) fn is_ws_url(url: &str) -> bool {
    let lower = url.trim_start();
    lower.len() >= 5
        && (lower[..5].eq_ignore_ascii_case("wss:/") || lower[..5].eq_ignore_ascii_case("ws://"))
}

pub(crate) async fn stream_openai_compat_ws(
    provider: &OpenAiCompatProvider,
    history: &[ConversationTurn],
    user_transcript: &Transcript,
    mode: CorrectionMode,
    scaffold: Option<&PedagogicalScaffold>,
) -> Result<TutorReplyStream, (SessionError, CloudFailure)> {
    let (pid, model) = (provider.provider_id(), provider.model_name());
    let prompt = assemble_scaffolded_tutor_prompt(history, user_transcript, mode, scaffold);
    let (ws, _) = retry_openai_compat(pid, provider.endpoint_url(), model, |cm| {
        connect_ws_stream(CloudWsRequest::from_http(
            provider.build_request(&prompt, cm, true),
        ))
    })
    .await?;
    Ok(spawn_ws_pump(
        ws,
        SsePumpCtx::new(pid, model),
        mode,
        extract_openai_ws_delta,
    ))
}

pub(crate) async fn stream_gemini_ws(
    provider: &GeminiProvider,
    history: &[ConversationTurn],
    user_transcript: &Transcript,
    mode: CorrectionMode,
    scaffold: Option<&PedagogicalScaffold>,
) -> Result<TutorReplyStream, (SessionError, CloudFailure)> {
    let model = provider.model_name();
    let prompt = assemble_scaffolded_tutor_prompt(history, user_transcript, mode, scaffold);
    let ws = retry_gemini(provider.base_url(), model, |tm| {
        connect_ws_stream(build_gemini_ws_request(provider, &prompt, tm))
    })
    .await?;
    Ok(spawn_ws_pump(
        ws,
        SsePumpCtx::new(ProviderId::Gemini, model),
        mode,
        extract_gemini_ws_delta,
    ))
}

pub(crate) async fn collect_ws_reply(
    stream_res: Result<TutorReplyStream, (SessionError, CloudFailure)>,
) -> Result<TutorReply, (SessionError, CloudFailure)> {
    let mut rx = stream_res?;
    while let Some(item) = rx.recv().await {
        match item {
            Ok(TutorStreamItem::Final(routed)) => return Ok(routed.reply),
            Ok(TutorStreamItem::Sentence(_)) => {}
            Err(err) => return Err((err, CloudFailure::None)),
        }
    }
    Err((empty_ws_err(), CloudFailure::None))
}

fn build_gemini_ws_request(
    provider: &GeminiProvider,
    prompt: &TutorPrompt,
    thinking: ThinkingMode,
) -> CloudWsRequest {
    let raw = serialize_gemini_body(prompt, thinking);
    let body = json!({
        "model": format!("models/{}", provider.model_name()),
        "request": serde_json::from_str::<Value>(&raw).unwrap_or(Value::Null),
    });
    CloudWsRequest {
        url: provider.base_url().to_string(),
        secret_headers: vec![("x-goog-api-key".into(), provider.api_key().clone())],
        extra_headers: Vec::new(),
        initial_messages: vec![body.to_string()],
        timeout_secs: 15,
    }
}

fn spawn_ws_pump(
    ws_rx: WsTextStream,
    ctx: SsePumpCtx,
    mode: CorrectionMode,
    extract_delta: fn(&str) -> Option<String>,
) -> TutorReplyStream {
    let (tx, rx) = mpsc::channel(16);
    tokio::spawn(async move { pump_ws_frames(ws_rx, &tx, ctx, mode, extract_delta).await });
    rx
}

async fn pump_ws_frames(
    mut ws_rx: WsTextStream,
    tx: &SseOutTx,
    mut ctx: SsePumpCtx,
    mode: CorrectionMode,
    extract_delta: fn(&str) -> Option<String>,
) {
    while let Some(item) = ws_rx.recv().await {
        match step_ws_frame(item, &mut ctx, extract_delta, tx).await {
            FrameStep::Continue => {}
            FrameStep::Done => break,
            FrameStep::Aborted => return,
        }
    }
    emit_cloud_final(ctx, tx, mode).await;
}

enum FrameStep {
    Continue,
    Done,
    Aborted,
}

async fn step_ws_frame(
    item: Result<String, CloudHttpError>,
    ctx: &mut SsePumpCtx,
    extract_delta: fn(&str) -> Option<String>,
    tx: &SseOutTx,
) -> FrameStep {
    let Ok(payload) = item else {
        ctx.abort_err(tx, item.unwrap_err()).await;
        return FrameStep::Aborted;
    };
    handle_ws_payload(payload.trim(), ctx, extract_delta, tx).await
}

async fn handle_ws_payload(
    trimmed: &str,
    ctx: &mut SsePumpCtx,
    extract_delta: fn(&str) -> Option<String>,
    tx: &SseOutTx,
) -> FrameStep {
    if trimmed == "[DONE]" {
        return FrameStep::Done;
    }
    if let Some(err) = extract_sse_error(trimmed) {
        ctx.abort_payload_err(tx, err.status, &err.message).await;
        return FrameStep::Aborted;
    }
    if !ctx.emit_delta(trimmed, extract_delta, tx).await {
        return FrameStep::Aborted;
    }
    if is_ws_turn_complete(trimmed) {
        FrameStep::Done
    } else {
        FrameStep::Continue
    }
}

fn empty_ws_err() -> SessionError {
    SessionError::new(
        SessionErrorKind::AiInferenceFailed,
        "WebSocket AI stream ended without a reply.",
        true,
        None,
    )
}
