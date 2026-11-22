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
    pending: Vec<u8>,
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
            pending: Vec::new(),
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
        let mut bytes = Vec::new();
        if self.reader.read_until(b'\n', &mut bytes).await? == 0 {
            return Ok(None);
        }
        Ok(Some(decode_utf8_lossy_owned(bytes)))
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
        let mut trail = Vec::with_capacity(2);
        let _ = self.reader.read_until(b'\n', &mut trail).await;
        self.pending.extend_from_slice(&bytes);
        Ok(())
    }
}

fn take_buffered_line(buf: &mut Vec<u8>, eof: bool) -> Option<String> {
    if let Some(pos) = buf.iter().position(|&b| b == b'\n') {
        let line: Vec<u8> = buf.drain(..=pos).collect();
        return Some(decode_utf8_lossy_owned(line));
    }
    (eof && !buf.is_empty()).then(|| decode_utf8_lossy_owned(std::mem::take(buf)))
}

fn decode_utf8_lossy_owned(bytes: Vec<u8>) -> String {
    String::from_utf8(bytes)
        .unwrap_or_else(|err| String::from_utf8_lossy(err.as_bytes()).into_owned())
}

fn parse_hex_chunk_size(line: &str) -> Result<usize, io::Error> {
    let hex = line.split(';').next().unwrap_or("").trim();
    usize::from_str_radix(hex, 16)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Invalid HTTP chunk header."))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncWriteExt;
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn chunked_reader_handles_lf_terminators_and_eof_split_utf8_without_trailing_newline() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let payload = "{\"response\":\"It\u{2019}s \u{2014} \u{1f60a}\",\"done\":true}";
        let raw = payload.as_bytes().to_vec();
        let cut = payload.find('\u{1f60a}').unwrap() + 2;

        tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            sock.write_all(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n")
                .await
                .unwrap();
            for slice in [&raw[..cut], &raw[cut..]] {
                let head = format!("{:x}\n", slice.len());
                sock.write_all(head.as_bytes()).await.unwrap();
                sock.write_all(slice).await.unwrap();
                sock.write_all(b"\n").await.unwrap();
            }
            sock.write_all(b"0\n\n").await.unwrap();
            let _ = sock.shutdown().await;
        });

        let stream = TcpStream::connect(addr).await.unwrap();
        let (head, mut reader) = HttpLineReader::read_head(stream).await.unwrap();
        assert_eq!(head.status, 200);
        let line = reader.next_line().await.unwrap().expect("expected line");
        assert_eq!(line, payload);
        assert_eq!(reader.next_line().await.unwrap(), None);
    }
}
