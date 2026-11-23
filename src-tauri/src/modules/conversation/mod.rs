//! Conversation orchestration, bounded context window, and turn processing.

pub mod cancellation;
mod capture_pump;
pub mod context;
pub mod host;
mod host_capture;
mod host_capture_state;
mod host_prefs;
mod host_spoken;
mod host_spoken_events;
mod host_spoken_playback;
mod host_spoken_stream;
mod host_turn_guards;
mod host_turn_stream;
mod host_turns;
pub mod manager;
mod persistence;
mod persistence_helpers;
pub mod review_engine;
mod review_host;
mod scaffold_lifecycle;
pub mod turn;
pub mod types;

pub use host::SpeakingSessionHost;
pub(crate) use host_spoken::SpokenTurnAck;
pub use host_spoken_events::SpokenTurnEvent;
pub use types::{SessionStatusSnapshot, StartSpeakingSessionConfig, TextTurnOutcome};

#[cfg(test)]
mod e2e_tests;
#[cfg(test)]
mod host_barge_in_tests;
#[cfg(test)]
mod host_capture_ptt_tests;
#[cfg(test)]
mod host_capture_tests;
#[cfg(test)]
mod host_tests;
#[cfg(test)]
mod pipeline_tests;
#[cfg(test)]
mod review_tests;
#[cfg(test)]
mod scaffolding_session_tests;
#[cfg(test)]
mod streaming_pipeline_helpers;
#[cfg(test)]
mod streaming_pipeline_tests;
#[cfg(test)]
mod tests;
