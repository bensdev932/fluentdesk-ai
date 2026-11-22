use crate::ai::{ollama::OllamaConfig, router::TutorRouter};
use crate::database::connection::init_sqlite_pool;
use crate::local_server::bridge::bind_with_port_fallback;
use std::sync::Arc;
use tokio::net::TcpListener;

#[tokio::test]
async fn bind_with_port_fallback_uses_free_loopback_port_when_busy() {
    let occupant = TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("bind occupant listener");
    let busy_port = occupant.local_addr().expect("occupant addr").port();

    let pool = init_sqlite_pool("sqlite::memory:")
        .await
        .expect("in-memory sqlite");
    let cfg = OllamaConfig::try_new("127.0.0.1", 11434, "qwen3.5:4b").expect("valid ollama config");
    let router = Arc::new(TutorRouter::new(cfg));

    let server = bind_with_port_fallback(busy_port, router, pool)
        .await
        .expect("fallback bind should succeed when preferred port is occupied");
    let bound = server.local_addr().expect("bound addr");

    assert!(bound.ip().is_loopback());
    assert_ne!(bound.port(), 0);
    assert_ne!(bound.port(), busy_port);
}
