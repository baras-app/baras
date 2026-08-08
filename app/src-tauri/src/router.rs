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
use tokio::sync::{Semaphore, mpsc};

/// Spawn the overlay update router task.
///
/// Routes service updates to overlay threads. Uses select! to avoid polling.
pub fn spawn_overlay_router(
    mut rx: mpsc::Receiver<OverlayUpdate>,
    overlay_state: SharedOverlayState,
    service_handle: ServiceHandle,
    shared: Arc<SharedState>,
    icon_cache: Option<Arc<baras_overlay::icons::IconCache>>,
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
    let detection_gate = Arc::new(Semaphore::new(1));
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
                                icon_cache.as_ref(),
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
                        process_registry_action(&service_handle, &detection_gate, action).await;
                    }
                }
            }
        }
    });
}

/// Process a registry action from the raid overlay
async fn process_registry_action(
    service_handle: &ServiceHandle,
    detection_gate: &Arc<Semaphore>,
    action: RaidRegistryAction,
) {
    match action {
        RaidRegistryAction::SwapSlots(a, b) => {
            service_handle.swap_raid_slots(a, b).await;
        }
        RaidRegistryAction::ClearSlot(slot) => {
            service_handle.remove_raid_slot(slot).await;
        }
        RaidRegistryAction::DetectNames {
            started_at,
            image,
            slots,
            result_tx,
        } => {
            let Ok(permit) = detection_gate.clone().try_acquire_owned() else {
                tracing::info!("Raid name detection is already running");
                let _ = result_tx.send("Detection is already running".into());
                return;
            };
            let service_handle = service_handle.clone();
            tauri::async_runtime::spawn(async move {
                let _permit = permit;
                detect_raid_names(&service_handle, started_at, image, slots, result_tx).await;
            });
        }
    }
}

struct RaidOcrResult {
    observations: Vec<baras_core::raid_detect::RowObservation>,
    assignments: Vec<baras_core::raid_detect::RowAssignment>,
    decisions: Vec<baras_core::raid_detect::RowDecision>,
}

/// One log line per row: what was read, what it was matched to, and why.
fn raid_row_lines(
    observations: &[baras_core::raid_detect::RowObservation],
    decisions: &[baras_core::raid_detect::RowDecision],
) -> Vec<String> {
    use baras_core::raid_detect::{CandidateScore, Contribution};

    let health = |row: usize| {
        observations
            .iter()
            .find(|o| o.row == row)
            .map(|o| {
                let value = o.hp_value.map_or("-".to_string(), |v| v.to_string());
                let percent = o.hp_percent.map_or("-".to_string(), |p| format!("{p}%"));
                format!("hp={value} {percent}")
            })
            .unwrap_or_default()
    };

    // Only signals that moved the score are worth printing; a health reading
    // that disagreed contributed nothing and says nothing.
    let support = |score: &CandidateScore| {
        let part = |label: &str, c: Option<Contribution>| {
            c.map(|c| {
                format!(
                    ", {label} {:.2}{}",
                    c.score,
                    if c.counted { "" } else { " (ignored)" }
                )
            })
            .unwrap_or_default()
        };
        format!(
            "name {:.2}{}{}",
            score.name_score,
            part("hp", score.hp_value),
            part("hp%", score.hp_percent)
        )
    };

    decisions
        .iter()
        .map(|d| {
            let read = d.observed.as_deref().unwrap_or("");
            let head = format!(
                "slot {:<2} read {read:?} -> {} {}",
                d.row,
                d.normalized.as_deref().unwrap_or("(nothing)"),
                health(d.row)
            );

            match (&d.assigned, &d.best, d.rejected) {
                (Some(a), _, _) => format!(
                    "{head} | {} conf {:.2} ({}), margin {:.2}",
                    a.name,
                    a.total,
                    support(a),
                    a.total - d.runner_up
                ),
                (None, Some(best), reason) => format!(
                    "{head} | unassigned: {}; closest {} at {:.2} ({})",
                    reason.unwrap_or("no reason recorded"),
                    best.name,
                    best.total.max(best.name_score),
                    support(best)
                ),
                (None, None, reason) => {
                    format!("{head} | unassigned: {}", reason.unwrap_or("no candidates"))
                }
            }
        })
        .collect()
}

