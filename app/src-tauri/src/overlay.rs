use tokio::sync::mpsc::{self, Sender};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use tauri::State;
use serde::{Deserialize, Serialize};

use baras_core::context::{OverlayPositionConfig, OverlayAppearanceConfig, PersonalOverlayConfig};
use baras_overlay::{colors, Color, MeterEntry, MetricOverlay, PersonalOverlay, PersonalStats, OverlayConfig};

/// Type alias for shared overlay state (must match lib.rs)
pub type SharedOverlayState = Arc<Mutex<OverlayState>>;

// ─────────────────────────────────────────────────────────────────────────────
// Overlay Types
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OverlayType {
    Dps,
    EDps,
    Hps,
    EHps,
    Tps,
    Dtps,
    EDtps,
    Abs,
}

impl OverlayType {
    pub fn title(&self) -> &'static str {
        match self {
            OverlayType::Dps => "DPS",
            OverlayType::EDps => "eDPS",
            OverlayType::Hps => "HPS",
            OverlayType::EHps => "eHPS",
            OverlayType::Tps => "TPS",
            OverlayType::Dtps => "DTPS",
            OverlayType::EDtps => "eDTPS",
            OverlayType::Abs => "ABS",
        }
    }

    pub fn namespace(&self) -> &'static str {
        match self {
            OverlayType::Dps => "baras-dps",
            OverlayType::EDps => "baras-edps",
            OverlayType::Hps => "baras-hps",
            OverlayType::EHps => "baras-ehps",
            OverlayType::Tps => "baras-tps",
            OverlayType::Dtps => "baras-dtps",
            OverlayType::EDtps => "baras-edtps",
            OverlayType::Abs => "baras-abs",
        }
    }

    pub fn default_position(&self) -> (i32, i32) {
        match self {
            OverlayType::Dps => (50, 50),
            OverlayType::EDps => (50, 50),
            OverlayType::Hps => (50, 280),
            OverlayType::EHps => (50, 280),
            OverlayType::Tps => (50, 510),
            OverlayType::Dtps => (350, 50),
            OverlayType::EDtps => (350, 50),
            OverlayType::Abs => (350, 280),
        }
    }

    pub fn all() -> &'static [OverlayType] {
        &[
            OverlayType::Dps,
            OverlayType::EDps,
            OverlayType::Hps,
            OverlayType::EHps,
            OverlayType::Tps,
            OverlayType::Dtps,
            OverlayType::EDtps,
            OverlayType::Abs,
        ]
    }

    /// Config key for position storage
    pub fn config_key(&self) -> &'static str {
        match self {
            OverlayType::Dps => "dps",
            OverlayType::EDps => "edps",
            OverlayType::Hps => "hps",
            OverlayType::EHps => "ehps",
            OverlayType::Tps => "tps",
            OverlayType::Dtps => "dtps",
            OverlayType::EDtps => "edtps",
            OverlayType::Abs => "abs",
        }
    }

    /// Parse from config key string
    pub fn from_config_key(key: &str) -> Option<Self> {
        match key {
            "dps" => Some(OverlayType::Dps),
            "edps" => Some(OverlayType::EDps),
            "hps" => Some(OverlayType::Hps),
            "ehps" => Some(OverlayType::EHps),
            "tps" => Some(OverlayType::Tps),
            "dtps" => Some(OverlayType::Dtps),
            "edtps" => Some(OverlayType::EDtps),
            "abs" => Some(OverlayType::Abs),
            _ => None,
        }
    }

    /// Bar fill color for this overlay type
    pub fn bar_color(&self) -> Color {
        match self {
            OverlayType::Dps | OverlayType::EDps => colors::dps_bar_fill(),
            OverlayType::Hps | OverlayType::EHps => colors::hps_bar_fill(),
            OverlayType::Tps => colors::tank_bar_fill(),
            OverlayType::Dtps | OverlayType::EDtps => Color::from_rgba8(180, 80, 80, 255), // Red-ish for damage taken
            OverlayType::Abs => Color::from_rgba8(100, 150, 200, 255), // Blue-ish for absorption
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
    /// Update appearance config and background alpha
    UpdateConfig(OverlayAppearanceConfig, u8),
    /// Request current position via oneshot channel
    GetPosition(tokio::sync::oneshot::Sender<PositionEvent>),
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

// ─────────────────────────────────────────────────────────────────────────────
// Personal Overlay
// ─────────────────────────────────────────────────────────────────────────────

/// Commands sent to a personal overlay thread
pub enum PersonalOverlayCommand {
    SetMoveMode(bool),
    UpdateStats(PersonalStats),
    /// Update personal overlay config and background alpha
    UpdateConfig(PersonalOverlayConfig, u8),
    Shutdown,
}

/// Handle to the personal overlay instance
pub struct PersonalOverlayHandle {
    pub tx: Sender<PersonalOverlayCommand>,
    pub handle: JoinHandle<()>,
}

/// State managing multiple overlay threads
pub struct OverlayState {
    overlays: HashMap<OverlayType, OverlayHandle>,
    personal: Option<PersonalOverlayHandle>,
    move_mode: bool,
}

impl Default for OverlayState {
    fn default() -> Self {
        Self {
            overlays: HashMap::new(),
            personal: None,
            move_mode: false,
        }
    }
}

impl std::fmt::Debug for OverlayState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OverlayState")
            .field("overlays", &self.overlays.keys().collect::<Vec<_>>())
            .field("personal", &self.personal.is_some())
            .field("move_mode", &self.move_mode)
            .finish()
    }
}

