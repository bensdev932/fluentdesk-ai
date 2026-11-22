//! Localhost-only HTTP bridge (`127.0.0.1:3420`) exposing session, preferences & multi-provider endpoints.

mod bridge;
mod dto;
mod handlers;
mod http;
mod review_handlers;
mod router;
mod session_dto;
mod session_handlers;
mod session_stream;
mod ws_audio;
mod ws_commands;
mod ws_events;
mod ws_handler;
mod ws_origin;
mod ws_protocol;
mod ws_turns;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_bridge_bind;
#[cfg(test)]
mod tests_cycling;
#[cfg(test)]
mod tests_failover;
#[cfg(test)]
mod tests_preferences;
#[cfg(test)]
mod tests_preferences_restart;
#[cfg(test)]
mod tests_review;
#[cfg(test)]
mod tests_sessions;
#[cfg(test)]
mod tests_sessions_persistence;
#[cfg(test)]
mod tests_stream;
#[cfg(test)]
mod tests_ws;
#[cfg(test)]
mod tests_ws_audio;
#[cfg(test)]
mod tests_ws_edge;

pub use bridge::{bind_default_bridge, bind_desktop_bridge, run_default_bridge};

use crate::ai::{ollama::OllamaConfig, router::TutorRouter};
use crate::database::{connection::init_sqlite_pool, repository::SessionRepository};
use crate::modules::conversation::SpeakingSessionHost;
use crate::preferences::LearnerPreferencesStore;
use bridge::build_bridge_session_host;
use router::{build_bridge_router, BridgeState};
use sqlx::SqlitePool;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;

/// Default loopback TCP port for the browser-to-Rust AI bridge.
pub const DEFAULT_SERVER_PORT: u16 = 3420;

/// Localhost HTTP server bound strictly to `127.0.0.1`.
pub struct LocalBridgeServer {
    listener: TcpListener,
    router: Arc<TutorRouter>,
    session_host: Arc<SpeakingSessionHost>,
    prefs: Arc<LearnerPreferencesStore>,
}

impl LocalBridgeServer {
    /// Binds the bridge server strictly to `127.0.0.1:{port}` with an in-memory SQLite session host.
    pub async fn bind(port: u16, ollama_base: OllamaConfig) -> std::io::Result<Self> {
        Self::bind_with_router(port, Arc::new(TutorRouter::new(ollama_base))).await
    }

    /// Binds the bridge server with a supplied `TutorRouter` and an in-memory SQLite session host.
    pub async fn bind_with_router(port: u16, router: Arc<TutorRouter>) -> std::io::Result<Self> {
        let pool = init_sqlite_pool("sqlite::memory:")
            .await
            .map_err(std::io::Error::other)?;
        Self::bind_with_router_and_pool(port, router, pool).await
    }

    /// Binds the bridge server with a supplied `TutorRouter` and SQLite connection pool.
    pub async fn bind_with_router_and_pool(
        port: u16,
        router: Arc<TutorRouter>,
        pool: SqlitePool,
    ) -> std::io::Result<Self> {
        let repo = SessionRepository::new(pool);
        let store = router
            .preferences_store()
            .cloned()
            .unwrap_or_else(|| LearnerPreferencesStore::new(repo.clone()));
        let prefs = Arc::new(store.clone());
        let host = Arc::new(
            build_bridge_session_host(Arc::clone(&router), repo).with_preferences_store(store),
        );
        Self::bind_with_parts(port, router, host, prefs).await
    }

    /// Binds the bridge server with explicit `TutorRouter` and `SpeakingSessionHost` instances.
    pub async fn bind_with_host(
        port: u16,
        router: Arc<TutorRouter>,
        session_host: Arc<SpeakingSessionHost>,
    ) -> std::io::Result<Self> {
        let prefs = match router
            .preferences_store()
            .cloned()
            .or_else(|| session_host.preferences_store().cloned())
        {
            Some(store) => Arc::new(store),
            None => {
                let pool = init_sqlite_pool("sqlite::memory:")
                    .await
                    .map_err(std::io::Error::other)?;
                Arc::new(LearnerPreferencesStore::new(SessionRepository::new(pool)))
            }
        };
        Self::bind_with_parts(port, router, session_host, prefs).await
    }

    async fn bind_with_parts(
        port: u16,
        router: Arc<TutorRouter>,
        session_host: Arc<SpeakingSessionHost>,
        prefs: Arc<LearnerPreferencesStore>,
    ) -> std::io::Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", port)).await?;
        Ok(Self {
            listener,
            router,
            session_host,
            prefs,
        })
    }

    /// Returns the bound local socket address (useful when binding ephemeral port `0` in tests).
    pub fn local_addr(&self) -> std::io::Result<SocketAddr> {
        self.listener.local_addr()
    }

    /// Returns the shared `TutorRouter` backing this server.
    pub fn router(&self) -> &Arc<TutorRouter> {
        &self.router
    }

    /// Returns the authoritative `SpeakingSessionHost` backing this server.
    pub fn session_host(&self) -> &Arc<SpeakingSessionHost> {
        &self.session_host
    }

    /// Serves incoming localhost HTTP, SSE, and WebSocket connections via Axum.
    pub async fn serve(self) -> std::io::Result<()> {
        let state = BridgeState {
            router: self.router,
            host: self.session_host,
            prefs: self.prefs,
        };
        axum::serve(self.listener, build_bridge_router(state)).await
    }
}