/// Progress is reported against rows, not against the roster.
///
/// The roster is every player the log has seen recently, which can legitimately
/// hold more people than there are frames — a swap mid-session leaves both. A
/// perfect read of eight frames used to print `matched 8/9` and read as a
/// failure, so the roster size is now context rather than a denominator.
fn raid_detection_message(
    slot_count: usize,
    names_read: usize,
    matched: usize,
    candidate_count: usize,
    provisional: usize,
    registered: usize,
) -> String {
    let prefix = format!("OCR {names_read}/{slot_count}");
    let retained = registered.saturating_sub(matched);
    let registry_state = match (retained, provisional) {
        (0, 0) => "assignments unchanged".to_string(),
        (retained, 0) => format!("{retained} retained"),
        (0, provisional) => format!("{provisional} provisional"),
        (retained, provisional) => format!("{retained} retained, {provisional} provisional"),
    };

    if candidate_count == 0 {
        return format!("{prefix}; no roster; {registry_state}");
    }

    if matched == names_read && provisional == 0 {
        format!("{prefix}; matched all {matched} (roster {candidate_count})")
    } else {
        format!("{prefix}; matched {matched}/{names_read} rows (roster {candidate_count}); {registry_state}")
    }
}

async fn detect_raid_names(
    service_handle: &ServiceHandle,
    started_at: std::time::Instant,
    image: baras_overlay::capture::CapturedImage,
    slots: Vec<(u8, i32, i32, u32, u32)>,
    result_tx: std::sync::mpsc::Sender<String>,
) {
    let candidates = service_handle.raid_detection_candidates().await;

    if let Err(e) = baras_raid_ocr::engine::ensure_model().await {
        tracing::warn!("Raid name detection unavailable: {e}");
        let _ = result_tx.send("OCR unavailable; assign manually".into());
        return;
    }

    let slot_count = slots.len();
    let candidate_count = candidates.len();
    let dump_enabled = service_handle
        .shared
        .config
        .read()
        .await
        .overlay_settings
        .raid_overlay
        .ocr_debug_dump;

    // Log first so stalled runs still include the hardware details.
    tracing::info!(
        target: "baras::raid_detect",
        cpu = baras_raid_ocr::cpu::summary(),
        slots = slot_count,
        candidates = candidate_count,
        "starting raid name detection"
    );

    let result = tokio::task::spawn_blocking(move || {
        let mut dump = dump_enabled
            .then(|| baras_raid_ocr::DebugDump::new(slot_count))
            .flatten();
        let read_started = std::time::Instant::now();
        let observations =
            baras_raid_ocr::observe_slots_dumping(&image, &slots, dump.as_mut());
        let read = read_started.elapsed();
        if let Some(dump) = dump {
            dump.finish();
        }

        let match_started = std::time::Instant::now();
        let (assignments, decisions) = baras_core::raid_detect::assign_rows_explained(
            &observations,
            &candidates,
            &baras_core::raid_detect::MatchConfig::default(),
        );
        // Completes the picture the capture timing starts.
        tracing::info!(
            target: "baras::raid_detect",
            read_ms = read.as_secs_f64() * 1000.0,
            match_ms = match_started.elapsed().as_secs_f64() * 1000.0,
            rows = observations.len(),
            candidates = candidate_count,
            "raid name detection"
        );

        RaidOcrResult {
            observations,
            assignments,
            decisions,
        }
    })
    .await;

    let result = match result {
        Ok(result) => result,
        Err(e) => {
            tracing::warn!("Raid name detection panicked: {e}");
            let _ = result_tx.send("Detection failed; assign manually".into());
            return;
        }
    };

    let RaidOcrResult {
        observations,
        assignments,
        decisions,
    } = result;
    for line in raid_row_lines(&observations, &decisions) {
        tracing::info!(target: "baras::raid_detect", "{line}");
    }
    let mut names = baras_raid_ocr::ocr_only_names(&observations);
    let names_read = names.len();
    names.retain(|(row, _)| !assignments.iter().any(|a| a.row == *row as usize));

    if assignments.is_empty() && names.is_empty() {
        let elapsed_ms = started_at.elapsed().as_millis() as u64;
        tracing::info!(
            elapsed_ms,
            candidate_count,
            "Raid name detection could not read any names"
        );
        // Health without names means the frames were found but the names sit
        // above the cells: the grid is placed too low or its cells too short.
        let msg = if observations
            .iter()
            .any(|o| o.hp_value.is_some() || o.hp_percent.is_some())
        {
            "Health read but no names; move the grid up so each name is inside its cell"
        } else {
            "No names read; check the raid-frame alignment"
        };
        let _ = result_tx.send(msg.into());
        return;
    }

    let matched = assignments.len();
    if matched > 0 {
        let _ = service_handle.apply_raid_detection(assignments).await;
    }
    let (provisional, registered) = service_handle.apply_provisional_raid_detection(names).await;
    let retained = registered.saturating_sub(matched);
    let elapsed_ms = started_at.elapsed().as_millis() as u64;

    tracing::info!(
        elapsed_ms,
        matched,
        names_read,
        provisional,
        retained,
        candidate_count,
        "Raid name detection complete"
    );

    let _ = result_tx.send(raid_detection_message(
        slot_count,
        names_read,
        matched,
        candidate_count,
        provisional,
        registered,
    ));
}

