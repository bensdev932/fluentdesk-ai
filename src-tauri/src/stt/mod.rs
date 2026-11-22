//! Speech-to-text (STT) provider boundary and adapter modules (`Whisper`, `MockStt`).

pub mod mock;
pub mod prompt_context;
pub mod provider;
mod sidecar;
pub mod whisper;

#[cfg(test)]
mod prompt_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod whisper_tests;
