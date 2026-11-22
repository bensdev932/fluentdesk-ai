//! Typed WebSocket wire protocol: inbound client JSON control frames.

use crate::domain::CorrectionMode;
use crate::local_server::ws_audio::WsAudioFormatPayload;
pub(super) use crate::local_server::ws_events::WsServerEvent;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(super) enum WsClientCommand {
    #[serde(alias = "StartTurn")]
    StartTurn {
        #[serde(default, alias = "userText")]
        user_text: Option<String>,
        #[serde(default, alias = "correctionMode")]
        correction_mode: Option<CorrectionMode>,
    },
    #[serde(alias = "AudioFormat")]
    AudioFormat(WsAudioFormatPayload),
    #[serde(alias = "PttHold")]
    PttHold { held: bool },
    #[serde(alias = "BargeIn")]
    BargeIn {
        #[serde(default, alias = "heardSentences")]
        heard_sentences: Option<Vec<String>>,
        #[serde(default, alias = "playedMs")]
        played_ms: Option<u32>,
    },
    #[serde(alias = "Stop")]
    Stop,
}
