//! Overlay update router
//!
//! Routes service updates (metrics, effects, boss health) to the appropriate overlay threads.
//! Also handles the raid overlay's registry action channel and forwards swap/clear commands
//! back to the service registry.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use crate::overlay::{
    MetricType, OverlayCommand, OverlayManager, OverlayType, SharedOverlayState, create_all_entries,
};
use crate::service::{OverlayUpdate, ServiceHandle};
use crate::state::SharedState;
use baras_overlay::{OverlayData, RaidRegistryAction};
use tokio::sync::mpsc;

/// Spawn the overlay update router task.
///
/// Routes service updates to overlay threads. Uses select! to avoid polling.
pub fn spawn_overlay_router(
    mut rx: mpsc::Receiver<OverlayUpdate>,
    overlay_state: SharedOverlayState,
    service_handle: ServiceHandle,
    shared: Arc<SharedState>,
) {
    // Create async channel for registry actions (bridges sync overlay thread → async router)
    let (registry_tx, mut registry_rx) = mpsc::channel::<RaidRegistryAction>(32);

    // Spawn registry action bridge task
    let overlay_state_clone = overlay_state.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            // Check if raid overlay exists and has a registry channel
            // Must not hold lock across await!
            let action = overlay_state_clone.lock().ok().and_then(|state| {
                state
                    .overlays
                    .get(&OverlayType::Raid)
                    .and_then(|h| h.registry_action_rx.as_ref())
                    .and_then(|rx| rx.try_recv().ok())
            });

            if let Some(action) = action {
                let _ = registry_tx.send(action).await;
            } else {
                // No action available, sleep briefly then check again
                // This is still polling but at a much lower rate (100ms vs 50ms)
                // and only affects the registry channel, not overlay updates
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        }
    });

    // Main router loop - no timeout needed, uses select!
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::select! {
                // Wait for overlay updates
                update = rx.recv() => {
                    match update {
                        Some(update) => {
                            process_overlay_update(
                                &overlay_state,
                                &service_handle,
                                &shared,
                                update,
                            ).await;
                        }
                        None => {
                            // Channel closed
                            break;
                        }
                    }
                }
                // Wait for registry actions
                action = registry_rx.recv() => {
                    if let Some(action) = action {
                        process_registry_action(&service_handle, action).await;
                    }
                }
            }
        }
    });
}

/// Process a registry action from the raid overlay
async fn process_registry_action(service_handle: &ServiceHandle, action: RaidRegistryAction) {
    match action {
        RaidRegistryAction::SwapSlots(a, b) => {
            service_handle.swap_raid_slots(a, b).await;
        }
        RaidRegistryAction::ClearSlot(slot) => {
            service_handle.remove_raid_slot(slot).await;
        }
    }
}

