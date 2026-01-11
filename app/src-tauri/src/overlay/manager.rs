//! Overlay lifecycle manager
//!
//! Provides a clean interface for spawning, shutting down, and updating overlays.
//! This consolidates the duplicated logic that was scattered across commands.

use baras_core::context::{OverlayPositionConfig, OverlaySettings};
use baras_overlay::{
    CooldownConfig, DotTrackerConfig, OverlayConfigUpdate, OverlayData, PersonalBuffsConfig,
    PersonalDebuffsConfig, RaidGridLayout, RaidOverlayConfig,
};
use std::time::Duration;

use super::metrics::create_entries_for_type;
use super::spawn::{
    create_alerts_overlay, create_boss_health_overlay, create_challenges_overlay,
    create_cooldowns_overlay, create_dot_tracker_overlay, create_metric_overlay,
    create_personal_buffs_overlay, create_personal_debuffs_overlay, create_personal_overlay,
    create_raid_overlay, create_timer_overlay,
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
            OverlayType::Timers => {
                let timer_config = settings.timer_overlay.clone();
                create_timer_overlay(position, timer_config, settings.timer_opacity)?
            }
            OverlayType::Challenges => {
                let challenge_config = settings.challenge_overlay.clone();
                create_challenges_overlay(position, challenge_config, settings.challenge_opacity)?
            }
            OverlayType::Alerts => {
                let alerts_config = settings.alerts_overlay.clone();
                create_alerts_overlay(position, alerts_config, settings.alerts_opacity)?
            }
            OverlayType::PersonalBuffs => {
                let buffs_config = settings.personal_buffs.clone();
                create_personal_buffs_overlay(
                    position,
                    buffs_config,
                    settings.personal_buffs_opacity,
                )?
            }
            OverlayType::PersonalDebuffs => {
                let debuffs_config = settings.personal_debuffs.clone();
                create_personal_debuffs_overlay(
                    position,
                    debuffs_config,
                    settings.personal_debuffs_opacity,
                )?
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

        // Send shutdown command
        let _ = handle.tx.send(OverlayCommand::Shutdown).await;
        let _ = handle.handle.join();

        position
    }

    /// Shutdown an overlay without getting position (for bulk operations).
    pub async fn shutdown_no_position(handle: OverlayHandle) {
        let _ = handle.tx.send(OverlayCommand::Shutdown).await;
        let _ = handle.handle.join();
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
        if data.metrics.is_empty() {
            return;
        }

        match kind {
            OverlayType::Metric(metric_type) => {
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
            OverlayType::Raid
            | OverlayType::BossHealth
            | OverlayType::Timers
            | OverlayType::Challenges
            | OverlayType::Alerts
            | OverlayType::PersonalBuffs
            | OverlayType::PersonalDebuffs
            | OverlayType::Cooldowns
            | OverlayType::DotTracker => {
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
    ) -> OverlayConfigUpdate {
        match kind {
            OverlayType::Metric(metric_type) => {
                let appearance = get_appearance_for_type(settings, metric_type);
                OverlayConfigUpdate::Metric(
                    appearance,
                    settings.metric_opacity,
                    settings.metric_show_empty_bars,
                    settings.metric_stack_from_bottom,
                    settings.metric_scaling_factor,
                )
            }
            OverlayType::Personal => {
                let personal_config = settings.personal_overlay.clone();
                OverlayConfigUpdate::Personal(personal_config, settings.personal_opacity)
            }
            OverlayType::Raid => {
                let raid_config: RaidOverlayConfig = settings.raid_overlay.clone().into();
                OverlayConfigUpdate::Raid(raid_config, settings.raid_opacity)
            }
            OverlayType::BossHealth => {
                let boss_config = settings.boss_health.clone();
                OverlayConfigUpdate::BossHealth(boss_config, settings.boss_health_opacity)
            }
            OverlayType::Timers => {
                let timer_config = settings.timer_overlay.clone();
                OverlayConfigUpdate::Timers(timer_config, settings.timer_opacity)
            }
            OverlayType::Challenges => {
                let challenge_config = settings.challenge_overlay.clone();
                OverlayConfigUpdate::Challenge(challenge_config, settings.challenge_opacity)
            }
            OverlayType::Alerts => {
                let alerts_config = settings.alerts_overlay.clone();
                OverlayConfigUpdate::Alerts(alerts_config, settings.alerts_opacity)
            }
            OverlayType::PersonalBuffs => {
                let cfg = &settings.personal_buffs;
                let buffs_config = PersonalBuffsConfig {
                    icon_size: cfg.icon_size,
                    max_display: cfg.max_display,
                    show_effect_names: cfg.show_effect_names,
                    show_countdown: cfg.show_countdown,
                    show_source_name: cfg.show_source_name,
                    show_target_name: cfg.show_target_name,
                    stack_priority: cfg.stack_priority,
                };
                OverlayConfigUpdate::PersonalBuffs(buffs_config, settings.personal_buffs_opacity)
            }
            OverlayType::PersonalDebuffs => {
                let cfg = &settings.personal_debuffs;
                let debuffs_config = PersonalDebuffsConfig {
                    icon_size: cfg.icon_size,
                    max_display: cfg.max_display,
                    show_effect_names: cfg.show_effect_names,
                    show_countdown: cfg.show_countdown,
                    highlight_cleansable: cfg.highlight_cleansable,
                    show_source_name: cfg.show_source_name,
                    show_target_name: cfg.show_target_name,
                    stack_priority: cfg.stack_priority,
                };
                OverlayConfigUpdate::PersonalDebuffs(
                    debuffs_config,
                    settings.personal_debuffs_opacity,
                )
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
                };
                OverlayConfigUpdate::Cooldowns(cooldowns_config, settings.cooldown_tracker_opacity)
            }
            OverlayType::DotTracker => {
                let cfg = &settings.dot_tracker;
                let dot_config = DotTrackerConfig {
                    max_targets: cfg.max_targets,
                    icon_size: cfg.icon_size,
                    prune_delay_secs: cfg.prune_delay_secs,
                    show_effect_names: cfg.show_effect_names,
                    show_source_name: cfg.show_source_name,
                };
                OverlayConfigUpdate::DotTracker(dot_config, settings.dot_tracker_opacity)
            }
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // High-Level Operations
    // ─────────────────────────────────────────────────────────────────────────

    /// Show a single overlay (enable + spawn if visible).
    /// Updates config and spawns overlay if global visibility is on.
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

        // Sync move mode
        Self::sync_move_mode(&tx, current_move_mode).await;

        // Send initial data if tailing
        if service.is_tailing().await {
            let combat_data = service.current_combat_data().await;
            Self::send_initial_data(kind, &tx, combat_data.as_ref()).await;
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
        let handle = {
            let mut s = state.lock().map_err(|e| e.to_string())?;
            if matches!(kind, OverlayType::Raid) {
                s.rearrange_mode = false;
                service.set_rearrange_mode(false);
            }
            s.remove(kind)
        };

        if let Some(h) = handle {
            Self::shutdown_no_position(h).await;
        }

        // Update overlay status flag for effects loop optimization
        service.set_overlay_active(kind.config_key(), false);

        Ok(true)
    }

    /// Show all enabled overlays.
    pub async fn show_all(
        state: &SharedOverlayState,
        service: &ServiceHandle,
    ) -> Result<Vec<MetricType>, String> {
        // Update visibility in config
        let mut config = service.config().await;
        config.overlay_settings.overlays_visible = true;
        service.update_config(config.clone()).await?;

        // Update state
        {
            let mut s = state.lock().map_err(|e| e.to_string())?;
            s.overlays_visible = true;
        }

        let enabled_keys = config.overlay_settings.enabled_types();

        // Get combat data once for all overlays
        let combat_data = if service.is_tailing().await {
            service.current_combat_data().await
        } else {
            None
        };

        let mut shown_metric_types = Vec::new();
        let mut needs_monitor_save = Vec::new();

        for key in &enabled_keys {
            let kind = match key.as_str() {
                "personal" => OverlayType::Personal,
                "raid" => OverlayType::Raid,
                "boss_health" => OverlayType::BossHealth,
                "timers" => OverlayType::Timers,
                "challenges" => OverlayType::Challenges,
                "alerts" => OverlayType::Alerts,
                "personal_buffs" => OverlayType::PersonalBuffs,
                "personal_debuffs" => OverlayType::PersonalDebuffs,
                "cooldowns" => OverlayType::Cooldowns,
                "dot_tracker" => OverlayType::DotTracker,
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

            // Send initial data
            Self::send_initial_data(kind, &spawn_result.0, combat_data.as_ref()).await;

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

    /// Restore overlays after temporary hide (does NOT modify config).
    /// Only respawns overlays that are enabled in config.
    pub async fn temporary_show_all(
        state: &SharedOverlayState,
        service: &ServiceHandle,
    ) -> Result<(), String> {
        let config = service.config().await;

        // Only restore if global visibility is still enabled in config
        if !config.overlay_settings.overlays_visible {
            return Ok(());
        }

        let enabled_keys = config.overlay_settings.enabled_types();

        // Get combat data once for all overlays
        let combat_data = if service.is_tailing().await {
            service.current_combat_data().await
        } else {
            None
        };

        for key in &enabled_keys {
            let kind = match key.as_str() {
                "personal" => OverlayType::Personal,
                "raid" => OverlayType::Raid,
                "boss_health" => OverlayType::BossHealth,
                "timers" => OverlayType::Timers,
                "challenges" => OverlayType::Challenges,
                "alerts" => OverlayType::Alerts,
                "personal_buffs" => OverlayType::PersonalBuffs,
                "personal_debuffs" => OverlayType::PersonalDebuffs,
                "cooldowns" => OverlayType::Cooldowns,
                "dot_tracker" => OverlayType::DotTracker,
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

            // Send initial data
            Self::send_initial_data(kind, &spawn_result, combat_data.as_ref()).await;

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

        Ok(new_mode)
    }

    /// Refresh settings for all running overlays, starting/stopping overlays as needed.
    pub async fn refresh_settings(
        state: &SharedOverlayState,
        service: &ServiceHandle,
    ) -> Result<bool, String> {
        let config = service.config().await;
        let settings = &config.overlay_settings;

        // Handle each overlay type
        for overlay_type in Self::all_overlay_types() {
            let key = overlay_type.config_key();
            let enabled = settings.enabled.get(key).copied().unwrap_or(false);
            let running = {
                let s = state.lock().map_err(|e| e.to_string())?;
                s.is_running(overlay_type)
            };

            if running && !enabled {
                // Shutdown if running but disabled
                if let Ok(mut s) = state.lock()
                    && let Some(handle) = s.remove(overlay_type)
                {
                    let _ = handle.tx.try_send(OverlayCommand::Shutdown);
                }
            } else if !running && enabled {
                // Start if not running but enabled
                if let Ok(result) = Self::spawn(overlay_type, settings)
                    && let Ok(mut s) = state.lock()
                {
                    s.insert(result.handle);
                }
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

        if (raid_was_running || raid_enabled)
            && let Ok(result) = Self::spawn(OverlayType::Raid, settings)
            && let Ok(mut s) = state.lock()
        {
            s.insert(result.handle);
        }

        // Update config for all running overlays
        let overlays: Vec<_> = {
            let s = state.lock().map_err(|e| e.to_string())?;
            s.all_overlays()
                .into_iter()
                .map(|(k, tx)| (k, tx.clone()))
                .collect()
        };

        for (kind, tx) in overlays {
            // Send position update
            if let Some(pos) = settings.positions.get(kind.config_key()) {
                let _ = tx.send(OverlayCommand::SetPosition(pos.x, pos.y)).await;
            }

            // Send config update
            let config_update = Self::create_config_update(kind, settings);
            let _ = tx.send(OverlayCommand::UpdateConfig(config_update)).await;
        }

        Ok(true)
    }

    /// Get all overlay types for iteration.
    fn all_overlay_types() -> Vec<OverlayType> {
        let mut types = vec![
            OverlayType::Personal,
            OverlayType::Raid,
            OverlayType::BossHealth,
            OverlayType::Timers,
            OverlayType::Challenges,
            OverlayType::Alerts,
            OverlayType::PersonalBuffs,
            OverlayType::PersonalDebuffs,
            OverlayType::Cooldowns,
            OverlayType::DotTracker,
        ];
        for mt in MetricType::all() {
            types.push(OverlayType::Metric(*mt));
        }
        types
    }
}
