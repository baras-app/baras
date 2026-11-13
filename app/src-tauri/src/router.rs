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
use tauri::{Emitter, Manager};
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
                        process_registry_action(
                            &service_handle,
                            &overlay_state,
                            &detection_gate,
                            action,
                        )
                        .await;
                    }
                }
            }
        }
    });
}

/// Process a registry action from the raid overlay
async fn process_registry_action(
    service_handle: &ServiceHandle,
    overlay_state: &SharedOverlayState,
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
        RaidRegistryAction::ClearAll => {
            service_handle.clear_raid_registry().await;
        }
        RaidRegistryAction::CapturePermissionRequired => {
            // Off the router loop: the portal can block on a compositor dialog.
            let service_handle = service_handle.clone();
            let overlay_state = overlay_state.clone();
            tauri::async_runtime::spawn(async move {
                request_capture_permission(&service_handle, &overlay_state).await;
            });
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

/// A detect press found no capture session. A permission saved by an earlier
/// run is restored silently and the detection re-run; otherwise the main window
/// asks. Nothing touches the portal before the first press.
async fn request_capture_permission(
    service_handle: &ServiceHandle,
    overlay_state: &SharedOverlayState,
) {
    use baras_overlay::capture;

    if capture::portal_required()
        && capture::has_portal_restore_token()
        && capture::enable_portal().await.is_ok()
    {
        let raid_tx = overlay_state
            .lock()
            .ok()
            .and_then(|state| state.get_raid_tx().cloned());
        if let Some(tx) = raid_tx {
            let _ = tx.send(OverlayCommand::DetectRaidNames).await;
        }
        return;
    }

    if let Some(window) = service_handle.app_handle.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
    let _ = service_handle
        .app_handle
        .emit("screen-capture-permission-required", ());
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
        format!("name {:.2}{}", score.name_score, part("hp", score.hp_value))
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
                // A tie is about the pair; a bare margin cannot name it.
                (None, Some(best), reason) => format!(
                    "{head} | unassigned: {}; closest {} at {:.2} ({}), then {} at {:.2}",
                    reason.map_or("no reason recorded", |r| r.reason()),
                    best.name,
                    best.total.max(best.name_score),
                    support(best),
                    d.runner_up_name.as_deref().unwrap_or("nobody"),
                    d.runner_up
                ),
                (None, None, reason) => {
                    format!("{head} | unassigned: {}", reason.map_or("no candidates", |r| r.reason()))
                }
            }
        })
        .collect()
}

/// What a pass found, counted in names the text detector actually read —
/// the only thing matching can ever work with. Frames, slots and occupancy
/// are pipeline internals and stay out of the message.
///
/// No names needs no advice — the user looking at the grid and the frames
/// can see why on their own.
fn raid_detection_message(
    names: usize,
    matched: usize,
    pending: usize,
    ambiguous: usize,
) -> String {
    if names == 0 {
        // Saying the frames were kept is what makes the no-op legible:
        // without it, lingering assignments look like a bug.
        return "No names detected. Frames unchanged.".to_string();
    }
    let noun = if names == 1 { "name" } else { "names" };
    let mut message = format!("{names} {noun} detected. {matched} matched.");
    if pending > 0 {
        message.push_str(&format!(
            " {pending} pending player appearance in logs."
        ));
    }
    // The one outcome another read cannot fix.
    if ambiguous > 0 {
        message.push_str(" Some names too similar. Manual assignment recommended.");
    }
    message
}

