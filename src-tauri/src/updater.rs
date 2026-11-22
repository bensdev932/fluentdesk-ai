//! Signed GitHub Releases auto-updater commands backed by `tauri-plugin-updater`.

use tauri::AppHandle;
use tauri_plugin_updater::{Update, UpdaterExt};

/// UI-facing snapshot describing the desktop app's current updater status.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopUpdateSnapshot {
    pub status: &'static str,
    pub current_version: String,
    pub latest_version: Option<String>,
    pub notes: Option<String>,
    pub message: String,
}

/// Checks the latest signed release manifest using strict no-cache headers.
#[tauri::command]
pub async fn check_desktop_update(app: AppHandle) -> DesktopUpdateSnapshot {
    let current = app.package_info().version.to_string();
    match fetch_latest_update(&app).await {
        Ok(Some(update)) => available_snapshot(current, update),
        Ok(None) => up_to_date_snapshot(current),
        Err(err) => error_snapshot(current, &err),
    }
}

/// Downloads, verifies, and installs the pending signed update, then restarts the app.
#[tauri::command]
pub async fn install_desktop_update(app: AppHandle) -> Result<DesktopUpdateSnapshot, String> {
    let current = app.package_info().version.to_string();
    let Some(update) = fetch_latest_update(&app).await? else {
        return Ok(up_to_date_snapshot(current));
    };
    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(|e| e.to_string())?;
    app.restart();
}

async fn fetch_latest_update(app: &AppHandle) -> Result<Option<Update>, String> {
    let updater = app
        .updater_builder()
        .header("Cache-Control", "no-cache, no-store, must-revalidate")
        .and_then(|b| b.header("Pragma", "no-cache"))
        .and_then(|b| b.build())
        .map_err(|e| e.to_string())?;
    updater.check().await.map_err(|e| e.to_string())
}

fn available_snapshot(current_version: String, update: Update) -> DesktopUpdateSnapshot {
    let message = format!("Update v{} available.", update.version);
    DesktopUpdateSnapshot {
        status: "update_available",
        current_version,
        latest_version: Some(update.version),
        notes: update.body,
        message,
    }
}

fn up_to_date_snapshot(current_version: String) -> DesktopUpdateSnapshot {
    let message = format!("Up to date (v{current_version}).");
    DesktopUpdateSnapshot {
        status: "up_to_date",
        current_version,
        latest_version: None,
        notes: None,
        message,
    }
}

fn error_snapshot(current_version: String, err: &str) -> DesktopUpdateSnapshot {
    let lower = err.to_ascii_lowercase();
    let rate_limited = lower.contains("429") || lower.contains("rate limit");
    DesktopUpdateSnapshot {
        status: if rate_limited {
            "rate_limited"
        } else {
            "error"
        },
        current_version,
        latest_version: None,
        notes: None,
        message: if rate_limited {
            "Update check rate-limited; retry shortly.".to_string()
        } else {
            format!("Update check failed: {err}")
        },
    }
}
