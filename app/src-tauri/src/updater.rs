//! Auto-updater module
//!
//! Checks for updates on startup and caches the Update object.
//! The user can then download and install without re-fetching.

use serde::Serialize;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};

/// Cached pending update stored in Tauri managed state
#[derive(Default)]
pub struct PendingUpdate(pub Mutex<Option<Update>>);

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
            tracing::error!(error = %e, "Update check failed");
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

        // Cache the update for later installation
        if let Some(state) = app.try_state::<PendingUpdate>() {
            *state.0.lock().unwrap_or_else(|poisoned| {
                tracing::warn!("PendingUpdate mutex poisoned, recovering");
                poisoned.into_inner()
            }) = Some(update);
        }

        // Emit event to frontend
        app.emit("update-available", info)?;
    }

    Ok(())
}

/// Download and install the cached pending update
#[tauri::command]
pub async fn install_update(app: AppHandle) -> Result<(), String> {
    // Take the cached update (removes it from state)
    let update = app
        .try_state::<PendingUpdate>()
        .and_then(|state| {
            state
                .0
                .lock()
                .unwrap_or_else(|poisoned| {
                    tracing::warn!("PendingUpdate mutex poisoned, recovering");
                    poisoned.into_inner()
                })
                .take()
        })
        .ok_or("No pending update available")?;

    // Download and install
    let mut downloaded: usize = 0;
    let app_handle = app.clone();

    let result = update
        .download_and_install(
            |chunk, total| {
                downloaded += chunk;
                if let Some(total) = total {
                    let progress = (downloaded as f64 / total as f64) * 100.0;
                    let _ = app_handle.emit("update-progress", progress);
                }
            },
            || {},
        )
        .await;

    match result {
        Ok(()) => {
            app.restart();
        }
        Err(e) => {
            let msg = e.to_string();
            // Emit failure event so frontend can show error and reset state
            let _ = app.emit("update-failed", &msg);
            return Err(msg);
        }
    }
}

/// Manually check for updates (called from frontend)
#[tauri::command]
pub async fn check_update(app: AppHandle) -> Result<Option<UpdateAvailable>, String> {
    let updater = app.updater().map_err(|e| e.to_string())?;

    let update = updater.check().await.map_err(|e| e.to_string())?;

    Ok(update.map(|u| {
        let info = UpdateAvailable {
            version: u.version.clone(),
            notes: u.body.clone(),
            date: u.date.map(|d| d.to_string()),
        };

        // Cache the update for later installation
        if let Some(state) = app.try_state::<PendingUpdate>() {
            *state.0.lock().unwrap_or_else(|poisoned| {
                tracing::warn!("PendingUpdate mutex poisoned, recovering");
                poisoned.into_inner()
            }) = Some(u);
        }

        info
    }))
}
