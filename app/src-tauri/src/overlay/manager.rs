//! Overlay lifecycle manager
//!
//! Provides a clean interface for spawning, shutting down, and updating overlays.
//! This consolidates the duplicated logic that was scattered across commands.

use baras_core::context::{OverlayPositionConfig, OverlaySettings};
use baras_overlay::{
    platform, CooldownConfig, DotTrackerConfig, EffectsABConfig, EffectsLayout, NotesConfig,
    OverlayConfigUpdate, OverlayData, RaidGridLayout, RaidOverlayConfig,
};
use std::time::Duration;

use super::metrics::create_entries_for_type;
use super::spawn::{
    create_alerts_overlay, create_boss_health_overlay, create_challenges_overlay,
    create_combat_time_overlay, create_cooldowns_overlay, create_dot_tracker_overlay,
    create_effects_a_overlay, create_effects_b_overlay, create_metric_overlay,
    create_notes_overlay, create_operation_timer_overlay, create_personal_overlay,
    create_raid_overlay, create_timers_a_overlay, create_timers_b_overlay,
};
use super::state::{OverlayCommand, OverlayHandle, PositionEvent};
use super::types::{MetricType, OverlayType};
use super::{SharedOverlayState, get_appearance_for_type};
use crate::service::{CombatData, ServiceHandle};

/// Result of a spawn operation
pub struct SpawnResult {
    pub handle: OverlayHandle,
    pub needs_monitor_save: bool,
}

/// Overlay lifecycle manager - handles spawn, shutdown, and updates
pub struct OverlayManager;

impl OverlayManager {
    // ─────────────────────────────────────────────────────────────────────────
    // Spawn Operations
    // ─────────────────────────────────────────────────────────────────────────

    /// Spawn a single overlay of the given type.
    /// Returns the handle and whether the position needs to be saved.
    pub fn spawn(kind: OverlayType, settings: &OverlaySettings) -> Result<SpawnResult, String> {
        let position = settings.get_position(kind.config_key());
        let needs_monitor_save = position.monitor_id.is_none();

        let handle = match kind {
            OverlayType::Metric(metric_type) => {
                let appearance = get_appearance_for_type(settings, metric_type);
                create_metric_overlay(
                    metric_type,
                    position,
                    appearance,
                    settings.metric_opacity,
                    settings.metric_show_empty_bars,
                    settings.metric_stack_from_bottom,
                    settings.metric_scaling_factor,
                    settings.class_icons_enabled,
                    settings.metric_font_scale,
                    settings.metric_dynamic_background,
                    settings.metric_show_background_bar,
                )?
            }
            OverlayType::Personal => {
                let personal_config = settings.personal_overlay.clone();
                create_personal_overlay(position, personal_config, settings.personal_opacity)?
            }
            OverlayType::Raid => {
                let raid_settings = &settings.raid_overlay;
                let layout = RaidGridLayout::from_config(raid_settings);
                let raid_config: RaidOverlayConfig = raid_settings.clone().into();
                create_raid_overlay(position, layout, raid_config, settings.raid_opacity)?
            }
            OverlayType::BossHealth => {
                let boss_config = settings.boss_health.clone();
                create_boss_health_overlay(position, boss_config, settings.boss_health_opacity)?
            }
            OverlayType::TimersA => {
                let timer_config = settings.timers_a_overlay.clone();
                create_timers_a_overlay(position, timer_config, settings.timers_a_opacity)?
            }
            OverlayType::TimersB => {
                let timer_config = settings.timers_b_overlay.clone();
                create_timers_b_overlay(position, timer_config, settings.timers_b_opacity)?
            }
            OverlayType::Challenges => {
                let challenge_config = settings.challenge_overlay.clone();
                create_challenges_overlay(position, challenge_config, settings.challenge_opacity)?
            }
            OverlayType::Alerts => {
                let alerts_config = settings.alerts_overlay.clone();
                create_alerts_overlay(position, alerts_config, settings.alerts_opacity)?
            }
            OverlayType::EffectsA => {
                let buffs_config = settings.effects_a.clone();
                create_effects_a_overlay(position, buffs_config, settings.effects_a_opacity)?
            }
            OverlayType::EffectsB => {
                let debuffs_config = settings.effects_b.clone();
                create_effects_b_overlay(position, debuffs_config, settings.effects_b_opacity)?
            }
            OverlayType::Cooldowns => {
                let cooldowns_config = settings.cooldown_tracker.clone();
                create_cooldowns_overlay(
                    position,
                    cooldowns_config,
                    settings.cooldown_tracker_opacity,
                )?
            }
            OverlayType::DotTracker => {
                let dot_config = settings.dot_tracker.clone();
                create_dot_tracker_overlay(position, dot_config, settings.dot_tracker_opacity)?
            }
            OverlayType::Notes => {
                let notes_config = settings.notes_overlay.clone();
                create_notes_overlay(position, notes_config, settings.notes_opacity)?
            }
            OverlayType::CombatTime => {
                let ct_config = settings.combat_time.clone();
                create_combat_time_overlay(position, ct_config, settings.combat_time_opacity)?
            }
            OverlayType::OperationTimer => {
                let ot_config = settings.operation_timer.clone();
                create_operation_timer_overlay(
                    position,
                    ot_config,
                    settings.operation_timer_opacity,
                )?
            }
        };

        Ok(SpawnResult {
            handle,
            needs_monitor_save,
        })
    }