/// Process a single overlay update
async fn process_overlay_update(
    overlay_state: &SharedOverlayState,
    service_handle: &ServiceHandle,
    shared: &Arc<SharedState>,
    update: OverlayUpdate,
) {
    match update {
        OverlayUpdate::DataUpdated(data) => {
            // Create entries for all metric overlay types
            let all_entries = create_all_entries(&data.metrics);

            // Get running metric overlays and their channels
            let (metric_txs, personal_tx): (Vec<_>, _) = {
                let state = match overlay_state.lock() {
                    Ok(s) => s,
                    Err(_) => return,
                };

                let metric_txs = MetricType::all()
                    .iter()
                    .filter_map(|&overlay_type| {
                        let kind = OverlayType::Metric(overlay_type);
                        state.get_tx(kind).cloned().map(|tx| (overlay_type, tx))
                    })
                    .collect();

                let personal_tx = state.get_personal_tx().cloned();

                (metric_txs, personal_tx)
            };

            // Send entries to each running metric overlay
            for (overlay_type, tx) in metric_txs {
                if let Some(entries) = all_entries.get(&overlay_type) {
                    let _ = tx
                        .send(OverlayCommand::UpdateData(OverlayData::Metrics(
                            entries.clone(),
                        )))
                        .await;
                }
            }

            // Send personal stats to personal overlay
            if let Some(tx) = personal_tx
                && let Some(stats) = data.to_personal_stats()
            {
                let _ = tx
                    .send(OverlayCommand::UpdateData(OverlayData::Personal(stats)))
                    .await;
            }

            // Send challenges data to challenges overlay
            let challenges_tx = {
                let state = match overlay_state.lock() {
                    Ok(s) => s,
                    Err(_) => return,
                };
                state.get_challenges_tx().cloned()
            };

            if let Some(tx) = challenges_tx
                && let Some(challenges) = data.challenges
            {
                let _ = tx
                    .send(OverlayCommand::UpdateData(OverlayData::Challenges(
                        challenges,
                    )))
                    .await;
            }
        }
        OverlayUpdate::EffectsUpdated(raid_data) => {
            // Send raid frame data to raid overlay
            let raid_tx = {
                let state = match overlay_state.lock() {
                    Ok(s) => s,
                    Err(_) => return,
                };
                state.get_raid_tx().cloned()
            };

            if let Some(tx) = raid_tx {
                let _ = tx
                    .send(OverlayCommand::UpdateData(OverlayData::Raid(raid_data)))
                    .await;
            }
        }
        OverlayUpdate::BossHealthUpdated(boss_data) => {
            // Send boss health data to boss health overlay
            let boss_tx = {
                let state = match overlay_state.lock() {
                    Ok(s) => s,
                    Err(_) => return,
                };
                state.get_boss_health_tx().cloned()
            };

            if let Some(tx) = boss_tx {
                let _ = tx
                    .send(OverlayCommand::UpdateData(OverlayData::BossHealth(
                        boss_data,
                    )))
                    .await;
            }
        }
        OverlayUpdate::TimersUpdated(timer_data) => {
            // Send timer data to timer overlay
            let timer_tx = {
                let state = match overlay_state.lock() {
                    Ok(s) => s,
                    Err(_) => return,
                };
                state.get_timers_tx().cloned()
            };

            if let Some(tx) = timer_tx {
                let _ = tx
                    .send(OverlayCommand::UpdateData(OverlayData::Timers(timer_data)))
                    .await;
            }
        }
        OverlayUpdate::AlertsFired(fired_alerts) => {
            // Convert FiredAlert to AlertEntry and send to alerts overlay
            use baras_overlay::AlertEntry;
            use std::time::Instant;

            let alerts_tx = {
                let state = match overlay_state.lock() {
                    Ok(s) => s,
                    Err(_) => return,
                };
                state.get_tx(OverlayType::Alerts).cloned()
            };

            if let Some(tx) = alerts_tx {
                let entries: Vec<AlertEntry> = fired_alerts
                    .into_iter()
                    .map(|a| AlertEntry {
                        text: a.text,
                        color: a.color.unwrap_or([255, 255, 255, 255]),
                        created_at: Instant::now(),
                        duration_secs: 5.0, // Default duration, could come from config
                    })
                    .collect();

                let _ = tx
                    .send(OverlayCommand::UpdateData(OverlayData::Alerts(
                        baras_overlay::AlertsData { entries },
                    )))
                    .await;
            }
        }
        OverlayUpdate::PersonalBuffsUpdated(buffs_data) => {
            let tx = {
                let state = match overlay_state.lock() {
                    Ok(s) => s,
                    Err(_) => return,
                };
                state.get_personal_buffs_tx().cloned()
            };

            if let Some(tx) = tx {
                let _ = tx
                    .send(OverlayCommand::UpdateData(OverlayData::PersonalBuffs(
                        buffs_data,
                    )))
                    .await;
            }
        }
        OverlayUpdate::PersonalDebuffsUpdated(debuffs_data) => {
            let tx = {
                let state = match overlay_state.lock() {
                    Ok(s) => s,
                    Err(_) => return,
                };
                state.get_personal_debuffs_tx().cloned()
            };

            if let Some(tx) = tx {
                let _ = tx
                    .send(OverlayCommand::UpdateData(OverlayData::PersonalDebuffs(
                        debuffs_data,
                    )))
                    .await;
            }
        }
        OverlayUpdate::CooldownsUpdated(cooldowns_data) => {
            let tx = {
                let state = match overlay_state.lock() {
                    Ok(s) => s,
                    Err(_) => return,
                };
                state.get_cooldowns_tx().cloned()
            };

            if let Some(tx) = tx {
                let _ = tx
                    .send(OverlayCommand::UpdateData(OverlayData::Cooldowns(
                        cooldowns_data,
                    )))
                    .await;
            }
        }
        OverlayUpdate::DotTrackerUpdated(dot_data) => {
            let tx = {
                let state = match overlay_state.lock() {
                    Ok(s) => s,
                    Err(_) => return,
                };
                state.get_dot_tracker_tx().cloned()
            };

            if let Some(tx) = tx {
                let _ = tx
                    .send(OverlayCommand::UpdateData(OverlayData::DotTracker(
                        dot_data,
                    )))
                    .await;
            }
        }
        OverlayUpdate::CombatStarted => {
            // Could show overlay or clear entries
        }
        OverlayUpdate::CombatEnded => {
            // Clear boss health, timer, and challenges overlays when combat ends
            let channels: Vec<_> = {
                let state = match overlay_state.lock() {
                    Ok(s) => s,
                    Err(_) => return,
                };

                let mut channels = Vec::new();

                // Boss health overlay
                if let Some(tx) = state.get_boss_health_tx() {
                    channels.push((tx.clone(), OverlayData::BossHealth(Default::default())));
                }

                // Timer overlay
                if let Some(tx) = state.get_timers_tx() {
                    channels.push((tx.clone(), OverlayData::Timers(Default::default())));
                }

                // Challenges overlay
                if let Some(tx) = state.get_challenges_tx() {
                    channels.push((tx.clone(), OverlayData::Challenges(Default::default())));
                }

                channels
            };

            for (tx, data) in channels {
                let _ = tx.send(OverlayCommand::UpdateData(data)).await;
            }
        }
        OverlayUpdate::ClearAllData => {
            // Clear all overlay data when switching files
            // Collect channels while holding lock, then release before awaiting
            use baras_overlay::RaidFrameData;

            let channels: Vec<_> = {
                let state = match overlay_state.lock() {
                    Ok(s) => s,
                    Err(_) => return,
                };

                let mut channels = Vec::new();

                // Collect metric overlay channels
                for metric_type in MetricType::all() {
                    if let Some(tx) = state.get_tx(OverlayType::Metric(*metric_type)) {
                        channels.push((tx.clone(), OverlayData::Metrics(vec![])));
                    }
                }

                // Personal overlay
                if let Some(tx) = state.get_personal_tx() {
                    channels.push((tx.clone(), OverlayData::Personal(Default::default())));
                }

                // Raid overlay
                if let Some(tx) = state.get_raid_tx() {
                    channels.push((
                        tx.clone(),
                        OverlayData::Raid(RaidFrameData { frames: vec![] }),
                    ));
                }

                // Boss health overlay
                if let Some(tx) = state.get_boss_health_tx() {
                    channels.push((tx.clone(), OverlayData::BossHealth(Default::default())));
                }

                // Timer overlay
                if let Some(tx) = state.get_timers_tx() {
                    channels.push((tx.clone(), OverlayData::Timers(Default::default())));
                }

                // Challenges overlay
                if let Some(tx) = state.get_challenges_tx() {
                    channels.push((tx.clone(), OverlayData::Challenges(Default::default())));
                }

                // Personal buffs overlay
                if let Some(tx) = state.get_personal_buffs_tx() {
                    channels.push((tx.clone(), OverlayData::PersonalBuffs(Default::default())));
                }

                // Personal debuffs overlay
                if let Some(tx) = state.get_personal_debuffs_tx() {
                    channels.push((tx.clone(), OverlayData::PersonalDebuffs(Default::default())));
                }

                // Cooldowns overlay
                if let Some(tx) = state.get_cooldowns_tx() {
                    channels.push((tx.clone(), OverlayData::Cooldowns(Default::default())));
                }

                // DOT tracker overlay
                if let Some(tx) = state.get_dot_tracker_tx() {
                    channels.push((tx.clone(), OverlayData::DotTracker(Default::default())));
                }

                channels
            }; // Lock released here

            // Now send to all channels (outside lock scope)
            for (tx, data) in channels {
                let _ = tx.send(OverlayCommand::UpdateData(data)).await;
            }
        }
        OverlayUpdate::ConversationStarted => {
            // Check if auto-hide during conversations is enabled
            let hide_enabled = shared
                .config
                .read()
                .await
                .overlay_settings
                .hide_during_conversations;
            if !hide_enabled {
                return;
            }

            // Check if overlays are currently visible and running
            let currently_visible = overlay_state
                .lock()
                .ok()
                .is_some_and(|s| s.overlays_visible && !s.overlays.is_empty());

            if currently_visible {
                // Remember that we hid them and that they were visible
                shared
                    .overlays_visible_before_conversation
                    .store(true, Ordering::SeqCst);
                shared
                    .conversation_hiding_active
                    .store(true, Ordering::SeqCst);
                let _ = OverlayManager::temporary_hide_all(overlay_state, service_handle).await;
            }
        }
        OverlayUpdate::ConversationEnded => {
            // Only restore if we were the ones who hid them
            if shared.conversation_hiding_active.load(Ordering::SeqCst) {
                shared
                    .conversation_hiding_active
                    .store(false, Ordering::SeqCst);

                if shared
                    .overlays_visible_before_conversation
                    .load(Ordering::SeqCst)
                {
                    shared
                        .overlays_visible_before_conversation
                        .store(false, Ordering::SeqCst);
                    let _ = OverlayManager::temporary_show_all(overlay_state, service_handle).await;
                }
            }
        }
    }
}
