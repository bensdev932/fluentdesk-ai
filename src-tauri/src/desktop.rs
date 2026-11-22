//! Tauri v2 desktop window bootstrap hosting the embedded localhost AI bridge.

use fluentdesk_core::local_server::{bind_desktop_bridge, DEFAULT_SERVER_PORT};
use tauri::{App, AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

const MAIN_WINDOW_LABEL: &str = "main";

/// Launches the FluentDesk AI Tauri desktop window with single-instance focus.
pub fn run() {
    fluentdesk_core::telemetry::init_default_tracing();
    if let Err(err) = build_app().run(tauri::generate_context!()) {
        tracing::error!(error = %err, "FluentDesk AI desktop shell failed");
        std::process::exit(1);
    }
}

fn build_app() -> tauri::Builder<tauri::Wry> {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(focus_main_window))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            crate::updater::check_desktop_update,
            crate::updater::install_desktop_update
        ])
        .setup(setup_desktop_app)
}

fn setup_desktop_app(app: &mut App) -> Result<(), Box<dyn std::error::Error>> {
    let server = tauri::async_runtime::block_on(bind_desktop_bridge(DEFAULT_SERVER_PORT))
        .map_err(std::io::Error::other)?;
    let port = server.local_addr()?.port();
    tracing::info!(port, "FluentDesk AI embedded bridge listening");
    tauri::async_runtime::spawn(async move {
        if let Err(err) = server.serve().await {
            tracing::error!(error = %err, "Embedded bridge server stopped");
        }
    });
    create_main_window(app, port)
}

fn create_main_window(app: &App, port: u16) -> Result<(), Box<dyn std::error::Error>> {
    let init_js = format!("window.__FLUENTDESK_BRIDGE_URL__ = 'http://127.0.0.1:{port}';");
    WebviewWindowBuilder::new(app, MAIN_WINDOW_LABEL, WebviewUrl::App("index.html".into()))
        .title("FluentDesk AI")
        .inner_size(1200.0, 820.0)
        .min_inner_size(900.0, 640.0)
        .resizable(true)
        .center()
        .initialization_script(init_js)
        .build()?;
    Ok(())
}

fn focus_main_window(app: &AppHandle, _args: Vec<String>, _cwd: String) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}
