//! Outbound `wss://` / loopback `ws://` streaming transport via `tokio-tungstenite` (`native-tls`).

use crate::ai::cloud_http::{err_msg, err_status, net_err, CloudHttpError};
pub(crate) use crate::ai::cloud_ws_policy::{build_ws_handshake_request, CloudWsRequest};
use futures_util::{
    stream::{SplitSink, SplitStream},
    SinkExt, StreamExt,
};
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio_tungstenite::{
    connect_async_tls_with_config,
    tungstenite::{http::Request, Error as WsError, Message},
    Connector, MaybeTlsStream, WebSocketStream,
};

pub(crate) type WsTextStream = mpsc::Receiver<Result<String, CloudHttpError>>;
type WsSocket = WebSocketStream<MaybeTlsStream<TcpStream>>;
type WsStep = Result<Option<Result<Message, WsError>>, tokio::time::error::Elapsed>;

pub(crate) async fn connect_ws_stream(req: CloudWsRequest) -> Result<WsTextStream, CloudHttpError> {
    let timeout = Duration::from_secs(req.timeout_secs.max(5));
    let (http_req, is_tls) = build_ws_handshake_request(&req)?;
    let ws = dial_ws(http_req, build_connector(is_tls)?, timeout).await?;
    let (mut sink, stream) = ws.split();
    send_initial_messages(&mut sink, req.initial_messages).await?;
    let (tx, rx) = mpsc::channel(32);
    tokio::spawn(pump_ws_messages(sink, stream, tx, timeout));
    Ok(rx)
}

fn build_connector(is_tls: bool) -> Result<Connector, CloudHttpError> {
    if !is_tls {
        return Ok(Connector::Plain);
    }
    let tls = native_tls::TlsConnector::new()
        .map_err(|e| err_msg(None, format!("TLS init failed: {e}"), false))?;
    Ok(Connector::NativeTls(tls))
}

async fn dial_ws(
    http_req: Request<()>,
    connector: Connector,
    timeout: Duration,
) -> Result<WsSocket, CloudHttpError> {
    let fut = connect_async_tls_with_config(http_req, None, false, Some(connector));
    let res = tokio::time::timeout(timeout, fut)
        .await
        .map_err(|_| net_err("WebSocket handshake timed out."))?;
    res.map(|(ws, _)| ws).map_err(map_ws_err)
}

async fn send_initial_messages(
    sink: &mut SplitSink<WsSocket, Message>,
    messages: Vec<String>,
) -> Result<(), CloudHttpError> {
    for body in messages {
        sink.send(Message::Text(body.into()))
            .await
            .map_err(map_ws_err)?;
    }
    Ok(())
}

async fn pump_ws_messages(
    mut sink: SplitSink<WsSocket, Message>,
    mut stream: SplitStream<WsSocket>,
    tx: mpsc::Sender<Result<String, CloudHttpError>>,
    timeout: Duration,
) {
    loop {
        tokio::select! {
            biased;
            _ = tx.closed() => { let _ = sink.close().await; break; }
            step = tokio::time::timeout(timeout, stream.next()) => {
                if !handle_ws_step(step, &tx).await { break; }
            }
        }
    }
}

async fn handle_ws_step(step: WsStep, tx: &mpsc::Sender<Result<String, CloudHttpError>>) -> bool {
    match step {
        Ok(Some(Ok(Message::Text(txt)))) => tx.send(Ok(txt.to_string())).await.is_ok(),
        Ok(Some(Ok(Message::Binary(bin)))) => send_utf8_binary(&bin, tx).await,
        Ok(Some(Ok(Message::Close(_)))) | Ok(None) => false,
        Ok(Some(Ok(_))) => true,
        Ok(Some(Err(err))) => emit_ws_step_err(tx, map_ws_err(err)).await,
        Err(_) => emit_ws_step_err(tx, net_err("WebSocket stream timed out.")).await,
    }
}

async fn emit_ws_step_err(
    tx: &mpsc::Sender<Result<String, CloudHttpError>>,
    err: CloudHttpError,
) -> bool {
    let _ = tx.send(Err(err)).await;
    false
}

async fn send_utf8_binary(bin: &[u8], tx: &mpsc::Sender<Result<String, CloudHttpError>>) -> bool {
    match std::str::from_utf8(bin) {
        Ok(txt) => tx.send(Ok(txt.to_string())).await.is_ok(),
        Err(_) => true,
    }
}

fn map_ws_err(err: WsError) -> CloudHttpError {
    match err {
        WsError::Http(resp) => map_http_ws_err(&resp),
        other => net_err(format!("WebSocket error: {other}")),
    }
}

fn map_http_ws_err(
    resp: &tokio_tungstenite::tungstenite::http::Response<Option<Vec<u8>>>,
) -> CloudHttpError {
    let status = resp.status().as_u16();
    let body = resp
        .body()
        .as_deref()
        .map(String::from_utf8_lossy)
        .unwrap_or_default();
    err_status(status, &body)
}
