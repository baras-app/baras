//! Overlay update router
//!
//! Routes service updates (metrics, effects, boss health) to the appropriate overlay threads.
//! Also handles the raid overlay's registry action channel and forwards swap/clear commands
//! back to the service registry.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use baras_overlay::icons::IconCache;

use crate::overlay::{
    MetricType, OverlayCommand, OverlayManager, OverlayType, SharedOverlayState, create_all_entries,
};
use crate::service::{OverlayUpdate, ServiceHandle};
use crate::state::SharedState;
use baras_overlay::{MapData, OverlayData, RaidRegistryAction};
use tauri::{Emitter, Manager};
use tokio::sync::{Semaphore, mpsc};

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
fn map_data(
    svg: Option<Arc<String>>,
    markers: Vec<baras_overlay::ResolvedMarker>,
    edit_mode: bool,
) -> MapData {
    MapData {
        svg,
        placeholder: root_placeholder_svg(edit_mode),
        markers,
    }
}

/// Cache of resolved marker asset rasters (icon / svg / png), keyed by content,
/// so each asset is rasterized/decoded once rather than on every poll.
fn marker_asset_cache() -> &'static Mutex<HashMap<String, Arc<(u32, u32, Vec<u8>)>>> {
    static C: OnceLock<Mutex<HashMap<String, Arc<(u32, u32, Vec<u8>)>>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

fn cached_asset(
    key: String,
    build: impl FnOnce() -> Option<(u32, u32, Vec<u8>)>,
) -> Option<Arc<(u32, u32, Vec<u8>)>> {
    if let Ok(cache) = marker_asset_cache().lock()
        && let Some(a) = cache.get(&key)
    {
        return Some(a.clone());
    }
    let built = Arc::new(build()?);
    if let Ok(mut cache) = marker_asset_cache().lock() {
        cache.insert(key, built.clone());
    }
    Some(built)
}

/// Read a binary map asset (e.g. PNG) by relative path: user dir then bundled.
fn read_map_bytes(rel: &Path) -> Option<Vec<u8>> {
    for base in [user_map_dir(), bundled_map_dir().map(Path::to_path_buf)]
        .into_iter()
        .flatten()
    {
        if let Ok(bytes) = std::fs::read(base.join(rel)) {
            return Some(bytes);
        }
    }
    None
}

/// Resolve core marker descriptors into paint-ready overlay markers: text is
/// filled from the sequence number; icon/svg/image refs become RGBA bytes
/// (icons via the cache, svg/png read from the map-overlays tree).
fn resolve_markers(
    markers: &[baras_core::markers::MarkerRender],
    icon_cache: Option<&Arc<IconCache>>,
) -> Vec<baras_overlay::ResolvedMarker> {
    markers
        .iter()
        .map(|m| baras_overlay::ResolvedMarker {
            x: m.x,
            y: m.y,
            elements: m
                .elements
                .iter()
                .filter_map(|el| resolve_element(el, m.number, icon_cache))
                .collect(),
        })
        .collect()
}

fn resolve_element(
    el: &baras_core::dsl::MarkerElement,
    number: u32,
    icon_cache: Option<&Arc<IconCache>>,
) -> Option<baras_overlay::ResolvedElement> {
    use baras_core::dsl::MarkerElement as E;
    match el {
        E::Text { text, color, size, offset, anchor } => Some(baras_overlay::ResolvedElement::Text {
            text: text.clone().unwrap_or_else(|| number.to_string()),
            color: *color,
            size: *size,
            offset: *offset,
            anchor: *anchor,
        }),
        E::Icon { ability_id, size, offset, anchor } => {
            let image = cached_asset(format!("icon:{ability_id}"), || {
                icon_cache?
                    .get_icon(*ability_id as u64)
                    .map(|d| (d.width, d.height, d.rgba))
            })?;
            Some(baras_overlay::ResolvedElement::Image { image, size: *size, offset: *offset, anchor: *anchor })
        }
        E::Svg { file, size, offset, anchor } => {
            let px = (size * 2.0).ceil().max(1.0) as u32;
            let image = cached_asset(format!("svg:{file}@{px}"), || {
                let svg = read_map_rel(Path::new(file))?;
                baras_overlay::rasterize_svg(svg.as_str(), px)
            })?;
            Some(baras_overlay::ResolvedElement::Image { image, size: *size, offset: *offset, anchor: *anchor })
        }
        E::Image { file, size, offset, anchor } => {
            let image = cached_asset(format!("image:{file}"), || {
                let bytes = read_map_bytes(Path::new(file))?;
                baras_overlay::icons::decode_png_rgba(&bytes)
            })?;
            Some(baras_overlay::ResolvedElement::Image { image, size: *size, offset: *offset, anchor: *anchor })
        }
    }
}

/// The full map payload for the current context (map + placeholder). Used to
/// feed the overlay on spawn and when entering edit mode.
pub fn current_map_data(edit_mode: bool) -> MapData {
    map_data(current_map_svg(), Vec::new(), edit_mode)
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
        OverlayUpdate::MapUpdated(update) => {
            // Feed the map overlay the SVG for the current encounter+phase plus
            // the active markers. Driven at 30ms (effects task), independent of
            // the metrics path. Reads disk only when the (encounter, phase) key
            // changes; the overlay ignores unchanged SVG source.
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
                ctx.encounter = update.encounter_slug.clone();
                ctx.phase = update.phase_slug.clone();
            });
            if let Some(tx) = map_tx {
                let markers = resolve_markers(&update.markers, icon_cache);
                let _ = tx
                    .send(OverlayCommand::UpdateData(OverlayData::Map(map_data(
                        svg, markers, edit_mode,
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
                    .send(OverlayCommand::UpdateData(crate::preview::with_preview(shared, OverlayData::Raid(raid_data))))
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
                    .send(OverlayCommand::UpdateData(crate::preview::with_preview(
                        shared,
                        OverlayData::BossHealth(boss_data),
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
                    .send(OverlayCommand::UpdateData(crate::preview::with_preview(shared, OverlayData::TimersA(timer_data))))
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
                    .send(OverlayCommand::UpdateData(crate::preview::with_preview(shared, OverlayData::TimersB(timer_data))))
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
                    .send(OverlayCommand::UpdateData(crate::preview::with_preview(
                        shared,
                        OverlayData::EffectsA(effects_data),
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
                    .send(OverlayCommand::UpdateData(crate::preview::with_preview(
                        shared,
                        OverlayData::EffectsB(effects_data),
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
                    .send(OverlayCommand::UpdateData(crate::preview::with_preview(
                        shared,
                        OverlayData::EffectsC(effects_data),
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
                    .send(OverlayCommand::UpdateData(crate::preview::with_preview(
                        shared,
                        OverlayData::Cooldowns(cooldowns_data),
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
                    .send(OverlayCommand::UpdateData(crate::preview::with_preview(
                        shared,
                        OverlayData::CooldownsB(cooldowns_data),
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
                    .send(OverlayCommand::UpdateData(crate::preview::with_preview(
                        shared,
                        OverlayData::DotTracker(dot_data),
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

                // Map overlay (clear the displayed SVG, keep the edit-mode placeholder)
                if let Some(tx) = state.get_tx(OverlayType::Map) {
                    channels.push((tx.clone(), OverlayData::Map(map_data(None, Vec::new(), state.move_mode))));
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
