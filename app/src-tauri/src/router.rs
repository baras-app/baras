//! Overlay update router
//!
//! Routes service updates (metrics, effects, boss health) to the appropriate overlay threads.
//! Also handles the raid overlay's registry action channel and forwards swap/clear commands
//! back to the service registry.

use std::sync::Arc;

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

            if let Some(tx) = challenges_tx {
                let challenge_data = data.challenges.unwrap_or_default();
                let _ = tx
                    .send(OverlayCommand::UpdateData(OverlayData::Challenges(
                        challenge_data,
                    )))
                    .await;
            }

            // Send combat time to combat time overlay
            let combat_time_tx = {
                let state = match overlay_state.lock() {
                    Ok(s) => s,
                    Err(_) => return,
                };
                state.get_combat_time_tx().cloned()
            };

            // Only send combat time while in combat; the CombatEnded handler
            // clears the overlay separately and we must not overwrite that clear
            // with the final DataUpdated that arrives after combat ends.
            if let Some(tx) = combat_time_tx
                && shared.in_combat.load(std::sync::atomic::Ordering::SeqCst)
            {
                let _ = tx
                    .send(OverlayCommand::UpdateData(OverlayData::CombatTime(
                        baras_overlay::CombatTimeData {
                            encounter_time_secs: data.encounter_time_secs,
                        },
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
        OverlayUpdate::TimersAUpdated(timer_data) => {
            // Send timer A data to Timers A overlay
            let timer_tx = {
                let state = match overlay_state.lock() {
                    Ok(s) => s,
                    Err(_) => return,
                };
                state.get_timers_a_tx().cloned()
            };

            if let Some(tx) = timer_tx {
                let _ = tx
                    .send(OverlayCommand::UpdateData(OverlayData::TimersA(timer_data)))
                    .await;
            }
        }
        OverlayUpdate::TimersBUpdated(timer_data) => {
            // Send timer B data to Timers B overlay
            let timer_tx = {
                let state = match overlay_state.lock() {
                    Ok(s) => s,
                    Err(_) => return,
                };
                state.get_timers_b_tx().cloned()
            };

            if let Some(tx) = timer_tx {
                let _ = tx
                    .send(OverlayCommand::UpdateData(OverlayData::TimersB(timer_data)))
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
        OverlayUpdate::EffectsAUpdated(effects_data) => {
            let tx = {
                let state = match overlay_state.lock() {
                    Ok(s) => s,
                    Err(_) => return,
                };
                state.get_effects_a_tx().cloned()
            };

            if let Some(tx) = tx {
                let _ = tx
                    .send(OverlayCommand::UpdateData(OverlayData::EffectsA(
                        effects_data,
                    )))
                    .await;
            }
        }
        OverlayUpdate::EffectsBUpdated(effects_data) => {
            let tx = {
                let state = match overlay_state.lock() {
                    Ok(s) => s,
                    Err(_) => return,
                };
                state.get_effects_b_tx().cloned()
            };

            if let Some(tx) = tx {
                let _ = tx
                    .send(OverlayCommand::UpdateData(OverlayData::EffectsB(
                        effects_data,
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
        OverlayUpdate::NotesUpdated(notes_data) => {
            let tx = {
                let state = match overlay_state.lock() {
                    Ok(s) => s,
                    Err(_) => return,
                };
                state.get_notes_tx().cloned()
            };

            if let Some(tx) = tx {
                let _ = tx
                    .send(OverlayCommand::UpdateData(OverlayData::Notes(notes_data)))
                    .await;
            }
        }
        OverlayUpdate::OperationTimerUpdated(timer_data) => {
            let tx = {
                let state = match overlay_state.lock() {
                    Ok(s) => s,
                    Err(_) => return,
                };
                state.get_operation_timer_tx().cloned()
            };

            if let Some(tx) = tx {
                let _ = tx
                    .send(OverlayCommand::UpdateData(
                        OverlayData::OperationTimer(timer_data),
                    ))
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

                // Timers A overlay
                if let Some(tx) = state.get_timers_a_tx() {
                    channels.push((tx.clone(), OverlayData::TimersA(Default::default())));
                }

                // Timers B overlay
                if let Some(tx) = state.get_timers_b_tx() {
                    channels.push((tx.clone(), OverlayData::TimersB(Default::default())));
                }

                // NOTE: Challenges overlay is NOT cleared on combat end — the finalized
                // snapshot remains visible until the next encounter starts or data is cleared.

                // Combat time overlay
                if let Some(tx) = state.get_combat_time_tx() {
                    channels.push((tx.clone(), OverlayData::CombatTime(Default::default())));
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

                // Timers A overlay
                if let Some(tx) = state.get_timers_a_tx() {
                    channels.push((tx.clone(), OverlayData::TimersA(Default::default())));
                }

                // Timers B overlay
                if let Some(tx) = state.get_timers_b_tx() {
                    channels.push((tx.clone(), OverlayData::TimersB(Default::default())));
                }

                // Challenges overlay
                if let Some(tx) = state.get_challenges_tx() {
                    channels.push((tx.clone(), OverlayData::Challenges(Default::default())));
                }

                // Effects A overlay
                if let Some(tx) = state.get_effects_a_tx() {
                    channels.push((tx.clone(), OverlayData::EffectsA(Default::default())));
                }

                // Effects B overlay
                if let Some(tx) = state.get_effects_b_tx() {
                    channels.push((tx.clone(), OverlayData::EffectsB(Default::default())));
                }

                // Cooldowns overlay
                if let Some(tx) = state.get_cooldowns_tx() {
                    channels.push((tx.clone(), OverlayData::Cooldowns(Default::default())));
                }

                // DOT tracker overlay
                if let Some(tx) = state.get_dot_tracker_tx() {
                    channels.push((tx.clone(), OverlayData::DotTracker(Default::default())));
                }

                // Notes overlay
                if let Some(tx) = state.get_notes_tx() {
                    channels.push((tx.clone(), OverlayData::Notes(Default::default())));
                }

                // Combat time overlay
                if let Some(tx) = state.get_combat_time_tx() {
                    channels.push((tx.clone(), OverlayData::CombatTime(Default::default())));
                }

                // Operation timer overlay (clear display, timer state lives in service)
                if let Some(tx) = state.get_operation_timer_tx() {
                    channels.push((
                        tx.clone(),
                        OverlayData::OperationTimer(Default::default()),
                    ));
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

            // Set the conversation flag — if we're transitioning from not-hidden
            // to hidden, actually tear down the overlay windows
            let was_hidden = shared.auto_hide.is_auto_hidden();
            shared.auto_hide.set_conversation(true);

            if !was_hidden {
                let _ = OverlayManager::temporary_hide_all(overlay_state, service_handle).await;
            }
            service_handle.emit_overlay_status_changed();
        }
        OverlayUpdate::ConversationEnded => {
            // Only act if we were actually in conversation auto-hide
            if !shared.auto_hide.is_conversation_active() {
                return;
            }

            // Clear the conversation flag — temporary_show_all checks
            // is_auto_hidden() internally, so if not-live is still active
            // overlays will stay hidden
            shared.auto_hide.set_conversation(false);
            let _ = OverlayManager::temporary_show_all(overlay_state, service_handle).await;
            service_handle.emit_overlay_status_changed();
        }
        OverlayUpdate::NotLiveStateChanged { is_live } => {
            // Always track the raw condition state so apply_not_live_auto_hide
            // knows the current state when the user toggles the setting ON
            shared.auto_hide.set_session_not_live(!is_live);

            // Check if auto-hide when not live is enabled
            let hide_enabled = shared
                .config
                .read()
                .await
                .overlay_settings
                .hide_when_not_live;
            if !hide_enabled {
                return;
            }

            if !is_live {
                // Session is no longer live — set the flag, hide if needed
                let was_hidden = shared.auto_hide.is_auto_hidden();
                shared.auto_hide.set_not_live(true);

                if !was_hidden {
                    let _ =
                        OverlayManager::temporary_hide_all(overlay_state, service_handle).await;
                }
            } else {
                // Session is live again — but verify the session is truly live
                // before restoring. This prevents a flash when resuming live tailing
                // to a stale/empty session: the is_live:true event fires from the
                // mode switch, but the underlying session is still not-live.
                if !shared.auto_hide.is_not_live_active() {
                    return;
                }
                if shared.is_session_not_live().await {
                    // Session is still effectively not-live; correct the condition
                    // flag and keep overlays hidden
                    shared.auto_hide.set_session_not_live(true);
                    return;
                }
                shared.auto_hide.set_not_live(false);
                let _ =
                    OverlayManager::temporary_show_all(overlay_state, service_handle).await;
            }
            service_handle.emit_overlay_status_changed();
        }
    }
}
