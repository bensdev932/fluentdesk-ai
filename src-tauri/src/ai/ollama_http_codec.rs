//! HTTP/1.1 response status and chunked transfer-encoding decoder for Ollama replies.

use crate::domain::{SessionError, SessionErrorKind};

pub(crate) struct RawHttpResponse {
    pub status: u16,
    pub body: String,
}

pub(crate) fn parse_http_response(raw: &[u8]) -> Result<String, SessionError> {
    let parsed = parse_raw_http_response(raw)?;
    check_status_code(parsed.status)?;
    Ok(parsed.body)
}

pub(crate) fn parse_raw_http_response(raw: &[u8]) -> Result<RawHttpResponse, SessionError> {
    let split = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or_else(bad_http_err)?;
    let headers = std::str::from_utf8(&raw[..split]).map_err(|_| bad_http_err())?;
    let status = parse_status_code(headers)?;
    let body = decode_body(headers, &raw[split + 4..])?;
    Ok(RawHttpResponse { status, body })
}

pub(crate) fn check_status_code(status: u16) -> Result<(), SessionError> {
    if (200..=299).contains(&status) {
        return Ok(());
    }
    Err(SessionError::new(
        SessionErrorKind::AiInferenceFailed,
        "Local Ollama returned HTTP error.",
        true,
        Some(format!("HTTP {status}")),
    ))
}

fn decode_body(headers: &str, body: &[u8]) -> Result<String, SessionError> {
    let decoded = if is_chunked(headers) {
        decode_chunked_body(body)?
    } else {
        body.to_vec()
    };
    Ok(String::from_utf8_lossy(&decoded).trim().to_string())
}

pub(crate) fn parse_status_code(headers: &str) -> Result<u16, SessionError> {
    let line = headers.lines().next().ok_or_else(bad_http_err)?;
    let mut parts = line.split_whitespace();
    let proto = parts.next().ok_or_else(bad_http_err)?;
    let code = parts.next().ok_or_else(bad_http_err)?;
    if !proto.starts_with("HTTP/1.") {
        return Err(bad_http_err());
    }
    code.parse::<u16>().map_err(|_| bad_http_err())
}

pub(crate) fn is_chunked(headers: &str) -> bool {
    headers.lines().skip(1).any(|line| {
        line.split_once(':')
            .map(|(k, v)| {
                k.trim().eq_ignore_ascii_case("transfer-encoding")
                    && v.to_ascii_lowercase().contains("chunked")
            })
            .unwrap_or(false)
    })
}

fn decode_chunked_body(raw: &[u8]) -> Result<Vec<u8>, SessionError> {
    let (mut out, mut pos) = (Vec::new(), 0);
    while pos < raw.len() {
        let Some(size) = read_chunk_size(raw, &mut pos)? else {
            return Ok(out);
        };
        append_chunk_slice(raw, &mut pos, size, &mut out)?;
    }
    Err(bad_http_err())
}

fn read_chunk_size(raw: &[u8], pos: &mut usize) -> Result<Option<usize>, SessionError> {
    let rel = raw[*pos..]
        .windows(2)
        .position(|w| w == b"\r\n")
        .ok_or_else(bad_http_err)?;
    let line = std::str::from_utf8(&raw[*pos..*pos + rel]).map_err(|_| bad_http_err())?;
    let hex = line.split(';').next().unwrap_or("").trim();
    let size = usize::from_str_radix(hex, 16).map_err(|_| bad_http_err())?;
    *pos += rel + 2;
    Ok((size != 0).then_some(size))
}

fn append_chunk_slice(
    raw: &[u8],
    pos: &mut usize,
    size: usize,
    out: &mut Vec<u8>,
) -> Result<(), SessionError> {
    if *pos + size + 2 > raw.len() {
        return Err(bad_http_err());
    }
    out.extend_from_slice(&raw[*pos..*pos + size]);
    *pos += size + 2;
    Ok(())
}

pub(crate) fn bad_http_err() -> SessionError {
    SessionError::new(
        SessionErrorKind::AiInferenceFailed,
        "Malformed HTTP response from local Ollama service.",
        true,
        None,
    )
}
