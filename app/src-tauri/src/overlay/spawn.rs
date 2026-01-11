//! Overlay spawning and lifecycle management
//!
//! Generic spawn function and factory functions for creating overlays.
//!
//! # Important: Threading Model
//!
//! On Windows, HWND handles must be used from the thread that created them.
//! The Win32 message queue is tied to the creating thread, so SetWindowLongPtrW,
//! PeekMessageW, and other window operations fail when called from a different thread.
//!
//! To handle this, overlays are created INSIDE the spawned thread via a factory
//! function, not passed as pre-created objects.

use std::thread::{self, JoinHandle};
use tokio::sync::mpsc::{self, Sender};

use baras_core::context::{
    AlertsOverlayConfig, BossHealthConfig, ChallengeOverlayConfig, OverlayAppearanceConfig,
    OverlayPositionConfig, PersonalOverlayConfig, TimerOverlayConfig,
};
use baras_overlay::{
    AlertsOverlay, BossHealthOverlay, ChallengeOverlay, CooldownConfig, CooldownOverlay,
    DotTrackerConfig, DotTrackerOverlay, MetricOverlay, Overlay, OverlayConfig,
    PersonalBuffsConfig, PersonalBuffsOverlay, PersonalDebuffsConfig, PersonalDebuffsOverlay,
    PersonalOverlay, RaidGridLayout, RaidOverlay, RaidOverlayConfig, RaidRegistryAction,
    TimerOverlay,
};
use baras_types::{
    CooldownTrackerConfig, DotTrackerConfig as TypesDotTrackerConfig,
    PersonalBuffsConfig as TypesPersonalBuffsConfig,
    PersonalDebuffsConfig as TypesPersonalDebuffsConfig,
};

use super::state::{OverlayCommand, OverlayHandle, PositionEvent};
use super::types::{MetricType, OverlayType};

// ─────────────────────────────────────────────────────────────────────────────
// Generic Spawn Function
// ─────────────────────────────────────────────────────────────────────────────

