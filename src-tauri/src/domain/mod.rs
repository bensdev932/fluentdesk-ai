//! Shared typed domain models for FluentDesk AI (`T003`).
//!
//! No provider-specific SDK types may leak into this module.

pub mod audio;
pub mod conversation;
pub mod error;
pub mod pedagogy;
pub mod preferences;
pub mod providers;
pub mod review;
pub mod scenario;
pub mod scenario_openers;
mod scenario_presets;
mod scenario_presets_extra;
pub mod session;
pub mod voice_prefs;

#[cfg(test)]
mod pedagogy_tests;
#[cfg(test)]
mod preferences_tests;
#[cfg(test)]
mod tests;

pub use audio::*;
pub use conversation::*;
pub use error::*;
pub use pedagogy::*;
pub use preferences::*;
pub use providers::*;
pub use review::*;
pub use scenario::*;
pub use scenario_openers::*;
pub use session::*;
pub use voice_prefs::*;
