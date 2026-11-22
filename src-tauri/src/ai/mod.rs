//! AI LLM provider boundary and local/cloud adapters (`Ollama`, `Groq`, `Gemini`, `OpenRouter`).

pub mod catalog;
mod cloud_error;
mod cloud_http;
mod cloud_http_retry;
mod cloud_http_stream;
mod cloud_stream_adapters;
mod cloud_stream_extract;
mod cloud_stream_retry;
mod cloud_ws;
mod cloud_ws_adapters;
mod cloud_ws_policy;
pub mod gemini;
mod http_stream_io;
pub mod mock;
pub mod ollama;
mod ollama_config;
pub mod ollama_discovery;
pub mod ollama_http;
mod ollama_http_codec;
mod ollama_stream;
mod ollama_warmup;
pub mod openai_compat;
pub mod provider;
pub mod router;
mod router_exec;
mod router_health;
mod router_health_view;
mod router_provider;
mod router_stream;
mod router_stream_failover;
pub mod router_types;
pub mod secrets;
pub mod turn_request;
pub mod vault;

#[cfg(test)]
mod ai_resilience_tests;
#[cfg(test)]
mod cloud_stream_tests;
#[cfg(test)]
mod cloud_ws_edge_tests;
#[cfg(test)]
mod cloud_ws_tests;
#[cfg(test)]
mod discovery_tests;
#[cfg(test)]
mod ollama_resilience_tests;
#[cfg(test)]
mod resilience_review_tests;
#[cfg(test)]
mod router_auth_tests;
#[cfg(test)]
mod router_cycling_tests;
#[cfg(test)]
mod router_tests;
#[cfg(test)]
mod stream_edge_tests;
#[cfg(test)]
mod streaming_http_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod vault_tests;
