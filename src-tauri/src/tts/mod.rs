//! Text-to-speech (TTS) provider boundary and adapter modules (`Kokoro`, `MockTts`).

pub mod kokoro;
mod kokoro_config;
pub mod mock;
pub mod phrase_cache;
mod phrase_lru;
pub mod provider;
mod sidecar;

#[cfg(test)]
mod kokoro_tests;
#[cfg(test)]
mod streaming_helpers;
#[cfg(test)]
mod streaming_tests;
#[cfg(test)]
mod tests;