/// Spawn an overlay using a factory function that creates it inside the thread.
///
/// This is critical for Windows where HWND must be created and used on the same thread.
/// The factory function is called inside the spawned thread, ensuring the window
/// handle's message queue is tied to the correct thread.
///
/// Returns `Err` if overlay creation fails (confirmed via channel from spawned thread).
///
/// This unified event loop handles:
/// - Command processing (move mode, data updates, config updates, position queries)
/// - Window event polling
/// - Render scheduling based on interaction state
/// - Resize corner state tracking
/// - Registry action forwarding (for raid overlay)
pub fn spawn_overlay_with_factory<O, F>(
    create_overlay: F,
    kind: OverlayType,
    registry_action_tx: Option<std::sync::mpsc::Sender<RaidRegistryAction>>,
) -> Result<(Sender<OverlayCommand>, JoinHandle<()>), String>
where
    O: Overlay,
    F: FnOnce() -> Result<O, String> + Send + 'static,
{
    let (tx, mut rx) = mpsc::channel::<OverlayCommand>(32);

    // Use a oneshot channel to get creation result back from spawned thread
    let (confirm_tx, confirm_rx) = std::sync::mpsc::channel::<Result<(), String>>();

    let handle = thread::spawn(move || {
        // Create the overlay INSIDE this thread - critical for Windows HWND threading
        let mut overlay = match create_overlay() {
            Ok(o) => {
                let _ = confirm_tx.send(Ok(()));
                o
            }
            Err(e) => {
                let _ = confirm_tx.send(Err(e));
                return;
            }
        };

        let mut needs_render = true;
        let mut was_in_resize_corner = false;
        let mut was_resizing = false;

        loop {
            // Process all pending commands
            while let Ok(cmd) = rx.try_recv() {
                match cmd {
                    OverlayCommand::SetMoveMode(enabled) => {
                        overlay.set_move_mode(enabled);
                        needs_render = true;
                    }
                    OverlayCommand::SetRearrangeMode(enabled) => {
                        overlay.set_rearrange_mode(enabled);
                        needs_render = true;
                    }
                    OverlayCommand::UpdateData(data) => {
                        if overlay.update_data(data) {
                            needs_render = true;
                        }
                    }
                    OverlayCommand::UpdateConfig(config) => {
                        overlay.update_config(config);
                        needs_render = true;
                    }
                    OverlayCommand::SetPosition(x, y) => {
                        overlay.frame_mut().window_mut().set_position(x, y);
                        needs_render = true;
                    }
                    OverlayCommand::GetPosition(response_tx) => {
                        let pos = overlay.position();
                        let current_monitor = overlay.frame().window().current_monitor();
                        let (monitor_id, monitor_x, monitor_y) = current_monitor
                            .map(|m| (Some(m.id), m.x, m.y))
                            .unwrap_or((None, 0, 0));
                        let _ = response_tx.send(PositionEvent {
                            kind,
                            x: pos.x,
                            y: pos.y,
                            width: pos.width,
                            height: pos.height,
                            monitor_id,
                            monitor_x,
                            monitor_y,
                        });
                    }
                    OverlayCommand::Shutdown => return,
                }
            }

            // Poll window events (returns false if window should close)
            if !overlay.poll_events() {
                break;
            }

            // Forward any pending registry actions to the service
            if let Some(ref tx) = registry_action_tx {
                for action in overlay.take_pending_registry_actions() {
                    let _ = tx.send(action);
                }
            }

            // Check if overlay's internal state requires a render (e.g., click handling)
            if overlay.needs_render() {
                needs_render = true;
            }

            // Check for pending resize
            if overlay.frame().window().pending_size().is_some() {
                needs_render = true;
            }

            // Clear position dirty flag (position is saved on lock, not continuously)
            let _ = overlay.take_position_dirty();

            // Check if resize corner state changed (need to show/hide grip)
            let in_resize_corner = overlay.in_resize_corner();
            let is_resizing = overlay.is_resizing();
            if in_resize_corner != was_in_resize_corner || is_resizing != was_resizing {
                needs_render = true;
                was_in_resize_corner = in_resize_corner;
                was_resizing = is_resizing;
            }

            let is_interactive = overlay.is_interactive();

            if needs_render {
                overlay.render();
                needs_render = false;
            }

            // Sleep longer when locked (no interaction), shorter when interactive
            // 100ms = 10 polls/sec when locked (smooth countdowns, visual-change detection skips redundant renders)
            // 16ms = 60 FPS when interactive (for responsive dragging)
            let sleep_ms = if is_interactive { 16 } else { 100 };
            thread::sleep(std::time::Duration::from_millis(sleep_ms));
        }
    });

    // Wait for confirmation from the spawned thread
    match confirm_rx.recv() {
        Ok(Ok(())) => Ok((tx, handle)),
        Ok(Err(e)) => Err(e),
        Err(_) => Err("Overlay thread exited before confirming creation".to_string()),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Factory Functions
// ─────────────────────────────────────────────────────────────────────────────

/// Create and spawn a metric overlay
///
/// Position is stored as relative to the saved monitor. On Wayland with layer-shell,
/// positions are used directly as margins from the output's top-left corner.
/// The target_monitor_id binds the surface to the correct output.
///
/// The overlay is created inside the spawned thread to ensure Windows HWND
/// threading requirements are satisfied.
pub fn create_metric_overlay(
    overlay_type: MetricType,
    position: OverlayPositionConfig,
    appearance: OverlayAppearanceConfig,
    background_alpha: u8,
    show_empty_bars: bool,
    stack_from_bottom: bool,
    scaling_factor: f32,
) -> Result<OverlayHandle, String> {
    // Position is already relative to the monitor - pass directly
    // On Wayland: used as layer-shell margins
    // On Windows: will be converted to absolute using monitor position
    let config = OverlayConfig {
        x: position.x,
        y: position.y,
        width: position.width,
        height: position.height,
        namespace: overlay_type.namespace().to_string(),
        click_through: true,
        target_monitor_id: position.monitor_id.clone(),
    };

    let title = overlay_type.title().to_string();
    let kind = OverlayType::Metric(overlay_type);

    // Create a factory closure that will be called inside the spawned thread
    let factory = move || {
        MetricOverlay::new(
            config,
            &title,
            appearance,
            background_alpha,
            show_empty_bars,
            stack_from_bottom,
            scaling_factor,
        )
        .map_err(|e| format!("Failed to create {} overlay: {}", title, e))
    };

    let (tx, handle) = spawn_overlay_with_factory(factory, kind, None)?;

    Ok(OverlayHandle {
        tx,
        handle,
        kind,
        registry_action_rx: None,
    })
}

/// Create and spawn the personal overlay
///
/// Position is stored as relative to the saved monitor. On Wayland with layer-shell,
/// positions are used directly as margins from the output's top-left corner.
/// The target_monitor_id binds the surface to the correct output.
///
/// The overlay is created inside the spawned thread to ensure Windows HWND
/// threading requirements are satisfied.
pub fn create_personal_overlay(
    position: OverlayPositionConfig,
    personal_config: PersonalOverlayConfig,
    background_alpha: u8,
) -> Result<OverlayHandle, String> {
    // Position is already relative to the monitor - pass directly
    let config = OverlayConfig {
        x: position.x,
        y: position.y,
        width: position.width,
        height: position.height,
        namespace: "baras-personal".to_string(),
        click_through: true,
        target_monitor_id: position.monitor_id.clone(),
    };

    let kind = OverlayType::Personal;

    // Create a factory closure that will be called inside the spawned thread
    let factory = move || {
        PersonalOverlay::new(config, personal_config, background_alpha)
            .map_err(|e| format!("Failed to create personal overlay: {}", e))
    };

    let (tx, handle) = spawn_overlay_with_factory(factory, kind, None)?;

    Ok(OverlayHandle {
        tx,
        handle,
        kind,
        registry_action_rx: None,
    })
}

/// Create and spawn the raid frames overlay (starts with empty frames)
///
/// Returns an OverlayHandle with a registry_action_rx receiver for processing
/// swap/clear actions from the overlay.
pub fn create_raid_overlay(
    position: OverlayPositionConfig,
    layout: RaidGridLayout,
    raid_config: RaidOverlayConfig,
    background_alpha: u8,
) -> Result<OverlayHandle, String> {
    let config = OverlayConfig {
        x: position.x,
        y: position.y,
        width: position.width,
        height: position.height,
        namespace: "baras-raid".to_string(),
        click_through: true,
        target_monitor_id: position.monitor_id.clone(),
    };

    let kind = OverlayType::Raid;

    // Create channel for registry actions (overlay → service)
    let (registry_tx, registry_rx) = std::sync::mpsc::channel::<RaidRegistryAction>();

    let factory = move || {
        RaidOverlay::new(config, layout, raid_config, background_alpha)
            .map_err(|e| format!("Failed to create raid overlay: {}", e))
    };

    let (tx, handle) = spawn_overlay_with_factory(factory, kind, Some(registry_tx))?;

    Ok(OverlayHandle {
        tx,
        handle,
        kind,
        registry_action_rx: Some(registry_rx),
    })
}

/// Create and spawn the boss health bar overlay
pub fn create_boss_health_overlay(
    position: OverlayPositionConfig,
    boss_config: BossHealthConfig,
    background_alpha: u8,
) -> Result<OverlayHandle, String> {
    let config = OverlayConfig {
        x: position.x,
        y: position.y,
        width: position.width,
        height: position.height,
        namespace: "baras-boss-health".to_string(),
        click_through: true,
        target_monitor_id: position.monitor_id.clone(),
    };

    let kind = OverlayType::BossHealth;

    let factory = move || {
        BossHealthOverlay::new(config, boss_config, background_alpha)
            .map_err(|e| format!("Failed to create boss health overlay: {}", e))
    };

    let (tx, handle) = spawn_overlay_with_factory(factory, kind, None)?;

    Ok(OverlayHandle {
        tx,
        handle,
        kind,
        registry_action_rx: None,
    })
}

/// Create and spawn the timer countdown overlay
pub fn create_timer_overlay(
    position: OverlayPositionConfig,
    timer_config: TimerOverlayConfig,
    background_alpha: u8,
) -> Result<OverlayHandle, String> {
    let config = OverlayConfig {
        x: position.x,
        y: position.y,
        width: position.width,
        height: position.height,
        namespace: "baras-timers".to_string(),
        click_through: true,
        target_monitor_id: position.monitor_id.clone(),
    };

    let kind = OverlayType::Timers;

    let factory = move || {
        TimerOverlay::new(config, timer_config, background_alpha)
            .map_err(|e| format!("Failed to create timer overlay: {}", e))
    };

    let (tx, handle) = spawn_overlay_with_factory(factory, kind, None)?;

    Ok(OverlayHandle {
        tx,
        handle,
        kind,
        registry_action_rx: None,
    })
}

/// Create and spawn the challenges overlay
pub fn create_challenges_overlay(
    position: OverlayPositionConfig,
    challenge_config: ChallengeOverlayConfig,
    background_alpha: u8,
) -> Result<OverlayHandle, String> {
    let config = OverlayConfig {
        x: position.x,
        y: position.y,
        width: position.width,
        height: position.height,
        namespace: "baras-challenges".to_string(),
        click_through: true,
        target_monitor_id: position.monitor_id.clone(),
    };

    let kind = OverlayType::Challenges;

    let factory = move || {
        ChallengeOverlay::new(config, challenge_config, background_alpha)
            .map_err(|e| format!("Failed to create challenges overlay: {}", e))
    };

    let (tx, handle) = spawn_overlay_with_factory(factory, kind, None)?;

    Ok(OverlayHandle {
        tx,
        handle,
        kind,
        registry_action_rx: None,
    })
}

/// Create and spawn the alerts overlay
pub fn create_alerts_overlay(
    position: OverlayPositionConfig,
    alerts_config: AlertsOverlayConfig,
    background_alpha: u8,
) -> Result<OverlayHandle, String> {
    let config = OverlayConfig {
        x: position.x,
        y: position.y,
        width: position.width,
        height: position.height,
        namespace: "baras-alerts".to_string(),
        click_through: true,
        target_monitor_id: position.monitor_id.clone(),
    };

    let kind = OverlayType::Alerts;

    let factory = move || {
        AlertsOverlay::new(config, alerts_config, background_alpha)
            .map_err(|e| format!("Failed to create alerts overlay: {}", e))
    };

    let (tx, handle) = spawn_overlay_with_factory(factory, kind, None)?;

    Ok(OverlayHandle {
        tx,
        handle,
        kind,
        registry_action_rx: None,
    })
}

/// Create and spawn the personal buffs overlay
pub fn create_personal_buffs_overlay(
    position: OverlayPositionConfig,
    buffs_config: TypesPersonalBuffsConfig,
    background_alpha: u8,
) -> Result<OverlayHandle, String> {
    let config = OverlayConfig {
        x: position.x,
        y: position.y,
        width: position.width,
        height: position.height,
        namespace: "baras-personal-buffs".to_string(),
        click_through: true,
        target_monitor_id: position.monitor_id.clone(),
    };

    let kind = OverlayType::PersonalBuffs;

    // Convert types config to overlay config
    let overlay_config = PersonalBuffsConfig {
        icon_size: buffs_config.icon_size,
        max_display: buffs_config.max_display,
        show_effect_names: buffs_config.show_effect_names,
        show_countdown: buffs_config.show_countdown,
        show_source_name: buffs_config.show_source_name,
        show_target_name: buffs_config.show_target_name,
        stack_priority: buffs_config.stack_priority,
    };

    let factory = move || {
        PersonalBuffsOverlay::new(config, overlay_config, background_alpha)
            .map_err(|e| format!("Failed to create personal buffs overlay: {}", e))
    };

    let (tx, handle) = spawn_overlay_with_factory(factory, kind, None)?;

    Ok(OverlayHandle {
        tx,
        handle,
        kind,
        registry_action_rx: None,
    })
}

/// Create and spawn the personal debuffs overlay
pub fn create_personal_debuffs_overlay(
    position: OverlayPositionConfig,
    debuffs_config: TypesPersonalDebuffsConfig,
    background_alpha: u8,
) -> Result<OverlayHandle, String> {
    let config = OverlayConfig {
        x: position.x,
        y: position.y,
        width: position.width,
        height: position.height,
        namespace: "baras-personal-debuffs".to_string(),
        click_through: true,
        target_monitor_id: position.monitor_id.clone(),
    };

    let kind = OverlayType::PersonalDebuffs;

    // Convert types config to overlay config
    let overlay_config = PersonalDebuffsConfig {
        icon_size: debuffs_config.icon_size,
        max_display: debuffs_config.max_display,
        show_effect_names: debuffs_config.show_effect_names,
        show_countdown: debuffs_config.show_countdown,
        highlight_cleansable: debuffs_config.highlight_cleansable,
        show_source_name: debuffs_config.show_source_name,
        show_target_name: debuffs_config.show_target_name,
        stack_priority: debuffs_config.stack_priority,
    };

    let factory = move || {
        PersonalDebuffsOverlay::new(config, overlay_config, background_alpha)
            .map_err(|e| format!("Failed to create personal debuffs overlay: {}", e))
    };

    let (tx, handle) = spawn_overlay_with_factory(factory, kind, None)?;

    Ok(OverlayHandle {
        tx,
        handle,
        kind,
        registry_action_rx: None,
    })
}

/// Create and spawn the cooldowns tracker overlay
pub fn create_cooldowns_overlay(
    position: OverlayPositionConfig,
    cooldowns_config: CooldownTrackerConfig,
    background_alpha: u8,
) -> Result<OverlayHandle, String> {
    let config = OverlayConfig {
        x: position.x,
        y: position.y,
        width: position.width,
        height: position.height,
        namespace: "baras-cooldowns".to_string(),
        click_through: true,
        target_monitor_id: position.monitor_id.clone(),
    };

    let kind = OverlayType::Cooldowns;

    // Convert types config to overlay config
    let overlay_config = CooldownConfig {
        icon_size: cooldowns_config.icon_size,
        max_display: cooldowns_config.max_display,
        show_ability_names: cooldowns_config.show_ability_names,
        sort_by_remaining: cooldowns_config.sort_by_remaining,
        show_source_name: cooldowns_config.show_source_name,
        show_target_name: cooldowns_config.show_target_name,
    };

    let factory = move || {
        CooldownOverlay::new(config, overlay_config, background_alpha)
            .map_err(|e| format!("Failed to create cooldowns overlay: {}", e))
    };

    let (tx, handle) = spawn_overlay_with_factory(factory, kind, None)?;

    Ok(OverlayHandle {
        tx,
        handle,
        kind,
        registry_action_rx: None,
    })
}

/// Create and spawn the DOT tracker overlay
pub fn create_dot_tracker_overlay(
    position: OverlayPositionConfig,
    dot_config: TypesDotTrackerConfig,
    background_alpha: u8,
) -> Result<OverlayHandle, String> {
    let config = OverlayConfig {
        x: position.x,
        y: position.y,
        width: position.width,
        height: position.height,
        namespace: "baras-dot-tracker".to_string(),
        click_through: true,
        target_monitor_id: position.monitor_id.clone(),
    };

    let kind = OverlayType::DotTracker;

    // Convert types config to overlay config (font_color in types not used by overlay)
    let overlay_config = DotTrackerConfig {
        max_targets: dot_config.max_targets,
        icon_size: dot_config.icon_size,
        prune_delay_secs: dot_config.prune_delay_secs,
        show_effect_names: dot_config.show_effect_names,
        show_source_name: dot_config.show_source_name,
    };

    let factory = move || {
        DotTrackerOverlay::new(config, overlay_config, background_alpha)
            .map_err(|e| format!("Failed to create DOT tracker overlay: {}", e))
    };

    let (tx, handle) = spawn_overlay_with_factory(factory, kind, None)?;

    Ok(OverlayHandle {
        tx,
        handle,
        kind,
        registry_action_rx: None,
    })
}
