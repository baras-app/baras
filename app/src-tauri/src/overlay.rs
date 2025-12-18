use tokio::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use tauri::State;

/// Type alias for shared overlay state (must match lib.rs)
pub type SharedOverlayState = Arc<Mutex<OverlayState>>;

use baras_overlay::{MeterEntry, MeterOverlay, OverlayConfig};

/// Commands sent to the overlay thread
pub enum OverlayCommand {
    SetMoveMode(bool),
    UpdateEntries(Vec<MeterEntry>),
    Shutdown,
}

/// State managing the overlay thread
#[derive(Debug, Default)]
pub struct OverlayState {
    pub(crate) tx: Option<Sender<OverlayCommand>>,
    handle: Option<JoinHandle<()>>,
    is_running: bool,
    move_mode: bool,
}

/// Spawn the overlay on a separate thread
pub async fn spawn_overlay() -> (Sender<OverlayCommand>, JoinHandle<()>) {
    let (tx, mut rx) = mpsc::channel::<OverlayCommand>(32);

    let handle = thread::spawn(move || {
        let config = OverlayConfig {
            x: 50,
            y: 50,
            width: 280,
            height: 200,
            namespace: "baras-dps".to_string(),
            click_through: true,
        };

        let mut overlay = match MeterOverlay::new(config, "DPS Meter") {
            Ok(o) => o,
            Err(e) => {
                eprintln!("Failed to create overlay: {}", e);
                return;
            }
        };

        // Overlay starts empty - real data comes via UpdateEntries commands
        let mut needs_render = true; // Initial render to show empty state

        loop {
            // Check for commands (non-blocking)
            while let Ok(cmd) = rx.try_recv() {
                match cmd {
                    OverlayCommand::SetMoveMode(enabled) => {
                        overlay.window_mut().set_click_through(!enabled);
                        needs_render = true; // Mode change might affect appearance
                    }
                    OverlayCommand::UpdateEntries(entries) => {
                        overlay.set_entries(entries);
                        needs_render = true; // New data to display
                    }
                    OverlayCommand::Shutdown => {
                        return;
                    }
                }
            }

            // Poll Wayland events (always needed for window management)
            if !overlay.poll_events() {
                break;
            }

            // Check if window was resized/moved (needs re-render)
            if overlay.window_mut().pending_size().is_some() {
                needs_render = true;
            }

            let is_interactive = overlay.window_mut().is_interactive();

            // Only render when needed:
            // - Interactive mode: always render for smooth drag/resize
            // - Locked mode: only render when data changed
            if needs_render || is_interactive {
                overlay.render();
                needs_render = false;
            }

            // Adaptive sleep:
            // - Interactive mode: 1ms for responsive input
            // - Locked mode: 50ms since we're just waiting for commands
            let sleep_ms = if is_interactive { 1 } else { 50 };
            thread::sleep(std::time::Duration::from_millis(sleep_ms));
        }
    });

    (tx, handle)
}

#[tauri::command]
pub async fn show_overlay(
    state: State<'_, SharedOverlayState>,
    service: State<'_, crate::service::ServiceHandle>,
) -> Result<bool, String> {
    // Check if already running (release lock before await)
    {
        let state = state.lock().map_err(|e| e.to_string())?;
        if state.is_running {
            return Ok(true);
        }
    }

    // Spawn without holding lock
    let (tx, handle) = spawn_overlay().await;

    // Re-acquire to update state
    {
        let mut state = state.lock().map_err(|e| e.to_string())?;
        state.tx = Some(tx.clone());
        state.handle = Some(handle);
        state.is_running = true;
    }

    // If tailing is active, send current metrics to the new overlay
    if service.is_tailing().await {
        if let Some(metrics) = service.current_metrics().await {
            if !metrics.is_empty() {
                let entries: Vec<_> = metrics
                    .iter()
                    .map(|m| MeterEntry::new(&m.name, m.dps, m.dps))
                    .collect();

                let max_dps = entries.iter().map(|e| e.value).fold(0, i64::max);
                let entries: Vec<_> = entries
                    .into_iter()
                    .map(|mut e| {
                        e.max_value = max_dps;
                        e
                    })
                    .collect();

                let _ = tx.send(OverlayCommand::UpdateEntries(entries)).await;
            }
        }
    }

    Ok(true)
}

#[tauri::command]
pub async fn hide_overlay(state: State<'_, SharedOverlayState>) -> Result<bool, String> {
    // Extract what we need, release lock before await
    let (tx, handle) = {
        let mut state = state.lock().map_err(|e| e.to_string())?;
        if !state.is_running {
            return Ok(true);
        }
        (state.tx.take(), state.handle.take())
    };

    // Send shutdown without holding lock
    if let Some(tx) = tx {
        let _ = tx.send(OverlayCommand::Shutdown).await;
    }

    // Join the thread (blocking, but lock is released)
    if let Some(handle) = handle {
        let _ = handle.join();
    }

    // Re-acquire to update state
    let mut state = state.lock().map_err(|e| e.to_string())?;
    state.is_running = false;
    state.move_mode = false;

    Ok(true)
}

#[tauri::command]
pub async fn toggle_move_mode(state: State<'_, SharedOverlayState>) -> Result<bool, String> {
    // Extract tx and toggle move_mode, release lock before await
    let (tx, new_mode) = {
        let mut state = state.lock().map_err(|e| e.to_string())?;
        if !state.is_running {
            return Err("Overlay not running".to_string());
        }
        state.move_mode = !state.move_mode;
        (state.tx.clone(), state.move_mode)
    };

    // Send command without holding lock
    if let Some(tx) = tx {
        tx.send(OverlayCommand::SetMoveMode(new_mode)).await
            .map_err(|e| e.to_string())?;
    }

    Ok(new_mode)
}

#[tauri::command]
pub fn get_overlay_status(state: State<'_, SharedOverlayState>) -> Result<(bool, bool), String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    Ok((state.is_running, state.move_mode))
}


