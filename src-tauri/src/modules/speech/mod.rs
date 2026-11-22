//! Speech pipeline boundaries: audio capture, conversion, unified speech intake,
//! STT orchestration, TTS orchestration, and audio output playback.

pub mod audio_capture;
mod audio_capture_mock;
pub mod conversion;
pub mod intake;
mod intake_config;
mod intake_processing;
pub mod playback;
pub mod spoken_text;
pub mod synthesis;
pub mod synthesis_stream;
pub mod transcription;
pub mod utterance;

#[cfg(test)]
mod conversion_tests;
#[cfg(test)]
mod intake_cap_tests;
#[cfg(test)]
mod intake_tests;
