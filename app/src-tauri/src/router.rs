//! Overlay update router
//!
//! Routes service updates (metrics, effects, boss health) to the appropriate overlay threads.
//! Also handles the raid overlay's registry action channel and forwards swap/clear commands
//! back to the service registry.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use crate::overlay::{
    MetricType, OverlayCommand, OverlayManager, OverlayType, SharedOverlayState, create_all_entries,
};
use crate::service::{OverlayUpdate, ServiceHandle};
use crate::state::SharedState;
use baras_overlay::{MapData, OverlayData, RaidRegistryAction};
use tokio::sync::mpsc;

// ─────────────────────────────────────────────────────────────────────────────
// Map overlay: encounter/phase/area → SVG file resolution
// ─────────────────────────────────────────────────────────────────────────────

/// The current context that selects which map SVG to show. Set from combat
/// updates (encounter + phase).
#[derive(Clone, Default, PartialEq)]
struct MapContext {
    encounter: Option<String>,
    phase: Option<String>,
}

/// The resolved map: the context it was resolved for plus the SVG source.
struct MapState {
    ctx: MapContext,
    svg: Option<Arc<String>>,
}

fn map_state() -> &'static Mutex<MapState> {
    static STATE: OnceLock<Mutex<MapState>> = OnceLock::new();
    STATE.get_or_init(|| {
        Mutex::new(MapState {
            ctx: MapContext::default(),
            svg: None,
        })
    })
}

/// The bundled `map-overlays` resource directory shipped with the app
/// (`definitions/map-overlays`), registered once at startup. `None` if the
/// resource isn't present (e.g. some dev runs) or resolution failed.
static BUNDLED_MAP_DIR: OnceLock<Option<PathBuf>> = OnceLock::new();

/// Register the bundled `map-overlays` resource directory. Call once at startup.
pub fn init_bundled_map_dir(dir: Option<PathBuf>) {
    let _ = BUNDLED_MAP_DIR.set(dir);
}

fn bundled_map_dir() -> Option<&'static Path> {
    BUNDLED_MAP_DIR.get().and_then(|o| o.as_deref())
}

/// The user's writable map-overlays directory: `~/.config/baras/map-overlays`.
fn user_map_dir() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join("baras").join("map-overlays"))
}

/// Read a map SVG at relative path `rel`, checking the user directory first (so
/// users can override) then the bundled resource directory. Reads fresh each
/// call so edits show up without a restart.
fn read_map_rel(rel: &Path) -> Option<Arc<String>> {
    for base in [user_map_dir(), bundled_map_dir().map(Path::to_path_buf)]
        .into_iter()
        .flatten()
    {
        let path = base.join(rel);
        match std::fs::read_to_string(&path) {
            Ok(src) => {
                tracing::debug!(path = %path.display(), bytes = src.len(), "map: loaded svg");
                return Some(Arc::new(src));
            }
            Err(e) => {
                tracing::debug!(path = %path.display(), error = %e, "map: candidate not usable");
            }
        }
    }
    None
}

/// A slug is safe to use as a single path component only if it can't escape the
/// map-overlays root. Any characters are allowed in file names *except* the ones
/// that would traverse directories.
fn safe_slug(s: &str) -> bool {
    !s.is_empty() && s != "." && s != ".." && !s.contains(['/', '\\'])
}

/// Phases that share another phase's map file because they play out in the same
/// physical location. Maps `(encounter, phase)` → the phase whose SVG to reuse,
/// so several fight phases can show one map without duplicating the file. A real
/// `<encounter>/<phase>.svg` still takes precedence when present.
fn phase_map_alias(encounter: &str, phase: &str) -> Option<&'static str> {
    match (encounter, phase) {
        // Revan: Resonance transition, Floor 3, and the Machine Core burn are all
        // the same room, so they share the resonance map.
        ("revan", "revan_p4") | ("revan", "revan_core") => Some("revan_resonance"),
        _ => None,
    }
}

