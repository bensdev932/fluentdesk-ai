//! FluentDesk AI — Core voice-first English speaking tutor library.
//!
//! Architecture boundaries:
//! - `domain`: Shared typed domain models (`SessionState`, `AudioChunk`, `Transcript`, `TutorReply`, `LearnerPreferences`)
//! - `telemetry`: Structured tracing and strict redaction of secrets and raw audio
//! - `preferences`: Unified SQLite-backed learner and routing preferences store
//! - `modules`: Core business logic (`conversation`, `speech`, `tutor`, `learning`)
//! - `ai`: LLM provider trait and adapters (`Ollama`, `llama.cpp`, `MockAiProvider`, `TutorRouter`)
//! - `stt`: Speech-to-text trait and adapters (`Whisper`, `MockStt`)
//! - `tts`: Text-to-speech trait and adapters (`Kokoro`, `MockTts`)
//! - `database`: SQLite connection pool, embedded schema migrations, and repositories

pub mod ai;
pub mod database;
pub mod domain;
pub mod local_server;
pub mod modules;
pub mod preferences;
pub mod stt;
pub mod telemetry;
pub mod tts;
