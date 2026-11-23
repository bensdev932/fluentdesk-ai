//! Streaming NDJSON HTTP client for local `ollama serve` `/api/generate` (`voice-research-upgrades` #01).

use crate::ai::{
    catalog::ProviderId,
    http_stream_io::HttpLineReader,
    ollama::OllamaConfig,
    ollama_http::{
        connect_with_timeout, format_http_request, io_err, ollama_service_err, parse_err,
        serialize_generate_request_with_stream, timeout_err, OLLAMA_GENERATE_TIMEOUT_SECS,
    },
    ollama_http_codec::{
        bad_http_err, check_status_with_body, OllamaStreamChunk,
        StreamTimeouts as HttpStreamTimeouts,
    },
    provider::{TutorReplyStream, TutorStreamItem},
    router_types::RoutedTutorReply,
};
use crate::domain::{CorrectionMode, SessionError};
use crate::modules::tutor::{
    feedback::parse_tutor_response_for_utterance, prompts::TutorPrompt,
    stream_parser::SpokenJsonStreamParser,
};
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;

pub(crate) type StreamTimeouts = HttpStreamTimeouts;

const OLLAMA_STREAM_IDLE_TIMEOUT_SECS: u64 = 25;
type StreamTx = mpsc::Sender<Result<TutorStreamItem, SessionError>>;

struct StreamTarget {
    model: String,
    mode: CorrectionMode,
    learner_text: Option<String>,
}

/// Connects to Ollama with `stream: true`, validates HTTP status, and streams spoken sentences.
pub(crate) async fn stream_ollama_completion(
    cfg: &OllamaConfig,
    prompt: &TutorPrompt,
    mode: CorrectionMode,
) -> Result<TutorReplyStream, SessionError> {
    let timeouts = StreamTimeouts {
        first: Duration::from_secs(OLLAMA_GENERATE_TIMEOUT_SECS),
        idle: Duration::from_secs(OLLAMA_STREAM_IDLE_TIMEOUT_SECS),
    };
    stream_ollama_completion_with_timeouts(cfg, prompt, mode, timeouts).await
}

pub(crate) async fn stream_ollama_completion_with_timeouts(
    cfg: &OllamaConfig,
    prompt: &TutorPrompt,
    mode: CorrectionMode,
    timeouts: StreamTimeouts,
) -> Result<TutorReplyStream, SessionError> {
    let payload = serialize_generate_request_with_stream(cfg, prompt, true)?;
    let addr = cfg.socket_addr();
    let raw_req = format_http_request(&addr, "POST", "/api/generate", &payload);
    let mut reader = open_ollama_stream(&addr, &raw_req, timeouts.first).await?;
    let (tx, rx) = mpsc::channel(16);
    let target = StreamTarget {
        model: cfg.model_name().to_string(),
        mode,
        learner_text: prompt.latest_user_text().map(str::to_string),
    };
    tokio::spawn(async move {
        let _ = pump_ollama_lines(&mut reader, &tx, &target, timeouts).await;
    });
    Ok(rx)
}

async fn open_ollama_stream(
    addr: &str,
    raw_req: &str,
    head_timeout: Duration,
) -> Result<HttpLineReader, SessionError> {
    let mut sock = connect_with_timeout(addr).await?;
    sock.write_all(raw_req.as_bytes()).await.map_err(io_err)?;
    let (head, mut reader) = tokio::time::timeout(head_timeout, HttpLineReader::read_head(sock))
        .await
        .map_err(|_| timeout_err())?
        .map_err(|_| bad_http_err())?;
    if !(200..=299).contains(&head.status) {
        let snippet = tokio::time::timeout(Duration::from_secs(1), reader.next_line())
            .await
            .ok()
            .and_then(|r| r.ok().flatten())
            .unwrap_or_default();
        check_status_with_body(head.status, &snippet)?;
    }
    Ok(reader)
}

async fn pump_ollama_lines(
    reader: &mut HttpLineReader,
    tx: &StreamTx,
    target: &StreamTarget,
    timeouts: StreamTimeouts,
) -> Result<(), ()> {
    let (mut parser, mut saw_chunk) = (SpokenJsonStreamParser::new(), false);
    loop {
        let wait = if saw_chunk {
            timeouts.idle
        } else {
            timeouts.first
        };
        let Some(line) = read_next_line_or_err(reader, tx, wait).await? else {
            break;
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        saw_chunk = true;
        if process_ndjson_line(trimmed, &mut parser, tx).await? {
            break;
        }
    }
    emit_ollama_final(parser, tx, target).await
}

async fn read_next_line_or_err(
    reader: &mut HttpLineReader,
    tx: &StreamTx,
    wait: Duration,
) -> Result<Option<String>, ()> {
    match tokio::time::timeout(wait, reader.next_line()).await {
        Ok(Ok(line)) => Ok(line),
        Ok(Err(err)) => send_err(tx, io_err(err)).await,
        Err(_) => send_err(tx, timeout_err()).await,
    }
}

async fn process_ndjson_line(
    line: &str,
    parser: &mut SpokenJsonStreamParser,
    tx: &StreamTx,
) -> Result<bool, ()> {
    let chunk: OllamaStreamChunk = match serde_json::from_str(line) {
        Ok(c) => c,
        Err(err) => return send_err(tx, parse_err(err.to_string())).await,
    };
    if let Some(err) = chunk
        .error
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return send_err(tx, ollama_service_err(err)).await;
    }
    send_sentences(parser.push_token(&chunk.response), tx).await?;
    Ok(chunk.done)
}

async fn emit_ollama_final(
    parser: SpokenJsonStreamParser,
    tx: &StreamTx,
    target: &StreamTarget,
) -> Result<(), ()> {
    let (tail, raw) = parser.finish();
    send_sentences(tail, tx).await?;
    let item =
        parse_tutor_response_for_utterance(&raw, target.mode, target.learner_text.as_deref()).map(
            |reply| {
                TutorStreamItem::Final(RoutedTutorReply {
                    reply,
                    provider: ProviderId::Ollama,
                    model: target.model.clone(),
                    is_cloud: false,
                    failover_note: None,
                })
            },
        );
    let _ = tx.send(item).await;
    Ok(())
}

async fn send_sentences(sentences: Vec<String>, tx: &StreamTx) -> Result<(), ()> {
    for sentence in sentences {
        if tx
            .send(Ok(TutorStreamItem::Sentence(sentence)))
            .await
            .is_err()
        {
            return Err(());
        }
    }
    Ok(())
}

async fn send_err<T>(tx: &StreamTx, err: SessionError) -> Result<T, ()> {
    let _ = tx.send(Err(err)).await;
    Err(())
}
