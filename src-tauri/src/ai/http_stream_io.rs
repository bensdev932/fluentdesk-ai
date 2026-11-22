//! Shared incremental HTTP/1.1 header and chunked/plain body line reader over `TcpStream`.

use crate::ai::ollama_http_codec::{is_chunked, parse_status_code};
use std::io;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio::net::TcpStream;

/// Parsed HTTP response head before streaming body lines.
pub(crate) struct HttpHead {
    pub status: u16,
}

/// Incremental HTTP/1.1 body line reader supporting both raw and chunked transfer encoding.
pub(crate) struct HttpLineReader {
    reader: BufReader<TcpStream>,
    chunked: bool,
    pending: String,
    eof: bool,
}

impl HttpLineReader {
    pub(crate) async fn read_head(stream: TcpStream) -> Result<(HttpHead, Self), io::Error> {
        let mut reader = BufReader::new(stream);
        let mut raw_headers = String::new();
        loop {
            let mut line = String::new();
            let n = reader.read_line(&mut line).await?;
            if n == 0 || line == "\r\n" || line == "\n" {
                break;
            }
            raw_headers.push_str(&line);
        }
        let status = parse_status_code(&raw_headers)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.user_message))?;
        let chunked = is_chunked(&raw_headers);
        Ok((HttpHead { status }, Self::new(reader, chunked)))
    }

    fn new(reader: BufReader<TcpStream>, chunked: bool) -> Self {
        Self {
            reader,
            chunked,
            pending: String::new(),
            eof: false,
        }
    }

    pub(crate) async fn next_line(&mut self) -> Result<Option<String>, io::Error> {
        if self.chunked {
            self.next_chunked_line().await
        } else {
            self.next_raw_line().await
        }
    }

    async fn next_raw_line(&mut self) -> Result<Option<String>, io::Error> {
        let mut line = String::new();
        if self.reader.read_line(&mut line).await? == 0 {
            return Ok(None);
        }
        Ok(Some(line))
    }

    async fn next_chunked_line(&mut self) -> Result<Option<String>, io::Error> {
        loop {
            if let Some(line) = take_buffered_line(&mut self.pending, self.eof) {
                return Ok(Some(line));
            }
            if self.eof {
                return Ok(None);
            }
            self.pull_next_chunk().await?;
        }
    }

    async fn pull_next_chunk(&mut self) -> Result<(), io::Error> {
        let mut size_line = String::new();
        if self.reader.read_line(&mut size_line).await? == 0 {
            self.eof = true;
            return Ok(());
        }
        let size = parse_hex_chunk_size(&size_line)?;
        if size == 0 {
            self.eof = true;
            return Ok(());
        }
        self.append_chunk_bytes(size).await
    }

    async fn append_chunk_bytes(&mut self, size: usize) -> Result<(), io::Error> {
        let mut bytes = vec![0u8; size];
        self.reader.read_exact(&mut bytes).await?;
        let mut crlf = [0u8; 2];
        let _ = self.reader.read_exact(&mut crlf).await;
        self.pending.push_str(&String::from_utf8_lossy(&bytes));
        Ok(())
    }
}

fn take_buffered_line(buf: &mut String, eof: bool) -> Option<String> {
    if let Some(pos) = buf.find('\n') {
        let line: String = buf.drain(..=pos).collect();
        return Some(line);
    }
    (eof && !buf.is_empty()).then(|| std::mem::take(buf))
}

fn parse_hex_chunk_size(line: &str) -> Result<usize, io::Error> {
    let hex = line.split(';').next().unwrap_or("").trim();
    usize::from_str_radix(hex, 16)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Invalid HTTP chunk header."))
}
