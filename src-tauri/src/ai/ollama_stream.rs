//! Streaming NDJSON HTTP client for local `ollama serve` `/api/generate` (`voice-research-upgrades` #01).

use crate::ai::{
    catalog::ProviderId,
    http_stream_io::HttpLineReader,
    ollama::OllamaConfig,
    ollama_http::{
        connect_with_timeout, format_http_request, io_err, parse_err,
        serialize_generate_request_with_stream, timeout_err, OLLAMA_GENERATE_TIMEOUT_SECS,
    },
    ollama_http_codec::{bad_http_err, check_status_code},
    provider::{TutorReplyStream, TutorStreamItem},
    router_types::RoutedTutorReply,
};
use crate::domain::{CorrectionMode, SessionError};
use crate::modules::tutor::{
    feedback::parse_tutor_response_or_fallback, prompts::TutorPrompt,
    stream_parser::SpokenJsonStreamParser,
};
use serde::Deserialize;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;

#[derive(Deserialize)]
struct OllamaStreamChunk {
    #[serde(default)]
    response: String,
    #[serde(default)]
    done: bool,
}

/// Connects to Ollama with `stream: true`, validates HTTP status, and streams spoken sentences.
pub(crate) async fn stream_ollama_completion(
    cfg: &OllamaConfig,
    prompt: &TutorPrompt,
    mode: CorrectionMode,
) -> Result<TutorReplyStream, SessionError> {
    let payload = serialize_generate_request_with_stream(cfg, prompt, true)?;
    let addr = cfg.socket_addr();
    let raw_req = format_http_request(&addr, "POST", "/api/generate", &payload);
    let mut reader = open_ollama_stream(&addr, &raw_req).await?;
    let (tx, rx) = mpsc::channel(16);
    let model = cfg.model_name().to_string();
    tokio::spawn(async move {
        let _ = pump_ollama_lines(&mut reader, &tx, &model, mode).await;
    });
    Ok(rx)
}

async fn open_ollama_stream(addr: &str, raw_req: &str) -> Result<HttpLineReader, SessionError> {
    let mut sock = connect_with_timeout(addr).await?;
    sock.write_all(raw_req.as_bytes()).await.map_err(io_err)?;
    let (head, reader) = tokio::time::timeout(
        Duration::from_secs(OLLAMA_GENERATE_TIMEOUT_SECS),
        HttpLineReader::read_head(sock),
    )
    .await
    .map_err(|_| timeout_err())?
    .map_err(|_| bad_http_err())?;
    check_status_code(head.status)?;
    Ok(reader)
}

async fn pump_ollama_lines(
    reader: &mut HttpLineReader,
    tx: &mpsc::Sender<Result<TutorStreamItem, SessionError>>,
    model: &str,
    mode: CorrectionMode,
) -> Result<(), ()> {
    let mut parser = SpokenJsonStreamParser::new();
    while let Some(line) = read_next_line_or_err(reader, tx).await? {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if process_ndjson_line(trimmed, &mut parser, tx).await? {
            break;
        }
    }
    emit_ollama_final(parser, tx, model, mode).await
}

async fn read_next_line_or_err(
    reader: &mut HttpLineReader,
    tx: &mpsc::Sender<Result<TutorStreamItem, SessionError>>,
) -> Result<Option<String>, ()> {
    match reader.next_line().await {
        Ok(line) => Ok(line),
        Err(err) => {
            let _ = tx.send(Err(io_err(err))).await;
            Err(())
        }
    }
}

async fn process_ndjson_line(
    line: &str,
    parser: &mut SpokenJsonStreamParser,
    tx: &mpsc::Sender<Result<TutorStreamItem, SessionError>>,
) -> Result<bool, ()> {
    let chunk: OllamaStreamChunk = match serde_json::from_str(line) {
        Ok(c) => c,
        Err(err) => {
            let _ = tx.send(Err(parse_err(err.to_string()))).await;
            return Err(());
        }
    };
    for sentence in parser.push_token(&chunk.response) {
        if tx
            .send(Ok(TutorStreamItem::Sentence(sentence)))
            .await
            .is_err()
        {
            return Err(());
        }
    }
    Ok(chunk.done)
}

async fn emit_ollama_final(
    parser: SpokenJsonStreamParser,
    tx: &mpsc::Sender<Result<TutorStreamItem, SessionError>>,
    model: &str,
    mode: CorrectionMode,
) -> Result<(), ()> {
    let (tail, raw) = parser.finish();
    for sentence in tail {
        if tx
            .send(Ok(TutorStreamItem::Sentence(sentence)))
            .await
            .is_err()
        {
            return Err(());
        }
    }
    let item = parse_tutor_response_or_fallback(&raw, mode).map(|reply| {
        TutorStreamItem::Final(RoutedTutorReply {
            reply,
            provider: ProviderId::Ollama,
            model: model.to_string(),
            is_cloud: false,
            failover_note: None,
        })
    });
    let _ = tx.send(item).await;
    Ok(())
}