/// Resolve the SVG source for a context. User files override bundled ones, and a
/// more specific file beats a less specific one:
///   1. `<encounter>/<phase>.svg`
///   2. `<encounter>/<aliased-phase>.svg` (shared map, see [`phase_map_alias`])
///   3. `<encounter>/_default.svg`
/// Returns `None` when there is no active encounter or no matching file.
fn resolve_map_svg(ctx: &MapContext) -> Option<Arc<String>> {
    let Some(enc) = ctx.encounter.as_deref() else {
        tracing::debug!(phase = ?ctx.phase, "map: no active encounter — no map");
        return None;
    };
    if !safe_slug(enc) {
        tracing::warn!(encounter = %enc, "map: unsafe encounter slug, ignoring");
        return None;
    }

    let mut rels: Vec<PathBuf> = Vec::new();
    if let Some(phase) = ctx.phase.as_deref() {
        if safe_slug(phase) {
            rels.push(Path::new(enc).join(format!("{phase}.svg")));
            // A phase that shares another phase's map falls back to it before
            // the encounter-wide default.
            if let Some(alias) = phase_map_alias(enc, phase) {
                rels.push(Path::new(enc).join(format!("{alias}.svg")));
            }
        } else {
            tracing::warn!(phase = %phase, "map: unsafe phase slug, ignoring");
        }
    }
    rels.push(Path::new(enc).join("_default.svg"));

    tracing::debug!(encounter = %enc, phase = ?ctx.phase, ?rels, "map: resolving svg");

    let found = rels.iter().find_map(|rel| read_map_rel(rel));
    if found.is_none() {
        tracing::debug!("map: no matching svg found");
    }
    found
}

/// Apply `update` to the current map context, re-reading from disk only when the
/// context actually changed, and return the SVG to display now.
fn update_map_context(update: impl FnOnce(&mut MapContext)) -> Option<Arc<String>> {
    let mut st = match map_state().lock() {
        Ok(s) => s,
        Err(_) => return None,
    };
    let before = st.ctx.clone();
    update(&mut st.ctx);
    if st.ctx != before {
        tracing::debug!(
            encounter = ?st.ctx.encounter,
            phase = ?st.ctx.phase,
            "map: encounter/phase changed, re-resolving"
        );
        st.svg = resolve_map_svg(&st.ctx);
    }
    st.svg.clone()
}

/// Forget the cached map context (e.g. when switching log files).
fn reset_map_context() {
    if let Ok(mut st) = map_state().lock() {
        st.ctx = MapContext::default();
        st.svg = None;
    }
}

/// The SVG for the current encounter, if any. Used to feed a map overlay its
/// correct map the moment it spawns (rather than waiting for the next update).
pub fn current_map_svg() -> Option<Arc<String>> {
    map_state().lock().ok().and_then(|st| st.svg.clone())
}

/// Load the root-level placeholder shown in edit mode when no specific map
/// applies: `map-overlays/_default.svg` (user override first, then the bundled
/// default shipped with the app). Only reads when `edit_mode` is true, and reads
/// fresh every time — so edits show up on the next switch into edit mode.
fn root_placeholder_svg(edit_mode: bool) -> Option<Arc<String>> {
    if !edit_mode {
        return None;
    }
    read_map_rel(Path::new("_default.svg"))
}

/// Build the payload sent to the map overlay: the current map plus the
/// edit-mode placeholder (only loaded when `edit_mode` is true).
fn map_data(svg: Option<Arc<String>>, edit_mode: bool) -> MapData {
    MapData {
        svg,
        placeholder: root_placeholder_svg(edit_mode),
    }
}

/// The full map payload for the current context (map + placeholder). Used to
/// feed the overlay on spawn and when entering edit mode.
pub fn current_map_data(edit_mode: bool) -> MapData {
    map_data(current_map_svg(), edit_mode)
}

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

            // Feed the map overlay the SVG for the current encounter+phase.
            // Reads disk only when the (encounter, phase) key changes; the
            // overlay itself ignores unchanged SVG source.
            let (map_tx, edit_mode) = {
                let state = match overlay_state.lock() {
                    Ok(s) => s,
                    Err(_) => return,
                };
                (state.get_tx(OverlayType::Map).cloned(), state.move_mode)
            };
            // Always keep the map context current, even if no map overlay is
            // running yet (so it can be fed the right map the moment it spawns).
            let svg = update_map_context(|ctx| {
                ctx.encounter = data.encounter_slug.clone();
                ctx.phase = data.phase_slug.clone();
            });
            if let Some(tx) = map_tx {
                let _ = tx
                    .send(OverlayCommand::UpdateData(OverlayData::Map(map_data(
                        svg, edit_mode,
                    ))))
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

                // Map overlay (clear the displayed SVG, keep the edit-mode placeholder)
                if let Some(tx) = state.get_tx(OverlayType::Map) {
                    channels.push((tx.clone(), OverlayData::Map(map_data(None, state.move_mode))));
                }

                channels
            }; // Lock released here

            // Forget the cached map so the next encounter/area re-reads from disk.
            reset_map_context();

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