impl OverlayState {
    pub fn is_running(&self, overlay_type: OverlayType) -> bool {
        self.overlays.contains_key(&overlay_type)
    }

    pub fn any_running(&self) -> bool {
        !self.overlays.is_empty() || self.personal.is_some()
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

    pub fn insert(&mut self, overlay_type: OverlayType, handle: OverlayHandle) {
        self.overlays.insert(overlay_type, handle);
    }

    pub fn remove(&mut self, overlay_type: OverlayType) -> Option<OverlayHandle> {
        self.overlays.remove(&overlay_type)
    }

    pub fn drain(&mut self) -> Vec<OverlayHandle> {
        self.overlays.drain().map(|(_, h)| h).collect()
    }

    // Personal overlay methods
    pub fn is_personal_running(&self) -> bool {
        self.personal.is_some()
    }

    pub fn set_personal(&mut self, handle: PersonalOverlayHandle) {
        self.personal = Some(handle);
    }

    pub fn get_personal_tx(&self) -> Option<&Sender<PersonalOverlayCommand>> {
        self.personal.as_ref().map(|h| &h.tx)
    }

    pub fn take_personal(&mut self) -> Option<PersonalOverlayHandle> {
        self.personal.take()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Overlay Spawning
// ─────────────────────────────────────────────────────────────────────────────

/// Spawn an overlay on a separate thread
pub fn spawn_overlay(
    overlay_type: OverlayType,
    position: OverlayPositionConfig,
    appearance: OverlayAppearanceConfig,
    background_alpha: u8,
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

        let mut overlay = match MetricOverlay::new(config, &title, appearance, background_alpha) {
            Ok(o) => o,
            Err(e) => {
                eprintln!("Failed to create overlay {}: {}", title, e);
                return;
            }
        };

        let mut needs_render = true;
        let mut was_in_resize_corner = false;
        let mut was_resizing = false;

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
                    OverlayCommand::UpdateConfig(appearance, bg_alpha) => {
                        overlay.set_appearance(appearance);
                        overlay.set_background_alpha(bg_alpha);
                        needs_render = true;
                    }
                    OverlayCommand::GetPosition(response_tx) => {
                        let _ = response_tx.send(PositionEvent {
                            overlay_type,
                            x: overlay.window_mut().x(),
                            y: overlay.window_mut().y(),
                            width: overlay.window_mut().width(),
                            height: overlay.window_mut().height(),
                        });
                    }
                    OverlayCommand::Shutdown => {
                        return;
                    }
                }
            }

            if !overlay.poll_events() {
                break;
            }

            if overlay.window_mut().pending_size().is_some() {
                needs_render = true;
            }

            // Clear position dirty flag (position is saved on lock, not continuously)
            let _ = overlay.window_mut().take_position_dirty();

            // Check if resize corner state changed (need to show/hide grip)
            let in_resize_corner = overlay.window_mut().in_resize_corner();
            let is_resizing = overlay.window_mut().is_resizing();
            if in_resize_corner != was_in_resize_corner || is_resizing != was_resizing {
                needs_render = true;
                was_in_resize_corner = in_resize_corner;
                was_resizing = is_resizing;
            }

            let is_interactive = overlay.window_mut().is_interactive();

            if needs_render {
                overlay.render();
                needs_render = false;
            }

            // Sleep longer when locked (no interaction), shorter when interactive
            let sleep_ms = if is_interactive { 16 } else { 50 };
            thread::sleep(std::time::Duration::from_millis(sleep_ms));
        }
    });

    (tx, handle)
}

