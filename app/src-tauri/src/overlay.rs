use tokio::sync::mpsc::{self, Sender};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use tauri::State;
use serde::{Deserialize, Serialize};

use baras_core::context::OverlayPositionConfig;
use baras_overlay::{colors, Color, MeterEntry, MeterOverlay, OverlayConfig};

/// Type alias for shared overlay state (must match lib.rs)
pub type SharedOverlayState = Arc<Mutex<OverlayState>>;

// ─────────────────────────────────────────────────────────────────────────────
// Overlay Types
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OverlayType {
    Dps,
    Hps,
    Tps,
}

impl OverlayType {
    pub fn title(&self) -> &'static str {
        match self {
            OverlayType::Dps => "DPS",
            OverlayType::Hps => "HPS",
            OverlayType::Tps => "TPS",
        }
    }

    pub fn namespace(&self) -> &'static str {
        match self {
            OverlayType::Dps => "baras-dps",
            OverlayType::Hps => "baras-hps",
            OverlayType::Tps => "baras-tps",
        }
    }

    pub fn default_position(&self) -> (i32, i32) {
        match self {
            OverlayType::Dps => (50, 50),
            OverlayType::Hps => (50, 280),
            OverlayType::Tps => (50, 510),
        }
    }

    pub fn all() -> &'static [OverlayType] {
        &[OverlayType::Dps, OverlayType::Hps, OverlayType::Tps]
    }

    /// Config key for position storage
    pub fn config_key(&self) -> &'static str {
        match self {
            OverlayType::Dps => "dps",
            OverlayType::Hps => "hps",
            OverlayType::Tps => "tps",
        }
    }

    /// Parse from config key string
    pub fn from_config_key(key: &str) -> Option<Self> {
        match key {
            "dps" => Some(OverlayType::Dps),
            "hps" => Some(OverlayType::Hps),
            "tps" => Some(OverlayType::Tps),
            _ => None,
        }
    }

    /// Bar fill color for this overlay type
    pub fn bar_color(&self) -> Color {
        match self {
            OverlayType::Dps => colors::dps_bar_fill(),
            OverlayType::Hps => colors::hps_bar_fill(),
            OverlayType::Tps => colors::tank_bar_fill(),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Overlay Commands and State
// ─────────────────────────────────────────────────────────────────────────────

/// Commands sent to an overlay thread
pub enum OverlayCommand {
    SetMoveMode(bool),
    UpdateEntries(Vec<MeterEntry>),
    Shutdown,
}

/// Position update event from overlay thread
#[derive(Debug, Clone)]
pub struct PositionEvent {
    pub overlay_type: OverlayType,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// Handle to a single overlay instance
pub struct OverlayHandle {
    pub tx: Sender<OverlayCommand>,
    pub handle: JoinHandle<()>,
}

/// Sender for position events (stored globally for position persistence)
pub type PositionEventSender = std::sync::mpsc::Sender<PositionEvent>;

/// State managing multiple overlay threads
pub struct OverlayState {
    overlays: HashMap<OverlayType, OverlayHandle>,
    move_mode: bool,
    position_tx: Option<PositionEventSender>,
}

impl Default for OverlayState {
    fn default() -> Self {
        Self {
            overlays: HashMap::new(),
            move_mode: false,
            position_tx: None,
        }
    }
}

impl std::fmt::Debug for OverlayState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OverlayState")
            .field("overlays", &self.overlays.keys().collect::<Vec<_>>())
            .field("move_mode", &self.move_mode)
            .finish()
    }
}

impl OverlayState {
    pub fn is_running(&self, overlay_type: OverlayType) -> bool {
        self.overlays.contains_key(&overlay_type)
    }

    pub fn any_running(&self) -> bool {
        !self.overlays.is_empty()
    }

    pub fn get_tx(&self, overlay_type: OverlayType) -> Option<&Sender<OverlayCommand>> {
        self.overlays.get(&overlay_type).map(|h| &h.tx)
    }

    pub fn all_txs(&self) -> Vec<&Sender<OverlayCommand>> {
        self.overlays.values().map(|h| &h.tx).collect()
    }

    pub fn running_types(&self) -> Vec<OverlayType> {
        self.overlays.keys().copied().collect()
    }

    pub fn set_position_tx(&mut self, tx: PositionEventSender) {
        self.position_tx = Some(tx);
    }

    pub fn position_tx(&self) -> Option<&PositionEventSender> {
        self.position_tx.as_ref()
    }

    pub fn insert(&mut self, overlay_type: OverlayType, handle: OverlayHandle) {
        self.overlays.insert(overlay_type, handle);
    }

    pub fn remove(&mut self, overlay_type: OverlayType) -> Option<OverlayHandle> {
        self.overlays.remove(&overlay_type)
    }

    pub fn drain(&mut self) -> Vec<OverlayHandle> {
        self.overlays.drain().map(|(_, h)| h).collect()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Overlay Spawning
// ─────────────────────────────────────────────────────────────────────────────

/// Spawn an overlay on a separate thread
pub fn spawn_overlay(
    overlay_type: OverlayType,
    position: OverlayPositionConfig,
    position_tx: Option<PositionEventSender>,
) -> (Sender<OverlayCommand>, JoinHandle<()>) {
    let (tx, mut rx) = mpsc::channel::<OverlayCommand>(32);

    let title = overlay_type.title().to_string();
    let namespace = overlay_type.namespace().to_string();

    let handle = thread::spawn(move || {
        let config = OverlayConfig {
            x: position.x,
            y: position.y,
            width: position.width,
            height: position.height,
            namespace,
            click_through: true,
        };

        let mut overlay = match MeterOverlay::new(config, &title) {
            Ok(o) => o,
            Err(e) => {
                eprintln!("Failed to create overlay {}: {}", title, e);
                return;
            }
        };

        let mut needs_render = true;

        loop {
            while let Ok(cmd) = rx.try_recv() {
                match cmd {
                    OverlayCommand::SetMoveMode(enabled) => {
                        overlay.window_mut().set_click_through(!enabled);
                        needs_render = true;
                    }
                    OverlayCommand::UpdateEntries(entries) => {
                        overlay.set_entries(entries);
                        needs_render = true;
                    }
                    OverlayCommand::Shutdown => {
                        // Send final position before shutdown
                        if let Some(ref tx) = position_tx {
                            let _ = tx.send(PositionEvent {
                                overlay_type,
                                x: overlay.window_mut().x(),
                                y: overlay.window_mut().y(),
                                width: overlay.window_mut().width(),
                                height: overlay.window_mut().height(),
                            });
                        }
                        return;
                    }
                }
            }

            if !overlay.poll_events() {
                // Send final position on window close
                if let Some(ref tx) = position_tx {
                    let _ = tx.send(PositionEvent {
                        overlay_type,
                        x: overlay.window_mut().x(),
                        y: overlay.window_mut().y(),
                        width: overlay.window_mut().width(),
                        height: overlay.window_mut().height(),
                    });
                }
                break;
            }

            if overlay.window_mut().pending_size().is_some() {
                needs_render = true;
            }

            // Check for position changes and report them
            if overlay.window_mut().take_position_dirty() {
                if let Some(ref tx) = position_tx {
                    let _ = tx.send(PositionEvent {
                        overlay_type,
                        x: overlay.window_mut().x(),
                        y: overlay.window_mut().y(),
                        width: overlay.window_mut().width(),
                        height: overlay.window_mut().height(),
                    });
                }
            }

            let is_interactive = overlay.window_mut().is_interactive();

            if needs_render || is_interactive {
                overlay.render();
                needs_render = false;
            }

            let sleep_ms = if is_interactive { 1 } else { 50 };
            thread::sleep(std::time::Duration::from_millis(sleep_ms));
        }
    });

    (tx, handle)
}

// ─────────────────────────────────────────────────────────────────────────────
// Tauri Commands
// ─────────────────────────────────────────────────────────────────────────────

/// Enable an overlay (persists to config, only shows if overlays_visible is true)
#[tauri::command]
pub async fn show_overlay(
    overlay_type: OverlayType,
    state: State<'_, SharedOverlayState>,
    service: State<'_, crate::service::ServiceHandle>,
) -> Result<bool, String> {
    // Get current config and update enabled state
    let mut config = service.config().await;
    config.overlay_settings.set_enabled(overlay_type.config_key(), true);

    // Save config immediately
    service.update_config(config.clone()).await?;

    // Only spawn overlay if global visibility is enabled
    if !config.overlay_settings.overlays_visible {
        return Ok(true);
    }

    // Check if already running and get position tx
    let position_tx = {
        let state = state.lock().map_err(|e| e.to_string())?;
        if state.is_running(overlay_type) {
            return Ok(true);
        }
        state.position_tx().cloned()
    };

    // Load position from config
    let position = config.overlay_settings.get_position(overlay_type.config_key());

    // Spawn without holding lock
    let (tx, handle) = spawn_overlay(overlay_type, position, position_tx);

    // Update state
    {
        let mut state = state.lock().map_err(|e| e.to_string())?;
        state.insert(overlay_type, OverlayHandle { tx: tx.clone(), handle });
    }

    // Send current metrics if tailing
    if service.is_tailing().await {
        if let Some(metrics) = service.current_metrics().await {
            if !metrics.is_empty() {
                let entries = create_entries_for_type(overlay_type, &metrics);
                let _ = tx.send(OverlayCommand::UpdateEntries(entries)).await;
            }
        }
    }

    Ok(true)
}

/// Disable an overlay (persists to config, hides if currently running)
#[tauri::command]
pub async fn hide_overlay(
    overlay_type: OverlayType,
    state: State<'_, SharedOverlayState>,
    service: State<'_, crate::service::ServiceHandle>,
) -> Result<bool, String> {
    eprintln!("hide_overlay called for {:?}", overlay_type);

    // Get current config and update enabled state
    let mut config = service.config().await;
    eprintln!("hide_overlay: before update, enabled = {:?}", config.overlay_settings.enabled);
    config.overlay_settings.set_enabled(overlay_type.config_key(), false);
    eprintln!("hide_overlay: after update, enabled = {:?}", config.overlay_settings.enabled);

    // Save config immediately
    service.update_config(config).await?;
    eprintln!("hide_overlay: config saved");

    // Shutdown overlay if running
    let overlay_handle = {
        let mut state = state.lock().map_err(|e| e.to_string())?;
        state.remove(overlay_type)
    };

    if let Some(handle) = overlay_handle {
        let _ = handle.tx.send(OverlayCommand::Shutdown).await;
        let _ = handle.handle.join();
    }

    Ok(true)
}

/// Hide all running overlays and set overlays_visible=false
#[tauri::command]
pub async fn hide_all_overlays(
    state: State<'_, SharedOverlayState>,
    service: State<'_, crate::service::ServiceHandle>,
) -> Result<bool, String> {
    // Update and persist overlays_visible = false
    let mut config = service.config().await;
    config.overlay_settings.overlays_visible = false;
    service.update_config(config).await?;

    // Shutdown all running overlays
    let handles: Vec<_> = {
        let mut state = state.lock().map_err(|e| e.to_string())?;
        state.move_mode = false;
        state.drain()
    };

    for handle in handles {
        let _ = handle.tx.send(OverlayCommand::Shutdown).await;
        let _ = handle.handle.join();
    }

    Ok(true)
}

/// Show all enabled overlays and set overlays_visible=true
#[tauri::command]
pub async fn show_all_overlays(
    state: State<'_, SharedOverlayState>,
    service: State<'_, crate::service::ServiceHandle>,
) -> Result<Vec<OverlayType>, String> {
    // Update and persist overlays_visible = true
    let mut config = service.config().await;
    config.overlay_settings.overlays_visible = true;
    service.update_config(config.clone()).await?;

    let enabled_keys = config.overlay_settings.enabled_types();
    eprintln!("show_all_overlays: enabled_keys = {:?}", enabled_keys);
    eprintln!("show_all_overlays: full enabled map = {:?}", config.overlay_settings.enabled);

    let mut shown = Vec::new();

    for key in enabled_keys {
        let Some(overlay_type) = OverlayType::from_config_key(&key) else {
            continue;
        };

        // Check if already running
        let (is_running, position_tx) = {
            let state = state.lock().map_err(|e| e.to_string())?;
            (state.is_running(overlay_type), state.position_tx().cloned())
        };

        if is_running {
            shown.push(overlay_type);
            continue;
        }

        // Load position and spawn
        let position = config.overlay_settings.get_position(&key);
        let (tx, handle) = spawn_overlay(overlay_type, position, position_tx);

        // Update state
        {
            let mut state = state.lock().map_err(|e| e.to_string())?;
            state.insert(overlay_type, OverlayHandle { tx: tx.clone(), handle });
        }

        // Send current metrics if tailing
        if service.is_tailing().await {
            if let Some(metrics) = service.current_metrics().await {
                if !metrics.is_empty() {
                    let entries = create_entries_for_type(overlay_type, &metrics);
                    let _ = tx.send(OverlayCommand::UpdateEntries(entries)).await;
                }
            }
        }

        shown.push(overlay_type);
    }

    Ok(shown)
}

#[tauri::command]
pub async fn toggle_move_mode(state: State<'_, SharedOverlayState>) -> Result<bool, String> {
    let (txs, new_mode) = {
        let mut state = state.lock().map_err(|e| e.to_string())?;
        if !state.any_running() {
            return Err("No overlays running".to_string());
        }
        state.move_mode = !state.move_mode;
        (state.all_txs().into_iter().cloned().collect::<Vec<_>>(), state.move_mode)
    };

    // Send to all overlays
    for tx in txs {
        let _ = tx.send(OverlayCommand::SetMoveMode(new_mode)).await;
    }

    Ok(new_mode)
}

#[tauri::command]
pub async fn get_overlay_status(
    state: State<'_, SharedOverlayState>,
    service: State<'_, crate::service::ServiceHandle>,
) -> Result<OverlayStatusResponse, String> {
    let (running, move_mode) = {
        let state = state.lock().map_err(|e| e.to_string())?;
        (state.running_types(), state.move_mode)
    };

    // Get enabled types and visibility from config
    let config = service.config().await;
    let enabled: Vec<OverlayType> = config
        .overlay_settings
        .enabled_types()
        .iter()
        .filter_map(|key| OverlayType::from_config_key(key))
        .collect();

    Ok(OverlayStatusResponse {
        running,
        enabled,
        overlays_visible: config.overlay_settings.overlays_visible,
        move_mode,
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct OverlayStatusResponse {
    pub running: Vec<OverlayType>,
    pub enabled: Vec<OverlayType>,
    pub overlays_visible: bool,
    pub move_mode: bool,
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

use crate::service::PlayerMetrics;

pub fn create_entries_for_type(overlay_type: OverlayType, metrics: &[PlayerMetrics]) -> Vec<MeterEntry> {
    let color = overlay_type.bar_color();
    let (mut values, max_value): (Vec<_>, i64) = match overlay_type {
        OverlayType::Dps => {
            let max = metrics.iter().map(|m| m.dps).max().unwrap_or(0);
            (metrics.iter().map(|m| (m.name.clone(), m.dps)).collect(), max)
        }
        OverlayType::Hps => {
            let max = metrics.iter().map(|m| m.hps).max().unwrap_or(0);
            (metrics.iter().map(|m| (m.name.clone(), m.hps)).collect(), max)
        }
        OverlayType::Tps => {
            let max = metrics.iter().map(|m| m.tps).max().unwrap_or(0);
            (metrics.iter().map(|m| (m.name.clone(), m.tps)).collect(), max)
        }
    };

    // Sort by metric value descending (highest first)
    values.sort_by(|a, b| b.1.cmp(&a.1));

    values
        .into_iter()
        .map(|(name, value)| MeterEntry::new(&name, value, max_value).with_color(color))
        .collect()
}

/// Create entries for all overlay types from metrics
pub fn create_all_entries(metrics: &[PlayerMetrics]) -> HashMap<OverlayType, Vec<MeterEntry>> {
    let mut result = HashMap::new();
    for overlay_type in OverlayType::all() {
        result.insert(*overlay_type, create_entries_for_type(*overlay_type, metrics));
    }
    result
}
