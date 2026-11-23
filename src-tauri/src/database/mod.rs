//! SQLite persistence boundary: connection management, versioned migrations, and repositories.

pub mod connection;
mod history;
pub mod mappers;
mod progress_repo;
pub mod repository;
mod review_repo;
mod schema_migrations;
pub(crate) mod turn_queries;

#[cfg(test)]
mod connection_tests;
#[cfg(test)]
mod preferences_tests;
#[cfg(test)]
mod structured_feedback_tests;
#[cfg(test)]
mod tests;
