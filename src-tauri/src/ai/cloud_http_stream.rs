//! Pooled `reqwest` outbound SSE streaming transport for cloud AI adapters.

use crate::ai::cloud_http::{
    build_reqwest, err_status, map_reqwest_err, CloudHttpError, CloudHttpRequest,
};
use reqwest::Response;
use std::ops::ControlFlow;
use tokio::sync::mpsc;

pub(crate) type SseLineStream = mpsc::Receiver<Result<String, CloudHttpError>>;

/// Opens a streaming SSE connection, validates 2xx status headers first, and streams body lines.
pub(crate) async fn post_sse_stream(
    req: CloudHttpRequest,
) -> Result<SseLineStream, CloudHttpError> {
    let (client, request) = build_reqwest(req, 15)?;
    let resp = client.execute(request).await.map_err(map_reqwest_err)?;
    open_sse_receiver(resp).await
}

async fn open_sse_receiver(resp: Response) -> Result<SseLineStream, CloudHttpError> {
    let status = resp.status().as_u16();
    if !(200..=299).contains(&status) {
        let body = read_error_snippet(resp).await;
        return Err(err_status(status, &body));
    }
    let (line_tx, line_rx) = mpsc::channel(32);
    tokio::spawn(async move { pump_sse_chunks(resp, line_tx).await });
    Ok(line_rx)
}

async fn read_error_snippet(mut resp: Response) -> String {
    let mut buf = Vec::new();
    while buf.len() < 1024 {
        match resp.chunk().await {
            Ok(Some(chunk)) => buf.extend_from_slice(&chunk),
            _ => break,
        }
    }
    String::from_utf8_lossy(&buf[..buf.len().min(1024)]).into_owned()
}

async fn pump_sse_chunks(
    mut resp: Response,
    line_tx: mpsc::Sender<Result<String, CloudHttpError>>,
) {
    let mut buf = Vec::new();
    loop {
        match resp.chunk().await {
            Ok(Some(c)) if step_chunk(&c, &mut buf, &line_tx).await => {}
            Ok(Some(_)) => return,
            Ok(None) => return flush_eof_tail(&buf, &line_tx).await,
            Err(e) => {
                let _ = line_tx.send(Err(map_reqwest_err(e))).await;
                return;
            }
        }
    }
}

async fn step_chunk(
    chunk: &[u8],
    buf: &mut Vec<u8>,
    tx: &mpsc::Sender<Result<String, CloudHttpError>>,
) -> bool {
    buf.extend_from_slice(chunk);
    drain_complete_lines(buf, tx).await.is_continue()
}

async fn drain_complete_lines(
    buf: &mut Vec<u8>,
    tx: &mpsc::Sender<Result<String, CloudHttpError>>,
) -> ControlFlow<()> {
    while let Some(pos) = buf.iter().position(|&b| b == b'\n') {
        let raw: Vec<u8> = buf.drain(..=pos).collect();
        let line = String::from_utf8_lossy(&raw).into_owned();
        if tx.send(Ok(line)).await.is_err() {
            return ControlFlow::Break(());
        }
    }
    ControlFlow::Continue(())
}

async fn flush_eof_tail(buf: &[u8], tx: &mpsc::Sender<Result<String, CloudHttpError>>) {
    if !buf.is_empty() {
        let line = String::from_utf8_lossy(buf).into_owned();
        let _ = tx.send(Ok(line)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn sse_stream_preserves_split_utf8_across_chunks_and_aborts_on_drop() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let (done_tx, done_rx) = tokio::sync::oneshot::channel::<()>();
        tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 2048];
            let _ = sock.read(&mut buf).await;
            let head = b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n\r\n";
            sock.write_all(head).await.unwrap();
            let msg = "data: Café 😊\n";
            let cut = msg.len() - 3;
            sock.write_all(&msg.as_bytes()[..cut]).await.unwrap();
            sock.flush().await.unwrap();
            tokio::task::yield_now().await;
            sock.write_all(&msg.as_bytes()[cut..]).await.unwrap();
            sock.write_all(b"data: keepalive\n").await.unwrap();
            sock.flush().await.unwrap();
            let mut probe = [0u8; 8];
            let _ = tokio::time::timeout(std::time::Duration::from_secs(2), sock.read(&mut probe))
                .await;
            let _ = done_tx.send(());
        });
        let mut rx = post_sse_stream(CloudHttpRequest {
            url: format!("http://127.0.0.1:{port}/sse"),
            secret_headers: vec![],
            extra_headers: vec![],
            body: "{}".into(),
            timeout_secs: 5,
        })
        .await
        .unwrap();
        assert_eq!(rx.recv().await.unwrap().unwrap(), "data: Café 😊\n");
        drop(rx);
        done_rx.await.unwrap();
    }

    #[tokio::test]
    async fn gemini_sse_skips_thought_parts_and_uses_minimal_for_3_5() {
        use crate::ai::{
            gemini::GeminiProvider,
            provider::{AiProvider, TutorStreamItem},
            router_tests::{stub, tx},
        };
        let c1 = "data: {\"candidates\":[{\"content\":{\"parts\":[{\"thought\":true,\"text\":\"plan\"}]}}]}\n\n";
        let c2 = "data: {\"candidates\":[{\"content\":{\"parts\":[{\"thought\":true,\"text\":\"x\"},{\"text\":\"{\\\"spokenReply\\\":\\\"Hello!\\\"}\"}]}}]}\n\n";
        let sse = format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{c1}{c2}", c1.len() + c2.len());
        let (port, seen) = stub(vec![sse]).await;
        let g = GeminiProvider::new(
            format!("http://127.0.0.1:{port}"),
            "gemini-3.5-flash",
            crate::telemetry::RedactedSecret::new("AIzaTest"),
        );
        let mut s = g
            .stream_routed_reply(&[], &tx("Hi"), crate::domain::CorrectionMode::Off)
            .await
            .unwrap();
        while let Some(item) = s.recv().await {
            if let TutorStreamItem::Final(r) = item.unwrap() {
                assert_eq!(r.reply.spoken_reply, "Hello!");
            }
        }
        assert!(seen.lock().unwrap()[0].contains("\"thinkingLevel\":\"MINIMAL\""));
    }
}