/// Spawn the personal overlay on a separate thread
pub fn spawn_personal_overlay(
    position: OverlayPositionConfig,
    config: PersonalOverlayConfig,
    background_alpha: u8,
) -> (Sender<PersonalOverlayCommand>, JoinHandle<()>) {
    let (tx, mut rx) = mpsc::channel::<PersonalOverlayCommand>(32);

    let handle = thread::spawn(move || {
        let window_config = OverlayConfig {
            x: position.x,
            y: position.y,
            width: position.width,
            height: position.height,
            namespace: "baras-personal".to_string(),
            click_through: true,
        };

        let mut overlay = match PersonalOverlay::new(window_config, config, background_alpha) {
            Ok(o) => o,
            Err(e) => {
                eprintln!("Failed to create personal overlay: {}", e);
                return;
            }
        };

        let mut needs_render = true;
        let mut was_in_resize_corner = false;
        let mut was_resizing = false;

        loop {
            while let Ok(cmd) = rx.try_recv() {
                match cmd {
                    PersonalOverlayCommand::SetMoveMode(enabled) => {
                        overlay.window_mut().set_click_through(!enabled);
                        needs_render = true;
                    }
                    PersonalOverlayCommand::UpdateStats(stats) => {
                        overlay.set_stats(stats);
                        needs_render = true;
                    }
                    PersonalOverlayCommand::UpdateConfig(config, bg_alpha) => {
                        overlay.set_config(config);
                        overlay.set_background_alpha(bg_alpha);
                        needs_render = true;
                    }
                    PersonalOverlayCommand::Shutdown => {
                        return;
                    }
                }
            }

            if !overlay.poll_events() {
                break;
            }

            if overlay.window_mut().pending_size().is_some() {
                needs_render = true;
            }

            // Clear position dirty flag
            let _ = overlay.window_mut().take_position_dirty();

            // Check if resize corner state changed
            let in_resize_corner = overlay.window_mut().in_resize_corner();
            let is_resizing = overlay.window_mut().is_resizing();
            if in_resize_corner != was_in_resize_corner || is_resizing != was_resizing {
                needs_render = true;
                was_in_resize_corner = in_resize_corner;
                was_resizing = is_resizing;
            }

            let is_interactive = overlay.window_mut().is_interactive();

            if needs_render {
                overlay.render();
                needs_render = false;
            }

            let sleep_ms = if is_interactive { 16 } else { 50 };
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

    // Check if already running
    {
        let state = state.lock().map_err(|e| e.to_string())?;
        if state.is_running(overlay_type) {
            return Ok(true);
        }
    }

    // Load position and appearance from config
    let position = config.overlay_settings.get_position(overlay_type.config_key());
    let appearance = config.overlay_settings.get_appearance(overlay_type.config_key());
    let background_alpha = config.overlay_settings.background_alpha;

    // Spawn without holding lock
    let (tx, handle) = spawn_overlay(overlay_type, position, appearance, background_alpha);

    // Update state
    {
        let mut state = state.lock().map_err(|e| e.to_string())?;
        state.insert(overlay_type, OverlayHandle { tx: tx.clone(), handle });
    }

    // Send current metrics if tailing
    if service.is_tailing().await
        && let Some(metrics) = service.current_metrics().await
        && !metrics.is_empty()
    {
        let entries = create_entries_for_type(overlay_type, &metrics);
        let _ = tx.send(OverlayCommand::UpdateEntries(entries)).await;
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
    let (handles, personal_handle) = {
        let mut state = state.lock().map_err(|e| e.to_string())?;
        state.move_mode = false;
        (state.drain(), state.take_personal())
    };

    for handle in handles {
        let _ = handle.tx.send(OverlayCommand::Shutdown).await;
        let _ = handle.handle.join();
    }

    // Shutdown personal overlay
    if let Some(handle) = personal_handle {
        let _ = handle.tx.send(PersonalOverlayCommand::Shutdown).await;
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

    let mut shown = Vec::new();

    for key in &enabled_keys {
        // Skip "personal" - handled separately below
        if key == "personal" {
            continue;
        }

        let Some(overlay_type) = OverlayType::from_config_key(key) else {
            continue;
        };

        // Check if already running
        {
            let state = state.lock().map_err(|e| e.to_string())?;
            if state.is_running(overlay_type) {
                shown.push(overlay_type);
                continue;
            }
        }

        // Load position, appearance, and spawn
        let position = config.overlay_settings.get_position(key);
        let appearance = config.overlay_settings.get_appearance(key);
        let background_alpha = config.overlay_settings.background_alpha;
        let (tx, handle) = spawn_overlay(overlay_type, position, appearance, background_alpha);

        // Update state
        {
            let mut state = state.lock().map_err(|e| e.to_string())?;
            state.insert(overlay_type, OverlayHandle { tx: tx.clone(), handle });
        }

        // Send current metrics if tailing
        if service.is_tailing().await
            && let Some(metrics) = service.current_metrics().await
            && !metrics.is_empty()
        {
            let entries = create_entries_for_type(overlay_type, &metrics);
            let _ = tx.send(OverlayCommand::UpdateEntries(entries)).await;
        }

        shown.push(overlay_type);
    }

    // Spawn personal overlay if enabled
    if enabled_keys.iter().any(|k| k == "personal") {
        let already_running = {
            let state = state.lock().map_err(|e| e.to_string())?;
            state.is_personal_running()
        };

        if !already_running {
            let position = config.overlay_settings.get_position("personal");
            let personal_config = config.overlay_settings.personal_overlay.clone();
            let background_alpha = config.overlay_settings.background_alpha;
            let (tx, handle) = spawn_personal_overlay(position, personal_config, background_alpha);

            let mut state = state.lock().map_err(|e| e.to_string())?;
            state.set_personal(PersonalOverlayHandle { tx, handle });
        }
    }

    Ok(shown)
}

#[tauri::command]
pub async fn toggle_move_mode(
    state: State<'_, SharedOverlayState>,
    service: State<'_, crate::service::ServiceHandle>,
) -> Result<bool, String> {
    let (txs, personal_tx, new_mode) = {
        let mut state = state.lock().map_err(|e| e.to_string())?;
        if !state.any_running() {
            return Err("No overlays running".to_string());
        }
        state.move_mode = !state.move_mode;
        let txs: Vec<_> = state.all_txs().into_iter().cloned().collect();
        let personal_tx = state.get_personal_tx().cloned();
        (txs, personal_tx, state.move_mode)
    };

    // Send to all metric overlays
    for tx in &txs {
        let _ = tx.send(OverlayCommand::SetMoveMode(new_mode)).await;
    }

    // Send to personal overlay
    if let Some(tx) = &personal_tx {
        let _ = tx.send(PersonalOverlayCommand::SetMoveMode(new_mode)).await;
    }

    // When locking (move_mode = false), save all overlay positions
    if !new_mode {
        let mut positions = Vec::new();
        for tx in &txs {
            let (pos_tx, pos_rx) = tokio::sync::oneshot::channel();
            let _ = tx.send(OverlayCommand::GetPosition(pos_tx)).await;
            if let Ok(pos) = pos_rx.await {
                positions.push(pos);
            }
        }

        // Save positions to config
        let mut config = service.config().await;
        for pos in positions {
            config.overlay_settings.set_position(
                pos.overlay_type.config_key(),
                OverlayPositionConfig {
                    x: pos.x,
                    y: pos.y,
                    width: pos.width,
                    height: pos.height,
                    monitor_id: None,
                },
            );
        }
        // Note: Personal overlay position save requires adding GetPosition command
        // For now, personal overlay position is not persisted on lock
        service.update_config(config).await.map_err(|e| e.to_string())?;
    }

    Ok(new_mode)
}

#[tauri::command]
pub async fn get_overlay_status(
    state: State<'_, SharedOverlayState>,
    service: State<'_, crate::service::ServiceHandle>,
) -> Result<OverlayStatusResponse, String> {
    let (running, personal_running, move_mode) = {
        let state = state.lock().map_err(|e| e.to_string())?;
        (state.running_types(), state.is_personal_running(), state.move_mode)
    };

    // Get enabled types and visibility from config
    let config = service.config().await;
    let enabled: Vec<OverlayType> = config
        .overlay_settings
        .enabled_types()
        .iter()
        .filter_map(|key| OverlayType::from_config_key(key))
        .collect();

    let personal_enabled = config.overlay_settings.is_enabled("personal");

    Ok(OverlayStatusResponse {
        running,
        enabled,
        personal_running,
        personal_enabled,
        overlays_visible: config.overlay_settings.overlays_visible,
        move_mode,
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct OverlayStatusResponse {
    pub running: Vec<OverlayType>,
    pub enabled: Vec<OverlayType>,
    pub personal_running: bool,
    pub personal_enabled: bool,
    pub overlays_visible: bool,
    pub move_mode: bool,
}

// ─────────────────────────────────────────────────────────────────────────────
// Personal Overlay Commands
// ─────────────────────────────────────────────────────────────────────────────

/// Show the personal overlay
#[tauri::command]
pub async fn show_personal_overlay(
    state: State<'_, SharedOverlayState>,
    service: State<'_, crate::service::ServiceHandle>,
) -> Result<bool, String> {
    // Get current config and update enabled state
    let mut config = service.config().await;
    config.overlay_settings.set_enabled("personal", true);
    service.update_config(config.clone()).await?;

    // Only spawn if global visibility is enabled
    if !config.overlay_settings.overlays_visible {
        return Ok(true);
    }

    // Check if already running
    {
        let state = state.lock().map_err(|e| e.to_string())?;
        if state.is_personal_running() {
            return Ok(true);
        }
    }

    // Load position and config
    let position = config.overlay_settings.get_position("personal");
    let personal_config = config.overlay_settings.personal_overlay.clone();
    let background_alpha = config.overlay_settings.background_alpha;

    // Spawn
    let (tx, handle) = spawn_personal_overlay(position, personal_config, background_alpha);

    // Update state
    {
        let mut state = state.lock().map_err(|e| e.to_string())?;
        state.set_personal(PersonalOverlayHandle { tx, handle });
    }

    Ok(true)
}

/// Hide the personal overlay
#[tauri::command]
pub async fn hide_personal_overlay(
    state: State<'_, SharedOverlayState>,
    service: State<'_, crate::service::ServiceHandle>,
) -> Result<bool, String> {
    // Get current config and update enabled state
    let mut config = service.config().await;
    config.overlay_settings.set_enabled("personal", false);
    service.update_config(config).await?;

    // Shutdown overlay if running
    let handle = {
        let mut state = state.lock().map_err(|e| e.to_string())?;
        state.take_personal()
    };

    if let Some(handle) = handle {
        let _ = handle.tx.send(PersonalOverlayCommand::Shutdown).await;
        let _ = handle.handle.join();
    }

    Ok(true)
}

/// Refresh overlay settings for all running overlays
#[tauri::command]
pub async fn refresh_overlay_settings(
    state: State<'_, SharedOverlayState>,
    service: State<'_, crate::service::ServiceHandle>,
) -> Result<bool, String> {
    let config = service.config().await;
    let background_alpha = config.overlay_settings.background_alpha;

    // Get channels for all running overlays
    let (overlay_info, personal_tx) = {
        let state = state.lock().map_err(|e| e.to_string())?;
        let overlay_info: Vec<_> = OverlayType::all()
            .iter()
            .filter_map(|&ot| {
                state.get_tx(ot).cloned().map(|tx| (ot, tx))
            })
            .collect();
        let personal_tx = state.get_personal_tx().cloned();
        (overlay_info, personal_tx)
    };

    // Send updated config to each metric overlay
    for (overlay_type, tx) in overlay_info {
        let appearance = config.overlay_settings.get_appearance(overlay_type.config_key());
        let _ = tx.send(OverlayCommand::UpdateConfig(appearance, background_alpha)).await;
    }

    // Send updated config to personal overlay
    if let Some(tx) = personal_tx {
        let personal_config = config.overlay_settings.personal_overlay.clone();
        let _ = tx.send(PersonalOverlayCommand::UpdateConfig(personal_config, background_alpha)).await;
    }

    Ok(true)
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
        OverlayType::EDps => {
            let max = metrics.iter().map(|m| m.edps).max().unwrap_or(0);
            (metrics.iter().map(|m| (m.name.clone(), m.edps)).collect(), max)
        }
        OverlayType::Hps => {
            let max = metrics.iter().map(|m| m.hps).max().unwrap_or(0);
            (metrics.iter().map(|m| (m.name.clone(), m.hps)).collect(), max)
        }
        OverlayType::EHps => {
            let max = metrics.iter().map(|m| m.ehps).max().unwrap_or(0);
            (metrics.iter().map(|m| (m.name.clone(), m.ehps)).collect(), max)
        }
        OverlayType::Tps => {
            let max = metrics.iter().map(|m| m.tps).max().unwrap_or(0);
            (metrics.iter().map(|m| (m.name.clone(), m.tps)).collect(), max)
        }
        OverlayType::Dtps => {
            let max = metrics.iter().map(|m| m.dtps).max().unwrap_or(0);
            (metrics.iter().map(|m| (m.name.clone(), m.dtps)).collect(), max)
        }
        OverlayType::EDtps => {
            let max = metrics.iter().map(|m| m.edtps).max().unwrap_or(0);
            (metrics.iter().map(|m| (m.name.clone(), m.edtps)).collect(), max)
        }
        OverlayType::Abs => {
            let max = metrics.iter().map(|m| m.abs).max().unwrap_or(0);
            (metrics.iter().map(|m| (m.name.clone(), m.abs)).collect(), max)
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
