//! Binary 16 kHz mono PCM frame decoding and bounded audio intake worker for WebSockets.

use crate::domain::{AudioChunk, AudioFormat, SampleRepresentation, SessionError, SessionId};
use crate::local_server::ws_protocol::WsServerEvent;
use crate::modules::conversation::SpeakingSessionHost;
use crate::modules::speech::{conversion::pcm_i16_to_f32, utterance::CompletedUtterance};
use serde::{Deserialize, Deserializer};
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};

pub(super) enum WsAudioMsg {
    Chunk(AudioChunk),
    PttHold(bool),
}

pub(super) struct WsUtteranceWork {
    pub utt: CompletedUtterance,
    pub started_tx: oneshot::Sender<()>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct WsAudioFormatPayload {
    #[serde(default = "default_sample_rate", alias = "sample_rate_hz")]
    pub sample_rate_hz: u32,
    #[serde(default = "default_channels")]
    pub channels: u8,
    #[serde(
        default = "default_sample_repr",
        alias = "sample_format",
        deserialize_with = "de_sample_repr"
    )]
    pub sample_format: SampleRepresentation,
}

impl WsAudioFormatPayload {
    pub(super) fn into_audio_format(self) -> Result<AudioFormat, String> {
        let fmt = AudioFormat {
            sample_rate_hz: self.sample_rate_hz,
            channels: self.channels,
            sample_format: self.sample_format,
        };
        fmt.validate().map_err(|e| e.to_string())?;
        Ok(fmt)
    }
}

pub(super) fn default_ws_audio_format() -> AudioFormat {
    AudioFormat {
        sample_rate_hz: 16_000,
        channels: 1,
        sample_format: SampleRepresentation::PcmI16,
    }
}

pub(super) fn decode_pcm_frame(
    bytes: &[u8],
    format: AudioFormat,
    ts_ms: u64,
) -> Result<Option<AudioChunk>, String> {
    if bytes.is_empty() {
        return Ok(None);
    }
    let samples = match format.sample_format {
        SampleRepresentation::PcmI16 => decode_i16_frame(bytes, format.channels)?,
        SampleRepresentation::PcmF32 => decode_f32_frame(bytes, format.channels)?,
    };
    let frames = (samples.len() as u64) / u64::from(format.channels.max(1));
    let ms = ((frames * 1000) / u64::from(format.sample_rate_hz)).max(1) as u32;
    AudioChunk::new_bounded(format, samples, ts_ms, ms)
        .map(Some)
        .map_err(|e| e.to_string())
}

pub(super) fn spawn_audio_worker(
    host: Arc<SpeakingSessionHost>,
    sid: SessionId,
    mut rx: mpsc::Receiver<WsAudioMsg>,
    utt_tx: mpsc::Sender<WsUtteranceWork>,
    ev_tx: mpsc::Sender<WsServerEvent>,
) {
    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if !process_audio_msg(&host, &sid, msg, &utt_tx, &ev_tx).await {
                break;
            }
        }
    });
}

async fn process_audio_msg(
    host: &SpeakingSessionHost,
    sid: &SessionId,
    msg: WsAudioMsg,
    utt_tx: &mpsc::Sender<WsUtteranceWork>,
    ev_tx: &mpsc::Sender<WsServerEvent>,
) -> bool {
    let res: Result<Option<CompletedUtterance>, SessionError> = match msg {
        WsAudioMsg::Chunk(chunk) => host.ingest_audio_chunk(sid, &chunk).await,
        WsAudioMsg::PttHold(held) => host.set_ptt_held(sid, held).await,
    };
    match res {
        Ok(Some(utt)) => dispatch_utterance_work(utt, utt_tx).await,
        Ok(None) => true,
        Err(err) => {
            let _ = ev_tx.send(WsServerEvent::from_error(sid, &err)).await;
            true
        }
    }
}

async fn dispatch_utterance_work(
    utt: CompletedUtterance,
    utt_tx: &mpsc::Sender<WsUtteranceWork>,
) -> bool {
    let (started_tx, started_rx) = oneshot::channel();
    if utt_tx
        .send(WsUtteranceWork { utt, started_tx })
        .await
        .is_err()
    {
        return false;
    }
    let _ = started_rx.await;
    true
}

fn default_sample_rate() -> u32 {
    16_000
}
fn default_channels() -> u8 {
    1
}
fn default_sample_repr() -> SampleRepresentation {
    SampleRepresentation::PcmI16
}

fn de_sample_repr<'de, D: Deserializer<'de>>(d: D) -> Result<SampleRepresentation, D::Error> {
    let raw = String::deserialize(d)?;
    match raw.trim().to_ascii_lowercase().as_str() {
        "pcmi16" | "pcm_i16" | "i16" => Ok(SampleRepresentation::PcmI16),
        "pcmf32" | "pcm_f32" | "f32" => Ok(SampleRepresentation::PcmF32),
        _ => Err(serde::de::Error::custom("unsupported sample format")),
    }
}

fn decode_i16_frame(bytes: &[u8], ch: u8) -> Result<Vec<f32>, String> {
    let stride = 2 * usize::from(ch.max(1));
    if !bytes.len().is_multiple_of(stride) {
        return Err("Unaligned 16-bit PCM audio frame.".into());
    }
    let samples: Vec<i16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&b| i16::from_le_bytes(b))
        .collect();
    Ok(pcm_i16_to_f32(&samples))
}

fn decode_f32_frame(bytes: &[u8], ch: u8) -> Result<Vec<f32>, String> {
    let stride = 4 * usize::from(ch.max(1));
    if !bytes.len().is_multiple_of(stride) {
        return Err("Unaligned 32-bit float PCM audio frame.".into());
    }
    let mut out = Vec::with_capacity(bytes.len() / 4);
    for &b in bytes.as_chunks::<4>().0 {
        let v = f32::from_le_bytes(b);
        if !v.is_finite() {
            return Err("Non-finite float sample in audio frame.".into());
        }
        out.push(v);
    }
    Ok(out)
}