async fn detect_raid_names(
    service_handle: &ServiceHandle,
    started_at: std::time::Instant,
    image: baras_overlay::capture::CapturedImage,
    slots: Vec<(u8, i32, i32, u32, u32)>,
    result_tx: std::sync::mpsc::Sender<String>,
) {
    let candidates = service_handle.raid_detection_candidates().await;

    // The first press ever downloads the models; without a word about it,
    // the wait reads as a hang and the failure as a broken feature.
    let downloading = !baras_raid_ocr::engine::model_is_present();
    if downloading {
        let _ = result_tx.send("Downloading OCR models (first run)...".into());
    }
    if let Err(e) = baras_raid_ocr::engine::ensure_model().await {
        tracing::warn!("Raid name detection unavailable: {e}");
        let message = if downloading {
            "Model download failed: check connection and try again"
        } else {
            "OCR unavailable: assign names manually"
        };
        let _ = result_tx.send(message.into());
        return;
    }

    let slot_count = slots.len();
    let candidate_count = candidates.len();
    let (dump_enabled, dump_max) = {
        let config = service_handle.shared.config.read().await;
        (config.ocr_debug_dump, config.ocr_debug_max_dumps)
    };

    // Log first so stalled runs still include the hardware details.
    // First suspect when readings are garbage: HiDPI and Wayland scale.
    let frame_size = slots
        .first()
        .map(|&(_, _, _, w, h)| format!("{w}x{h}"))
        .unwrap_or_else(|| "none".into());
    tracing::info!(
        target: "baras::raid_detect",
        cpu = baras_raid_ocr::cpu::summary(),
        slots = slot_count,
        candidates = candidate_count,
        capture = %format!("{}x{}", image.width, image.height),
        frame = %frame_size,
        dump = dump_enabled,
        "starting raid name detection"
    );

    let result = tokio::task::spawn_blocking(move || {
        let mut dump = dump_enabled
            .then(|| baras_raid_ocr::DebugDump::new(slot_count, dump_max))
            .flatten();
        let read_started = std::time::Instant::now();
        let mut reading = baras_raid_ocr::observe_slots_dumping(&image, &slots, dump.as_mut());
        let read = read_started.elapsed();

        // Health is read inside here, for the rows names could not settle.
        let match_started = std::time::Instant::now();
        let (assignments, decisions) =
            baras_raid_ocr::assign_with_health(&mut reading, &candidates, dump.as_mut());
        if let Some(dump) = dump {
            dump.finish();
        }
        // The tensors are freed; make the allocator give the pages back.
        baras_raid_ocr::release_memory();

        // Completes the picture the capture timing starts.
        tracing::info!(
            target: "baras::raid_detect",
            read_ms = read.as_secs_f64() * 1000.0,
            match_ms = match_started.elapsed().as_secs_f64() * 1000.0,
            rows = reading.rows.len(),
            candidates = candidate_count,
            "raid name detection"
        );

        RaidOcrResult {
            observations: reading.rows,
            assignments,
            decisions,
        }
    })
    .await;

    let result = match result {
        Ok(result) => result,
        Err(e) => {
            tracing::warn!("Raid name detection panicked: {e}");
            // Nothing decided, so the previous pulse is stale.
            service_handle.set_ambiguous_slots(Vec::new()).await;
            let _ = result_tx.send("Detection failed: assign names manually".into());
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
            "Raid name detection read nothing"
        );
        service_handle.set_ambiguous_slots(Vec::new()).await;
        // An empty pass must not wipe names an earlier pass left waiting, so
        // the registry is not touched.
        let _ = result_tx.send(raid_detection_message(0, 0, 0, 0));
        return;
    }

    // Lookalikes need a human: their slots pulse, and their occupants are
    // exempt from eviction below.
    let ambiguous: Vec<u8> = decisions
        .iter()
        .filter(|d| d.rejected == Some(baras_core::raid_detect::Rejection::Ambiguous))
        .filter_map(|d| u8::try_from(d.row).ok())
        .collect();

    // Applied even with zero matches: a pass that read names is authoritative,
    // and inside it evicts registered players it no longer saw.
    let matched = assignments.len();
    let _ = service_handle
        .apply_raid_detection(assignments, &ambiguous)
        .await;
    let (pending, _) = service_handle.apply_provisional_raid_detection(names).await;
    let elapsed_ms = started_at.elapsed().as_millis() as u64;

    tracing::info!(
        elapsed_ms,
        matched,
        names_read,
        pending,
        candidate_count,
        "Raid name detection complete"
    );

    let ambiguous_count = ambiguous.len();
    service_handle.set_ambiguous_slots(ambiguous).await;

    let _ = result_tx.send(raid_detection_message(
        names_read,
        matched,
        pending,
        ambiguous_count,
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
            let mut all_entries = create_all_entries(&data.metrics, data.player_entity_id);
            all_entries.insert(
                MetricType::IncomingDamage,
                crate::overlay::create_incoming_damage_entries(&data.incoming_damage, &data.metrics),
            );

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
                            baras_overlay::MetricData {
                                entries: entries.clone(),
                                // Warzones hide cumulative totals, but Incoming Damage's
                                // total is the round total — keep it
                                hide_totals: data.is_warzone
                                    && overlay_type != MetricType::IncomingDamage,
                            },
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
            let (challenges_tx, enemy_frames_tx) = {
                let state = match overlay_state.lock() {
                    Ok(s) => s,
                    Err(_) => return,
                };
                (
                    state.get_challenges_tx().cloned(),
                    state.get_tx(OverlayType::EnemyFrames).cloned(),
                )
            };

            if let Some(tx) = challenges_tx {
                let challenge_data = data.challenges.unwrap_or_default();
                let _ = tx
                    .send(OverlayCommand::UpdateData(OverlayData::Challenges(
                        challenge_data,
                    )))
                    .await;
            }

            // Send enemy frames to the PvP enemy HP overlay
            if let Some(tx) = enemy_frames_tx {
                let _ = tx
                    .send(OverlayCommand::UpdateData(OverlayData::EnemyFrames(
                        baras_overlay::EnemyFramesData {
                            frames: data.enemy_frames,
                        },
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
        OverlayUpdate::AbilityCast { ability_id, name } => {
            let tx = {
                let state = match overlay_state.lock() {
                    Ok(s) => s,
                    Err(_) => return,
                };
                state.get_tx(OverlayType::AbilityCast).cloned()
            };
            if let Some(tx) = tx {
                let icon = icon_cache
                    .and_then(|cache| cache.get_icon(ability_id))
                    .map(|d| std::sync::Arc::new((d.width, d.height, d.rgba)));
                let entry = baras_overlay::AbilityCastEntry {
                    ability_id,
                    name,
                    cast_at: std::time::Instant::now(),
                    icon,
                };
                let _ = tx
                    .send(OverlayCommand::UpdateData(OverlayData::AbilityCast(
                        baras_overlay::AbilityCastData { entries: vec![entry] },
                    )))
                    .await;
            }
        }
        OverlayUpdate::AbilityCastCleared => {
            let tx = {
                let state = match overlay_state.lock() {
                    Ok(s) => s,
                    Err(_) => return,
                };
                state.get_tx(OverlayType::AbilityCast).cloned()
            };
            if let Some(tx) = tx {
                let _ = tx
                    .send(OverlayCommand::UpdateData(OverlayData::AbilityCast(Default::default())))
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
        OverlayUpdate::EffectsCUpdated(effects_data) => {
            let tx = {
                let state = match overlay_state.lock() {
                    Ok(s) => s,
                    Err(_) => return,
                };
                state.get_tx(OverlayType::EffectsC).cloned()
            };

            if let Some(tx) = tx {
                let _ = tx
                    .send(OverlayCommand::UpdateData(OverlayData::EffectsC(
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
        OverlayUpdate::CooldownsBUpdated(cooldowns_data) => {
            let tx = {
                let state = match overlay_state.lock() {
                    Ok(s) => s,
                    Err(_) => return,
                };
                state.get_tx(OverlayType::CooldownsB).cloned()
            };

            if let Some(tx) = tx {
                let _ = tx
                    .send(OverlayCommand::UpdateData(OverlayData::CooldownsB(
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
            // Personal overlay shows elapsed time as its "Op Time" row
            let (timer_tx, personal_tx) = {
                let state = match overlay_state.lock() {
                    Ok(s) => s,
                    Err(_) => return,
                };
                (
                    state.get_operation_timer_tx().cloned(),
                    state.get_personal_tx().cloned(),
                )
            };

            if let Some(tx) = personal_tx {
                let _ = tx
                    .send(OverlayCommand::UpdateData(OverlayData::OperationTimer(
                        timer_data.clone(),
                    )))
                    .await;
            }
            if let Some(tx) = timer_tx {
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

                if let Some(tx) = state.get_tx(OverlayType::EnemyFrames) {
                    channels.push((tx.clone(), OverlayData::EnemyFrames(Default::default())));
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
                        channels.push((tx.clone(), OverlayData::Metrics(Default::default())));
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

                if let Some(tx) = state.get_tx(OverlayType::EnemyFrames) {
                    channels.push((tx.clone(), OverlayData::EnemyFrames(Default::default())));
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

                // Effects C overlay
                if let Some(tx) = state.get_tx(OverlayType::EffectsC) {
                    channels.push((tx.clone(), OverlayData::EffectsC(Default::default())));
                }

                // Cooldowns overlay
                if let Some(tx) = state.get_cooldowns_tx() {
                    channels.push((tx.clone(), OverlayData::Cooldowns(Default::default())));
                }

                // Cooldowns B overlay
                if let Some(tx) = state.get_tx(OverlayType::CooldownsB) {
                    channels.push((tx.clone(), OverlayData::CooldownsB(Default::default())));
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

                // Ability cast overlay
                if let Some(tx) = state.get_tx(OverlayType::AbilityCast) {
                    channels.push((tx.clone(), OverlayData::AbilityCast(Default::default())));
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
        OverlayUpdate::EnemyFramesCleared => {
            let tx = overlay_state
                .lock()
                .ok()
                .and_then(|state| state.get_tx(OverlayType::EnemyFrames).cloned());
            if let Some(tx) = tx {
                let _ = tx
                    .send(OverlayCommand::UpdateData(OverlayData::EnemyFrames(
                        Default::default(),
                    )))
                    .await;
            }
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

    /// Args: names, matched, pending, ambiguous.
    #[test]
    fn counts_are_against_names_read() {
        assert_eq!(
            raid_detection_message(7, 3, 4, 0),
            "7 names detected. 3/7 matched. 4/7 pending player appearance in logs."
        );
    }

    /// Nothing pending means nothing to wait for — the pending clause is omitted.
    #[test]
    fn a_full_match_omits_the_pending_count() {
        assert_eq!(
            raid_detection_message(4, 4, 0, 0),
            "4 names detected. 4/4 matched."
        );
    }

    /// The user looking at the grid and the frames can see why on their own,
    /// but they are told their assignments were deliberately left alone.
    #[test]
    fn an_empty_pass_says_the_frames_were_kept() {
        assert_eq!(
            raid_detection_message(0, 0, 0, 0),
            "No names detected. Frames unchanged."
        );
    }

    /// The one outcome another read cannot fix asks for a human.
    #[test]
    fn lookalikes_ask_for_manual_assignment() {
        assert_eq!(
            raid_detection_message(8, 6, 0, 2),
            "8 names detected. 6/8 matched. \
             Some names too similar. Manual assignment recommended."
        );
    }

    #[test]
    fn one_name_reads_singular() {
        assert_eq!(
            raid_detection_message(1, 0, 1, 0),
            "1 name detected. 0/1 matched. 1/1 pending player appearance in logs."
        );
    }
}
