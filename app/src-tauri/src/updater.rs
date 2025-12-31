//! Auto-updater module
//!
//! Checks for updates on startup and emits events to the frontend.
//! The user can then choose to download and install the update.

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tauri_plugin_updater::UpdaterExt;

#[derive(Clone, Serialize)]
pub struct UpdateAvailable {
    pub version: String,
    pub notes: Option<String>,
    pub date: Option<String>,
}

/// Check for updates and emit an event if one is available
pub fn spawn_update_check(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        // Small delay to let the app fully initialize
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;

        if let Err(e) = check_for_update(&app).await {
            eprintln!("Update check failed: {e}");
        }
    });
}

async fn check_for_update(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let updater = app.updater()?;

    if let Some(update) = updater.check().await? {
        let info = UpdateAvailable {
            version: update.version.clone(),
            notes: update.body.clone(),
            date: update.date.map(|d| d.to_string()),
        };

        // Emit event to frontend
        app.emit("update-available", info)?;
    }

    Ok(())
}

/// Download and install a pending update (called from frontend)
#[tauri::command]
pub async fn install_update(app: AppHandle) -> Result<(), String> {
    let updater = app.updater().map_err(|e| e.to_string())?;

    let update = updater
        .check()
        .await
        .map_err(|e| e.to_string())?
        .ok_or("No update available")?;

    // Download and install
    let mut downloaded = 0;

    update
        .download_and_install(
            |chunk, total| {
                downloaded += chunk;
                if let Some(total) = total {
                    let progress = (downloaded as f64 / total as f64) * 100.0;
                    let _ = app.emit("update-progress", progress);
                }
            },
            || {
                // Called before the app restarts
                let _ = app.emit("update-installing", ());
            },
        )
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

/// Manually check for updates (called from frontend)
#[tauri::command]
pub async fn check_update(app: AppHandle) -> Result<Option<UpdateAvailable>, String> {
    let updater = app.updater().map_err(|e| e.to_string())?;

    let update = updater.check().await.map_err(|e| e.to_string())?;

    Ok(update.map(|u| UpdateAvailable {
        version: u.version,
        notes: u.body,
        date: u.date.map(|d| d.to_string()),
    }))
}