/// Process a single overlay update
async fn process_overlay_update(
    overlay_state: &SharedOverlayState,
    service_handle: &ServiceHandle,
    shared: &Arc<SharedState>,
    icon_cache: Option<&Arc<baras_overlay::icons::IconCache>>,
    update: OverlayUpdate,
) {
    match update {
        OverlayUpdate::DataUpdated(data) => {
            // Create entries for all metric overlay types
            let all_entries = create_all_entries(&data.metrics, data.player_entity_id);

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
            use std::sync::Arc;
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
                    .filter(|a| a.alert_text_enabled)
                    .map(|a| {
                        let icon = a.icon_ability_id.and_then(|id| {
                            icon_cache.and_then(|cache| cache.get_icon(id))
                        });
                        AlertEntry {
                            id: Some(a.id),
                            text: a.text,
                            color: a.color.unwrap_or([255, 255, 255, 255]),
                            created_at: Instant::now(),
                            duration_secs: 5.0, // Default duration, could come from config
                            remaining_secs: a.remaining_secs,
                            icon_ability_id: a.icon_ability_id,
                            icon: icon.map(|d| Arc::new((d.width, d.height, d.rgba))),
                        }
                    })
                    .collect();

                if !entries.is_empty() {
                    let _ = tx
                        .send(OverlayCommand::UpdateData(OverlayData::Alerts(
                            baras_overlay::AlertsData { entries },
                        )))
                        .await;
                }
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
        OverlayUpdate::AbilityQueueUpdated(data) => {
            // Route to ability queue overlay when it exists (wired in Phase 4)
            let tx = {
                let state = match overlay_state.lock() {
                    Ok(s) => s,
                    Err(_) => return,
                };
                state.get_tx(OverlayType::AbilityQueue).cloned()
            };
            if let Some(tx) = tx {
                let _ = tx
                    .send(OverlayCommand::UpdateData(OverlayData::AbilityQueue(data)))
                    .await;
            }
        }
        OverlayUpdate::CombatStarted => {
            // Safety fallback: combat starting lifts ALL auto-hide conditions immediately,
            // regardless of settings. If overlays were temporarily hidden (conversation or
            // not-live), force-clear those flags and restore the windows. We do NOT restore
            // if the user has globally disabled overlays — only temporary auto-hides are lifted.
            let was_hidden = shared.auto_hide.is_auto_hidden();
            shared.auto_hide.set_conversation(false);
            shared.auto_hide.set_not_live(false);
            if was_hidden {
                let _ = OverlayManager::temporary_show_all(overlay_state, service_handle).await;
                service_handle.emit_overlay_status_changed();
            }
        }
        OverlayUpdate::DetectRaidNames => {
            let raid_tx = {
                let state = match overlay_state.lock() {
                    Ok(state) => state,
                    Err(_) => return,
                };
                state.get_raid_tx().cloned()
            };
            if let Some(tx) = raid_tx {
                let _ = tx.send(OverlayCommand::DetectRaidNames).await;
            }
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

                // Ability queue overlay
                if let Some(tx) = state.get_tx(OverlayType::AbilityQueue) {
                    channels.push((tx.clone(), OverlayData::AbilityQueue(Default::default())));
                }

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

                // Ability queue overlay
                if let Some(tx) = state.get_tx(OverlayType::AbilityQueue) {
                    channels.push((tx.clone(), OverlayData::AbilityQueue(Default::default())));
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

#[cfg(test)]
mod raid_detection_message_tests {
    use super::raid_detection_message;

    #[test]
    fn reports_ocr_and_registry_results_separately() {
        assert_eq!(
            raid_detection_message(8, 8, 0, 0, 1, 7),
            "OCR 8/8; no roster; 7 retained, 1 provisional"
        );
        assert_eq!(
            raid_detection_message(8, 8, 0, 0, 0, 8),
            "OCR 8/8; no roster; 8 retained"
        );
        assert_eq!(
            raid_detection_message(8, 6, 6, 8, 0, 8),
            "OCR 6/8; matched all 6 (roster 8)"
        );
        assert_eq!(
            raid_detection_message(8, 8, 8, 8, 0, 8),
            "OCR 8/8; matched all 8 (roster 8)"
        );
        assert_eq!(
            raid_detection_message(8, 8, 6, 8, 1, 7),
            "OCR 8/8; matched 6/8 rows (roster 8); 1 retained, 1 provisional"
        );
    }

    /// The case that used to read `matched 8/9` and look like a failure: every
    /// frame matched, but the log had seen a ninth player.
    #[test]
    fn a_roster_larger_than_the_frame_count_is_not_a_failure() {
        assert_eq!(
            raid_detection_message(8, 8, 8, 9, 0, 8),
            "OCR 8/8; matched all 8 (roster 9)"
        );
    }
}