    /// Shutdown an overlay and return its final position for saving.
    pub async fn shutdown(handle: OverlayHandle) -> Option<PositionEvent> {
        // Request position before shutdown
        let (pos_tx, pos_rx) = tokio::sync::oneshot::channel();
        let _ = handle.tx.send(OverlayCommand::GetPosition(pos_tx)).await;
        let position = pos_rx.await.ok();

        // Send shutdown command and join on blocking thread pool (same as shutdown_no_position)
        let _ = handle.tx.send(OverlayCommand::Shutdown).await;
        tokio::task::spawn_blocking(move || {
            let _ = handle.handle.join();
        })
        .await
        .ok();

        position
    }

    /// Shutdown an overlay without getting position (for bulk operations).
    pub async fn shutdown_no_position(handle: OverlayHandle) {
        let _ = handle.tx.send(OverlayCommand::Shutdown).await;
        // Join on a blocking thread pool so we don't stall the Tokio async executor.
        // The overlay OS thread exits within one poll interval (≤100ms), but calling
        // std::thread::JoinHandle::join() directly from an async context would block
        // the worker thread for that duration — starving the router task and delaying
        // other Tauri commands, timer ticks, and overlay updates.
        tokio::task::spawn_blocking(move || {
            let _ = handle.handle.join();
        })
        .await
        .ok();
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Data Sending
    // ─────────────────────────────────────────────────────────────────────────

    /// Send initial data to a newly spawned overlay if available.
    pub async fn send_initial_data(
        kind: OverlayType,
        tx: &tokio::sync::mpsc::Sender<OverlayCommand>,
        combat_data: Option<&CombatData>,
    ) {
        let Some(data) = combat_data else { return };

        match kind {
            OverlayType::Metric(metric_type) => {
                if data.metrics.is_empty() {
                    return;
                }
                let entries = create_entries_for_type(metric_type, &data.metrics);
                let _ = tx
                    .send(OverlayCommand::UpdateData(OverlayData::Metrics(entries)))
                    .await;
            }
            OverlayType::Personal => {
                if let Some(stats) = data.to_personal_stats() {
                    let _ = tx
                        .send(OverlayCommand::UpdateData(OverlayData::Personal(stats)))
                        .await;
                }
            }
            OverlayType::Challenges => {
                if let Some(challenges) = &data.challenges {
                    let _ = tx
                        .send(OverlayCommand::UpdateData(OverlayData::Challenges(
                            challenges.clone(),
                        )))
                        .await;
                }
            }
            OverlayType::CombatTime => {
                use baras_overlay::CombatTimeData;
                let _ = tx
                    .send(OverlayCommand::UpdateData(OverlayData::CombatTime(
                        CombatTimeData {
                            encounter_time_secs: data.encounter_time_secs,
                        },
                    )))
                    .await;
            }
            OverlayType::OperationTimer => {
                // Operation timer gets data via dedicated tick task, not initial combat data
            }
            OverlayType::Raid
            | OverlayType::BossHealth
            | OverlayType::TimersA
            | OverlayType::TimersB
            | OverlayType::Alerts
            | OverlayType::EffectsA
            | OverlayType::EffectsB
            | OverlayType::Cooldowns
            | OverlayType::DotTracker
            | OverlayType::Notes => {
                // These get data via separate update channels (bridge)
            }
        }
    }

    /// Sync move mode state with overlay.
    pub async fn sync_move_mode(tx: &tokio::sync::mpsc::Sender<OverlayCommand>, move_mode: bool) {
        if move_mode {
            let _ = tx.send(OverlayCommand::SetMoveMode(true)).await;
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Position Persistence
    // ─────────────────────────────────────────────────────────────────────────

    /// Query position from overlay and convert to config-relative coordinates.
    pub async fn query_position(
        tx: &tokio::sync::mpsc::Sender<OverlayCommand>,
    ) -> Option<PositionEvent> {
        let (pos_tx, pos_rx) = tokio::sync::oneshot::channel();
        let _ = tx.send(OverlayCommand::GetPosition(pos_tx)).await;
        pos_rx.await.ok()
    }

    /// Convert a PositionEvent to a config position (relative to monitor).
    pub fn position_to_config(pos: &PositionEvent) -> OverlayPositionConfig {
        OverlayPositionConfig {
            x: pos.x - pos.monitor_x,
            y: pos.y - pos.monitor_y,
            width: pos.width,
            height: pos.height,
            monitor_id: pos.monitor_id.clone(),
        }
    }

    /// Flush all running overlay positions into config (for profile save).
    pub async fn flush_positions(
        state: &SharedOverlayState,
        config: &mut baras_core::context::AppConfig,
    ) {
        let overlays: Vec<_> = {
            let Ok(s) = state.lock() else { return };
            s.all_overlays()
                .into_iter()
                .map(|(k, tx)| (k, tx.clone()))
                .collect()
        };

        for (kind, tx) in overlays {
            if let Some(pos) = Self::query_position(&tx).await {
                config
                    .overlay_settings
                    .set_position(kind.config_key(), Self::position_to_config(&pos));
            }
        }
    }

    /// Save overlay positions to config after a delay (for newly spawned overlays).
    pub async fn save_positions_delayed(
        pending: Vec<(String, tokio::sync::mpsc::Sender<OverlayCommand>)>,
        service: &ServiceHandle,
    ) {
        if pending.is_empty() {
            return;
        }

        // Give overlays a moment to be placed by compositor
        tokio::time::sleep(Duration::from_millis(100)).await;

        let mut config = service.config().await;
        for (key, tx) in pending {
            if let Some(pos) = Self::query_position(&tx).await {
                config
                    .overlay_settings
                    .set_position(&key, Self::position_to_config(&pos));
            }
        }
        let _ = service.update_config(config).await;
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Config Updates
    // ─────────────────────────────────────────────────────────────────────────

    /// Create a config update for an overlay type.
    pub fn create_config_update(
        kind: OverlayType,
        settings: &OverlaySettings,
        european_number_format: bool,
    ) -> OverlayConfigUpdate {
        let eu = european_number_format;
        match kind {
            OverlayType::Metric(metric_type) => {
                let appearance = get_appearance_for_type(settings, metric_type);
                OverlayConfigUpdate::Metric(
                    appearance,
                    settings.metric_opacity,
                    settings.metric_show_empty_bars,
                    settings.metric_stack_from_bottom,
                    settings.metric_scaling_factor,
                    settings.class_icons_enabled,
                    settings.metric_font_scale,
                    settings.metric_dynamic_background,
                    eu,
                    settings.metric_show_background_bar,
                )
            }
            OverlayType::Personal => {
                let personal_config = settings.personal_overlay.clone();
                OverlayConfigUpdate::Personal(personal_config, settings.personal_opacity, eu)
            }
            OverlayType::Raid => {
                let raid_config: RaidOverlayConfig = settings.raid_overlay.clone().into();
                OverlayConfigUpdate::Raid(raid_config, settings.raid_opacity, eu)
            }
            OverlayType::BossHealth => {
                let boss_config = settings.boss_health.clone();
                OverlayConfigUpdate::BossHealth(boss_config, settings.boss_health_opacity, eu)
            }
            OverlayType::TimersA => {
                let timer_config = settings.timers_a_overlay.clone();
                OverlayConfigUpdate::TimersA(timer_config, settings.timers_a_opacity, eu)
            }
            OverlayType::TimersB => {
                let timer_config = settings.timers_b_overlay.clone();
                OverlayConfigUpdate::TimersB(timer_config, settings.timers_b_opacity, eu)
            }
            OverlayType::Challenges => {
                let challenge_config = settings.challenge_overlay.clone();
                OverlayConfigUpdate::Challenge(challenge_config, settings.challenge_opacity, eu)
            }
            OverlayType::Alerts => {
                let alerts_config = settings.alerts_overlay.clone();
                OverlayConfigUpdate::Alerts(alerts_config, settings.alerts_opacity, eu)
            }
            OverlayType::EffectsA => {
                let cfg = &settings.effects_a;
                let layout = if cfg.layout_vertical {
                    EffectsLayout::Vertical
                } else {
                    EffectsLayout::Horizontal
                };
                let buffs_config = EffectsABConfig {
                    icon_size: cfg.icon_size,
                    max_display: cfg.max_display,
                    layout,
                    show_effect_names: cfg.show_effect_names,
                    show_countdown: cfg.show_countdown,
                    stack_priority: cfg.stack_priority,
                    show_header: cfg.show_header,
                    header_title: "Effects A".to_string(),
                    font_scale: cfg.font_scale,
                    dynamic_background: cfg.dynamic_background,
                };
                OverlayConfigUpdate::EffectsA(buffs_config, settings.effects_a_opacity, eu)
            }
            OverlayType::EffectsB => {
                let cfg = &settings.effects_b;
                let layout = if cfg.layout_vertical {
                    EffectsLayout::Vertical
                } else {
                    EffectsLayout::Horizontal
                };
                let debuffs_config = EffectsABConfig {
                    icon_size: cfg.icon_size,
                    max_display: cfg.max_display,
                    layout,
                    show_effect_names: cfg.show_effect_names,
                    show_countdown: cfg.show_countdown,
                    stack_priority: cfg.stack_priority,
                    show_header: cfg.show_header,
                    header_title: "Effects B".to_string(),
                    font_scale: cfg.font_scale,
                    dynamic_background: cfg.dynamic_background,
                };
                OverlayConfigUpdate::EffectsB(debuffs_config, settings.effects_b_opacity, eu)
            }
            OverlayType::Cooldowns => {
                let cfg = &settings.cooldown_tracker;
                let cooldowns_config = CooldownConfig {
                    icon_size: cfg.icon_size,
                    max_display: cfg.max_display,
                    show_ability_names: cfg.show_ability_names,
                    sort_by_remaining: cfg.sort_by_remaining,
                    show_source_name: cfg.show_source_name,
                    show_target_name: cfg.show_target_name,
                    show_header: cfg.show_header,
                    font_scale: cfg.font_scale,
                    dynamic_background: cfg.dynamic_background,
                };
                OverlayConfigUpdate::Cooldowns(cooldowns_config, settings.cooldown_tracker_opacity, eu)
            }
            OverlayType::DotTracker => {
                let cfg = &settings.dot_tracker;
                let dot_config = DotTrackerConfig {
                    max_targets: cfg.max_targets,
                    icon_size: cfg.icon_size,
                    prune_delay_secs: cfg.prune_delay_secs,
                    show_effect_names: cfg.show_effect_names,
                    show_source_name: cfg.show_source_name,
                    show_header: cfg.show_header,
                    show_countdown: cfg.show_countdown,
                    font_scale: cfg.font_scale,
                    dynamic_background: cfg.dynamic_background,
                };
                OverlayConfigUpdate::DotTracker(dot_config, settings.dot_tracker_opacity, eu)
            }
            OverlayType::Notes => {
                let cfg = &settings.notes_overlay;
                let notes_config = NotesConfig {
                    font_size: cfg.font_size,
                    font_color: cfg.font_color.clone(),
                    dynamic_background: cfg.dynamic_background,
                };
                OverlayConfigUpdate::Notes(notes_config, settings.notes_opacity, eu)
            }
            OverlayType::CombatTime => {
                use baras_overlay::CombatTimeConfig;
                let cfg = &settings.combat_time;
                let ct_config = CombatTimeConfig {
                    show_title: cfg.show_title,
                    font_scale: cfg.font_scale,
                    font_color: cfg.font_color,
                    dynamic_background: cfg.dynamic_background,
                    clear_after_combat: cfg.clear_after_combat,
                };
                OverlayConfigUpdate::CombatTime(ct_config, settings.combat_time_opacity, eu)
            }
            OverlayType::OperationTimer => {
                use baras_overlay::OperationTimerConfig;
                let cfg = &settings.operation_timer;
                let ot_config = OperationTimerConfig {
                    show_title: cfg.show_title,
                    font_scale: cfg.font_scale,
                    font_color: cfg.font_color,
                    dynamic_background: cfg.dynamic_background,
                };
                OverlayConfigUpdate::OperationTimer(
                    ot_config,
                    settings.operation_timer_opacity,
                )
            }
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // High-Level Operations
    // ─────────────────────────────────────────────────────────────────────────

    /// Show a single overlay (enable + spawn if visible).
    /// Updates config and spawns overlay if global visibility is on.
    /// Respects auto-hide: if any auto-hide condition is active, the overlay is
    /// enabled in config but NOT spawned (it will appear when auto-hide clears).
    pub async fn show(
        kind: OverlayType,
        state: &SharedOverlayState,
        service: &ServiceHandle,
    ) -> Result<bool, String> {
        // Update enabled state in config
        let mut config = service.config().await;
        config.overlay_settings.set_enabled(kind.config_key(), true);
        service.update_config(config.clone()).await?;

        // Only spawn if global visibility is enabled
        if !config.overlay_settings.overlays_visible {
            return Ok(true);
        }

        // Don't spawn if auto-hide is active — overlay is enabled in config
        // and will appear when auto-hide clears via temporary_show_all()
        if service.shared.auto_hide.is_auto_hidden() {
            return Ok(true);
        }

        // Check if already running, spawn, and insert - all under lock to prevent race conditions
        // from rapid toggle clicks spawning duplicate overlays
        let (tx, needs_monitor_save, current_move_mode) = {
            let mut s = state.lock().map_err(|e| e.to_string())?;
            if s.is_running(kind) {
                return Ok(true);
            }
            // Spawn while holding lock (spawn is synchronous, so this is safe)
            let result = Self::spawn(kind, &config.overlay_settings)?;
            let tx = result.handle.tx.clone();
            let needs_monitor_save = result.needs_monitor_save;
            let mode = s.move_mode;
            s.insert(result.handle);
            (tx, needs_monitor_save, mode)
        };

        // Send config update so overlay gets current settings (e.g., european_number_format)
        let config_update =
            Self::create_config_update(kind, &config.overlay_settings, config.european_number_format);
        let _ = tx.send(OverlayCommand::UpdateConfig(config_update)).await;

        // Sync move mode
        Self::sync_move_mode(&tx, current_move_mode).await;

        // Send initial data from cache if available (regardless of tailing state)
        let combat_data = service.current_combat_data().await;
        Self::send_initial_data(kind, &tx, combat_data.as_ref()).await;

        // For Notes overlay, send current notes immediately
        if matches!(kind, OverlayType::Notes) {
            if let Some(notes_data) = service.get_current_notes().await {
                let _ = tx
                    .send(OverlayCommand::UpdateData(OverlayData::Notes(notes_data)))
                    .await;
            }
        }

        // Save position if needed
        if needs_monitor_save {
            Self::save_positions_delayed(vec![(kind.config_key().to_string(), tx)], service).await;
        }

        // Update overlay status flag for effects loop optimization
        service.set_overlay_active(kind.config_key(), true);

        Ok(true)
    }

    /// Hide a single overlay (disable + shutdown if running).
    pub async fn hide(
        kind: OverlayType,
        state: &SharedOverlayState,
        service: &ServiceHandle,
    ) -> Result<bool, String> {
        // Update enabled state in config
        let mut config = service.config().await;
        config
            .overlay_settings
            .set_enabled(kind.config_key(), false);
        service.update_config(config).await?;

        // Remove and shutdown if running
        let (handle, other_timer_running) = {
            let mut s = state.lock().map_err(|e| e.to_string())?;
            if matches!(kind, OverlayType::Raid) {
                s.rearrange_mode = false;
                service.set_rearrange_mode(false);
            }
            
            // Check if the other timer overlay is still running (for edge case handling)
            let other_timer_running = match kind {
                OverlayType::TimersA => s.is_running(OverlayType::TimersB),
                OverlayType::TimersB => s.is_running(OverlayType::TimersA),
                _ => false,
            };
            
            (s.remove(kind), other_timer_running)
        };

        if let Some(h) = handle {
            Self::shutdown_no_position(h).await;
        }

        // Update overlay status flag for effects loop optimization
        // For timer overlays, only set to false if the other timer is also not running
        if matches!(kind, OverlayType::TimersA | OverlayType::TimersB) && other_timer_running {
            // Keep timer_overlay_active true since the other timer overlay is still running
        } else {
            service.set_overlay_active(kind.config_key(), false);
        }

        Ok(true)
    }

    /// Show all enabled overlays.
    /// Always records intent (overlays_visible=true) in config, but if auto-hide
    /// is active the overlays are not actually spawned — they will appear when
    /// auto-hide clears via temporary_show_all().
    pub async fn show_all(
        state: &SharedOverlayState,
        service: &ServiceHandle,
    ) -> Result<Vec<MetricType>, String> {
        // Update visibility in config (always — this records user intent)
        let mut config = service.config().await;
        config.overlay_settings.overlays_visible = true;
        service.update_config(config.clone()).await?;

        // Update state
        {
            let mut s = state.lock().map_err(|e| e.to_string())?;
            s.overlays_visible = true;
        }

        // If auto-hide is active, don't spawn — intent is recorded, overlays
        // will appear when auto-hide clears
        if service.shared.auto_hide.is_auto_hidden() {
            service.emit_overlay_status_changed();
            return Ok(vec![]);
        }

        let enabled_keys = config.overlay_settings.enabled_types();

        // Get combat data once for all overlays (always try, regardless of tailing state)
        let combat_data = service.current_combat_data().await;

        let mut shown_metric_types = Vec::new();
        let mut needs_monitor_save = Vec::new();

        for key in &enabled_keys {
            let kind = match key.as_str() {
                "personal" => OverlayType::Personal,
                "raid" => OverlayType::Raid,
                "boss_health" => OverlayType::BossHealth,
                // Support both old "timers" and new "timers_a" keys
                "timers" | "timers_a" => OverlayType::TimersA,
                "timers_b" => OverlayType::TimersB,
                "challenges" => OverlayType::Challenges,
                "alerts" => OverlayType::Alerts,
                "effects_a" => OverlayType::EffectsA,
                "effects_b" => OverlayType::EffectsB,
                "cooldowns" => OverlayType::Cooldowns,
                "dot_tracker" => OverlayType::DotTracker,
                "notes" => OverlayType::Notes,
                "combat_time" => OverlayType::CombatTime,
                "operation_timer" => OverlayType::OperationTimer,
                _ => {
                    if let Some(mt) = MetricType::from_config_key(key) {
                        OverlayType::Metric(mt)
                    } else {
                        continue;
                    }
                }
            };

            // Check if running, spawn, and insert - all under lock to prevent race conditions
            let spawn_result = {
                let mut s = state.lock().map_err(|e| e.to_string())?;
                if s.is_running(kind) {
                    if let OverlayType::Metric(mt) = kind {
                        shown_metric_types.push(mt);
                    }
                    continue;
                }
                // Spawn while holding lock (spawn is synchronous, so this is safe)
                let Ok(result) = Self::spawn(kind, &config.overlay_settings) else {
                    continue;
                };
                let tx = result.handle.tx.clone();
                let save_monitor = result.needs_monitor_save;
                s.insert(result.handle);
                (tx, save_monitor)
            };

            // Send config update so overlay gets current settings (e.g., european_number_format)
            let config_update =
                Self::create_config_update(kind, &config.overlay_settings, config.european_number_format);
            let _ = spawn_result.0.send(OverlayCommand::UpdateConfig(config_update)).await;

            // Send initial data
            Self::send_initial_data(kind, &spawn_result.0, combat_data.as_ref()).await;

            // For Notes overlay, send current notes immediately
            if matches!(kind, OverlayType::Notes) {
                if let Some(notes_data) = service.get_current_notes().await {
                    let _ = spawn_result.0
                        .send(OverlayCommand::UpdateData(OverlayData::Notes(notes_data)))
                        .await;
                }
            }

            // Track for position saving
            if spawn_result.1 {
                needs_monitor_save.push((key.clone(), spawn_result.0));
            }

            // Update overlay status flag for effects loop optimization
            service.set_overlay_active(key, true);

            if let OverlayType::Metric(mt) = kind {
                shown_metric_types.push(mt);
            }
        }

        // Save positions for overlays that needed monitor IDs
        Self::save_positions_delayed(needs_monitor_save, service).await;

        // Notify frontend to update UI buttons
        service.emit_overlay_status_changed();

        Ok(shown_metric_types)
    }

    /// Hide all running overlays.
    pub async fn hide_all(
        state: &SharedOverlayState,
        service: &ServiceHandle,
    ) -> Result<bool, String> {
        // Update visibility in config
        let mut config = service.config().await;
        config.overlay_settings.overlays_visible = false;
        service.update_config(config).await?;

        // Drain and shutdown all overlays
        let handles = {
            let mut s = state.lock().map_err(|e| e.to_string())?;
            s.move_mode = false;
            s.overlays_visible = false;
            s.drain()
        };

        for handle in handles {
            Self::shutdown_no_position(handle).await;
        }

        // Clear all overlay status flags
        service.set_overlay_active("raid", false);
        service.set_overlay_active("boss_health", false);
        service.set_overlay_active("timers", false);
        service.set_overlay_active("effects", false);

        // Notify frontend to update UI buttons
        service.emit_overlay_status_changed();

        Ok(true)
    }

    /// Temporarily hide all overlays (does NOT persist to config).
    /// Used for auto-hide during conversations.
    pub async fn temporary_hide_all(
        state: &SharedOverlayState,
        service: &ServiceHandle,
    ) -> Result<bool, String> {
        // Drain and shutdown all overlays (no config update)
        let handles = {
            let mut s = state.lock().map_err(|e| e.to_string())?;
            s.move_mode = false;
            // DO NOT update s.overlays_visible - this is temporary
            s.drain()
        };

        for handle in handles {
            Self::shutdown_no_position(handle).await;
        }

        // Clear all overlay status flags
        service.set_overlay_active("raid", false);
        service.set_overlay_active("boss_health", false);
        service.set_overlay_active("timers", false);
        service.set_overlay_active("effects", false);

        Ok(true)
    }

    /// Restore overlays after an auto-hide condition clears (does NOT modify config).
    /// Only respawns overlays if no auto-hide condition remains active and
    /// global visibility is still enabled in config.
    pub async fn temporary_show_all(
        state: &SharedOverlayState,
        service: &ServiceHandle,
    ) -> Result<(), String> {
        let config = service.config().await;

        // Only restore if global visibility is still enabled in config
        if !config.overlay_settings.overlays_visible {
            return Ok(());
        }

        // Don't restore if another auto-hide condition is still active
        if service.shared.auto_hide.is_auto_hidden() {
            return Ok(());
        }

        let enabled_keys = config.overlay_settings.enabled_types();

        // Get combat data once for all overlays (always try, regardless of tailing state)
        let combat_data = service.current_combat_data().await;

        for key in &enabled_keys {
            let kind = match key.as_str() {
                "personal" => OverlayType::Personal,
                "raid" => OverlayType::Raid,
                "boss_health" => OverlayType::BossHealth,
                // Support both old "timers" and new "timers_a" keys
                "timers" | "timers_a" => OverlayType::TimersA,
                "timers_b" => OverlayType::TimersB,
                "challenges" => OverlayType::Challenges,
                "alerts" => OverlayType::Alerts,
                "effects_a" => OverlayType::EffectsA,
                "effects_b" => OverlayType::EffectsB,
                "cooldowns" => OverlayType::Cooldowns,
                "dot_tracker" => OverlayType::DotTracker,
                "notes" => OverlayType::Notes,
                "combat_time" => OverlayType::CombatTime,
                "operation_timer" => OverlayType::OperationTimer,
                _ => {
                    if let Some(mt) = MetricType::from_config_key(key) {
                        OverlayType::Metric(mt)
                    } else {
                        continue;
                    }
                }
            };

            // Check if running, spawn, and insert
            let spawn_result = {
                let mut s = state.lock().map_err(|e| e.to_string())?;
                if s.is_running(kind) {
                    continue;
                }
                let Ok(result) = Self::spawn(kind, &config.overlay_settings) else {
                    continue;
                };
                let tx = result.handle.tx.clone();
                s.insert(result.handle);
                tx
            };

            // Send config update so overlay gets current settings (e.g., european_number_format)
            let config_update =
                Self::create_config_update(kind, &config.overlay_settings, config.european_number_format);
            let _ = spawn_result.send(OverlayCommand::UpdateConfig(config_update)).await;

            // Send initial data
            Self::send_initial_data(kind, &spawn_result, combat_data.as_ref()).await;

            // For Notes overlay, send current notes immediately
            if matches!(kind, OverlayType::Notes) {
                if let Some(notes_data) = service.get_current_notes().await {
                    let _ = spawn_result
                        .send(OverlayCommand::UpdateData(OverlayData::Notes(notes_data)))
                        .await;
                }
            }

            // Update overlay status flag
            service.set_overlay_active(key, true);
        }

        Ok(())
    }

    /// Toggle move mode for all overlays.
    /// Returns the new move mode state.
    pub async fn toggle_move_mode(
        state: &SharedOverlayState,
        service: &ServiceHandle,
    ) -> Result<bool, String> {
        let (txs, new_mode, raid_tx, was_rearranging) = {
            let mut s = state.lock().map_err(|e| e.to_string())?;
            if !s.any_running() {
                return Err("No overlays running".to_string());
            }
            s.move_mode = !s.move_mode;
            let was_rearranging = s.rearrange_mode;
            if s.move_mode {
                s.rearrange_mode = false;
            }
            let txs: Vec<_> = s.all_txs().into_iter().cloned().collect();
            let raid_tx = s.get_raid_tx().cloned();
            (txs, s.move_mode, raid_tx, was_rearranging)
        };

        // Turn off rearrange mode first if entering move mode
        if was_rearranging && new_mode {
            service.set_rearrange_mode(false);
            if let Some(ref tx) = raid_tx {
                let _ = tx.send(OverlayCommand::SetRearrangeMode(false)).await;
            }
        }

        // Broadcast move mode to all overlays
        for tx in &txs {
            let _ = tx.send(OverlayCommand::SetMoveMode(new_mode)).await;
        }

        // When locking (move_mode = false), save all positions
        if !new_mode {
            let mut positions = Vec::new();
            for tx in &txs {
                if let Some(pos) = Self::query_position(tx).await {
                    positions.push(pos);
                }
            }

            let mut config = service.config().await;
            for pos in positions {
                config
                    .overlay_settings
                    .set_position(pos.kind.config_key(), Self::position_to_config(&pos));
            }
            service.update_config(config).await?;
        }

        // Notify frontend to update UI buttons
        service.emit_overlay_status_changed();

        Ok(new_mode)
    }

    /// Toggle raid rearrange mode.
    pub async fn toggle_rearrange(
        state: &SharedOverlayState,
        service: &ServiceHandle,
    ) -> Result<bool, String> {
        let (raid_tx, new_mode) = {
            let mut s = state.lock().map_err(|e| e.to_string())?;
            if !s.is_raid_running() {
                return Ok(false);
            }
            s.rearrange_mode = !s.rearrange_mode;
            (s.get_raid_tx().cloned(), s.rearrange_mode)
        };

        // Update shared state flag for rendering loop
        service.set_rearrange_mode(new_mode);

        if let Some(tx) = raid_tx {
            let _ = tx.send(OverlayCommand::SetRearrangeMode(new_mode)).await;
        }

        // Notify frontend to update UI buttons
        service.emit_overlay_status_changed();

        Ok(new_mode)
    }

    /// Refresh settings for all running overlays, starting/stopping overlays as needed.
    pub async fn refresh_settings(
        state: &SharedOverlayState,
        service: &ServiceHandle,
    ) -> Result<bool, String> {
        // Flush current overlay positions into config before reading it back.
        // Without this, refresh sends stale saved positions back to overlays,
        // resetting any moves the user made since the last position-save event.
        {
            let mut config = service.config().await;
            Self::flush_positions(state, &mut config).await;
            let _ = service.update_config(config).await;
        }

        let config = service.config().await;
        let settings = &config.overlay_settings;
        let globally_visible = settings.overlays_visible;

        // Handle each overlay type, collecting newly spawned overlays for initial data
        let mut newly_spawned: Vec<(OverlayType, tokio::sync::mpsc::Sender<OverlayCommand>)> = Vec::new();

        for overlay_type in Self::all_overlay_types() {
            // Raid is handled separately below (always full teardown+respawn to pick up
            // grid size changes). Skip it here to avoid a redundant spawn that the
            // special-case block would immediately tear down.
            if matches!(overlay_type, OverlayType::Raid) {
                continue;
            }

            let key = overlay_type.config_key();
            let enabled = settings.enabled.get(key).copied().unwrap_or(false);

            if !enabled {
                // Shutdown if running but disabled — remove under lock so no concurrent
                // spawner can observe it as running after we've taken the handle.
                let handle = state.lock().map_err(|e| e.to_string())?.remove(overlay_type);
                if let Some(h) = handle {
                    Self::shutdown_no_position(h).await;
                    service.set_overlay_active(key, false);
                }
            } else if enabled && globally_visible && !service.shared.auto_hide.is_auto_hidden() {
                // Spawn under lock if not already running — the is_running check and the
                // insert must be a single atomic lock scope to prevent a concurrent
                // temporary_show_all from also spawning the same overlay type, which
                // would create a duplicate ghost window on screen.
                // spawn() is synchronous so it is safe to call while holding the lock.
                let tx = {
                    let mut s = state.lock().map_err(|e| e.to_string())?;
                    if s.is_running(overlay_type) {
                        // Already running — nothing to do (temporary_show_all beat us here)
                        continue;
                    }
                    match Self::spawn(overlay_type, settings) {
                        Ok(result) => {
                            let tx = result.handle.tx.clone();
                            s.insert(result.handle);
                            tx
                        }
                        Err(_) => continue,
                    }
                };
                newly_spawned.push((overlay_type, tx));
                service.set_overlay_active(key, true);
            }
        }

        // Special case: Raid overlay always recreates to handle grid size changes
        let raid_enabled = settings.enabled.get("raid").copied().unwrap_or(false);
        let raid_was_running = {
            let mut was_running = false;
            if let Ok(mut s) = state.lock()
                && let Some(handle) = s.remove(OverlayType::Raid)
            {
                let _ = handle.tx.try_send(OverlayCommand::Shutdown);
                was_running = true;
            }
            was_running
        };

        // Brief delay after shutting down old raid overlay before spawning the new one.
        // On Wayland (Hyprland), two layer-shell surfaces with the same namespace
        // coexisting briefly causes the compositor to misposition the new surface.
        if raid_was_running {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        // Only respawn raid if it is enabled in the current profile, global visibility
        // is on, and not auto-hidden. raid_was_running is intentionally not included
        // here — if the newly loaded profile has raid disabled, tearing down the old
        // window above is correct and we must not respawn it.
        // spawn() is called inside the lock scope so the is_running check and insert
        // are atomic — prevents temporary_show_all from racing in and also spawning
        // a Raid window in the gap between spawn() and insert().
        let raid_respawned = if raid_enabled && globally_visible && !service.shared.auto_hide.is_auto_hidden()
        {
            match state.lock() {
                Ok(mut s) => match Self::spawn(OverlayType::Raid, settings) {
                    Ok(result) => {
                        s.insert(result.handle);
                        true
                    }
                    Err(_) => false,
                },
                Err(_) => false,
            }
        } else {
            false
        };

        // Update overlay status flag and send current data to newly spawned raid overlay
        if raid_respawned {
            service.set_overlay_active("raid", true);
            service.refresh_raid_frames().await;
        }

        // Update config for all running overlays
        let overlays: Vec<_> = {
            let s = state.lock().map_err(|e| e.to_string())?;
            s.all_overlays()
                .into_iter()
                .map(|(k, tx)| (k, tx.clone()))
                .collect()
        };

        // Get monitors once for position resolution
        let monitors = platform::get_all_monitors();

        for (kind, tx) in overlays {
            // Send position and size updates from profile
            if let Some(pos) = settings.positions.get(kind.config_key()) {
                // Convert relative position to absolute screen coordinates
                // Positions are stored relative to their target monitor
                let (abs_x, abs_y) = platform::resolve_absolute_position(
                    pos.x,
                    pos.y,
                    pos.monitor_id.as_deref(),
                    &monitors,
                );
                let _ = tx.send(OverlayCommand::SetPosition(abs_x, abs_y)).await;
                let _ = tx.send(OverlayCommand::SetSize(pos.width, pos.height)).await;
            }

            // Send config update
            let config_update =
                Self::create_config_update(kind, settings, config.european_number_format);
            let _ = tx.send(OverlayCommand::UpdateConfig(config_update)).await;
        }

        // Send initial data to newly spawned overlays so they don't appear empty
        if !newly_spawned.is_empty() {
            let combat_data = service.current_combat_data().await;
            for (kind, tx) in newly_spawned {
                Self::send_initial_data(kind, &tx, combat_data.as_ref()).await;

                if matches!(kind, OverlayType::Notes) {
                    if let Some(notes_data) = service.get_current_notes().await {
                        let _ = tx
                            .send(OverlayCommand::UpdateData(OverlayData::Notes(notes_data)))
                            .await;
                    }
                }
            }
        }

        Ok(true)
    }

    /// Get all overlay types for iteration.
    fn all_overlay_types() -> Vec<OverlayType> {
        let mut types = vec![
            OverlayType::Personal,
            OverlayType::Raid,
            OverlayType::BossHealth,
            OverlayType::TimersA,
            OverlayType::TimersB,
            OverlayType::Challenges,
            OverlayType::Alerts,
            OverlayType::EffectsA,
            OverlayType::EffectsB,
            OverlayType::Cooldowns,
            OverlayType::DotTracker,
            OverlayType::Notes,
            OverlayType::CombatTime,
            OverlayType::OperationTimer,
        ];
        for mt in MetricType::all() {
            types.push(OverlayType::Metric(*mt));
        }
        types
    }
}
