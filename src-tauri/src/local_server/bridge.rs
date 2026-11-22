//! Persistent bridge startup (`run_default_bridge`) and default session host builder.

use crate::ai::{
    ollama_discovery::ollama_config_from_env, router::TutorRouter, vault::default_storage_dir,
};
use crate::database::{connection::init_sqlite_pool_at_path, repository::SessionRepository};
use crate::local_server::LocalBridgeServer;
use crate::modules::{
    conversation::{manager::SessionPipelineAdapters, SpeakingSessionHost},
    speech::{audio_capture::MockAudioInput, playback::MockAudioOutput},
};
use crate::stt::mock::MockSttProvider;
use crate::tts::{mock::MockTtsProvider, phrase_cache::TtsPhraseCache};
use std::sync::Arc;

/// Initializes persistent SQLite storage and multi-provider router dependencies.
async fn init_default_bridge_deps() -> Result<(Arc<TutorRouter>, sqlx::SqlitePool), String> {
    let cfg = ollama_config_from_env().map_err(|e| e.user_message)?;
    let dir = default_storage_dir();
    let pool = init_sqlite_pool_at_path(&dir.join("fluentdesk.db"))
        .await
        .map_err(|e| e.user_message)?;
    let repo = SessionRepository::new(pool.clone());
    let router = TutorRouter::new_with_persistence(cfg, repo, dir)
        .await
        .map_err(|e| e.user_message)?;
    router.spawn_local_warmup_if_needed();
    Ok((Arc::new(router), pool))
}

/// Binds the persistent SQLite + multi-provider bridge strictly to `127.0.0.1:{port}`.
pub async fn bind_default_bridge(port: u16) -> Result<LocalBridgeServer, String> {
    let (router, pool) = init_default_bridge_deps().await?;
    LocalBridgeServer::bind_with_router_and_pool(port, router, pool)
        .await
        .map_err(|e| e.to_string())
}

/// Binds the desktop bridge on `preferred_port`, falling back to `127.0.0.1:0` if occupied.
pub async fn bind_desktop_bridge(preferred_port: u16) -> Result<LocalBridgeServer, String> {
    let (router, pool) = init_default_bridge_deps().await?;
    bind_with_port_fallback(preferred_port, router, pool)
        .await
        .map_err(|e| e.to_string())
}

pub(crate) async fn bind_with_port_fallback(
    preferred_port: u16,
    router: Arc<TutorRouter>,
    pool: sqlx::SqlitePool,
) -> std::io::Result<LocalBridgeServer> {
    let preferred = LocalBridgeServer::bind_with_router_and_pool(
        preferred_port,
        Arc::clone(&router),
        pool.clone(),
    )
    .await;
    match preferred {
        Ok(server) => Ok(server),
        Err(err) if err.kind() == std::io::ErrorKind::AddrInUse => {
            tracing::warn!(
                preferred_port,
                "Bridge port busy; binding free loopback port"
            );
            LocalBridgeServer::bind_with_router_and_pool(0, router, pool).await
        }
        Err(err) => Err(err),
    }
}

/// Resolves Ollama config and runs the persistent SQLite + multi-provider bridge on `127.0.0.1:{port}`.
pub async fn run_default_bridge(port: u16) -> Result<(), String> {
    let server = bind_default_bridge(port).await?;
    let addr = server.local_addr().map_err(|e| e.to_string())?;
    tracing::info!(listen = %addr, "FluentDesk AI localhost HTTP + WS bridge listening");
    server.serve().await.map_err(|e| e.to_string())
}

pub(super) fn build_bridge_session_host(
    router: Arc<TutorRouter>,
    repository: SessionRepository,
) -> SpeakingSessionHost {
    let adapters = SessionPipelineAdapters {
        audio_input: Arc::new(MockAudioInput::default()),
        stt: Arc::new(MockSttProvider::default()),
        ai: router,
        tts: Arc::new(TtsPhraseCache::new(Arc::new(MockTtsProvider::default()))),
        audio_output: Arc::new(MockAudioOutput::new()),
    };
    SpeakingSessionHost::new(adapters, Some(repository))
}
