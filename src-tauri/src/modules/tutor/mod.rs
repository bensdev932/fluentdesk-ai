//! Tutor pedagogy, voice-first prompt assembly, and correction formatting boundaries.

mod director_context;
mod director_rules;
pub mod feedback;
mod feedback_guard;
pub mod prompts;
pub mod roleplay_director;
pub mod stream_parser;
mod stream_parser_json;

#[cfg(test)]
mod adaptive_feedback_tests;
#[cfg(test)]
mod scaffolding_tests;
#[cfg(test)]
mod stream_parser_tests;
#[cfg(test)]
mod tests;
