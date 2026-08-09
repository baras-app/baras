//! Map overlay: app-side asset resolution + display state.
//!
//! Owns everything that turns a combat `MapUpdate` into a paint-ready `MapData`
//! for the map overlay: the display context/state (which base SVG + timed-overlay
//! library + active id are current), file resolution from the `map-overlays` tree
//! (user override → bundled), the timed-overlay library pre-load, marker asset
//! resolution/caching, and the payload builders. The router just dispatches the
//! `MapUpdated` arm here; nothing about routing lives in this module.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use baras_overlay::MapData;
use baras_overlay::icons::IconCache;

use crate::service::MapUpdate;

/// The current context that selects which map SVG to show. Set from combat
/// updates (encounter + phase).
#[derive(Clone, Default, PartialEq)]
struct MapContext {
    encounter: Option<String>,
    phase: Option<String>,
    /// Core-resolved base map name ([[boss.map]]); wins over the phase-name path.
    base: Option<String>,
}

/// The resolved map: the base context/source plus the encounter's timed-overlay
/// library (pre-loaded once per encounter) and the currently-active overlay id.
struct MapState {
    ctx: MapContext,
    svg: Option<Arc<String>>,
    /// Active timed-overlay id (passed straight through to the overlay).
    overlay_id: Option<String>,
    /// The encounter's timed-overlay library, sent whole to the overlay.
    library: Option<Arc<baras_overlay::MapLibrary>>,
    /// `(encounter, ids)` the library was built for — rebuilt only on change.
    library_key: Option<(String, Vec<String>)>,
}

fn map_state() -> &'static Mutex<MapState> {
    static STATE: OnceLock<Mutex<MapState>> = OnceLock::new();
    STATE.get_or_init(|| {
        Mutex::new(MapState {
            ctx: MapContext::default(),
            svg: None,
            overlay_id: None,
            library: None,
            library_key: None,
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

/// Resolve the base SVG from the phase-name path (the fallback used when no
/// `[[boss.map]]` gives an explicit base). User files override bundled ones, and
/// a more specific file beats a less specific one:
///   1. `<encounter>/<phase>.svg`
///   2. `<encounter>/<aliased-phase>.svg` (shared map, see [`phase_map_alias`])
///   3. `<encounter>/_default.svg`
fn resolve_phase_svg(enc: &str, phase: Option<&str>) -> Option<Arc<String>> {
    let mut rels: Vec<PathBuf> = Vec::new();
    if let Some(phase) = phase {
        if safe_slug(phase) {
            rels.push(Path::new(enc).join(format!("{phase}.svg")));
            if let Some(alias) = phase_map_alias(enc, phase) {
                rels.push(Path::new(enc).join(format!("{alias}.svg")));
            }
        } else {
            tracing::warn!(phase = %phase, "map: unsafe phase slug, ignoring");
        }
    }
    rels.push(Path::new(enc).join("_default.svg"));
    tracing::debug!(encounter = %enc, ?phase, ?rels, "map: resolving phase svg");
    rels.iter().find_map(|rel| read_map_rel(rel))
}

/// Resolve the base SVG source for a context. An explicit core-resolved
/// `[[boss.map]]` name wins (`<enc>/<name>.svg`, then the encounter default);
/// otherwise the phase-name path (with alias). `None` when there's no encounter.
fn resolve_map(ctx: &MapContext) -> Option<Arc<String>> {
    let Some(enc) = ctx.encounter.as_deref() else {
        tracing::debug!(phase = ?ctx.phase, "map: no active encounter — no map");
        return None;
    };
    if !safe_slug(enc) {
        tracing::warn!(encounter = %enc, "map: unsafe encounter slug, ignoring");
        return None;
    }

    let base = match ctx.base.as_deref().filter(|s| safe_slug(s)) {
        Some(name) => read_map_rel(&Path::new(enc).join(format!("{name}.svg")))
            .or_else(|| read_map_rel(&Path::new(enc).join("_default.svg"))),
        None => resolve_phase_svg(enc, ctx.phase.as_deref()),
    };
    if base.is_none() {
        tracing::debug!(encounter = %enc, phase = ?ctx.phase, "map: no matching base svg found");
    }
    base
}

/// Build (or reuse) the encounter's timed-overlay library: read each id's SVG
/// once (user override → bundled) into an `id → source` map. Keyed by
/// `(encounter, ids)` so it rebuilds only when the encounter or its id set
/// changes — never on the per-tick map poll.
fn ensure_library(st: &mut MapState, enc: Option<&str>, ids: &[String]) {
    let key = enc.map(|e| (e.to_string(), ids.to_vec()));
    if st.library_key == key {
        return;
    }
    st.library = enc.filter(|e| safe_slug(e)).map(|enc| {
        let mut sources = HashMap::new();
        for id in ids {
            if safe_slug(id) {
                if let Some(src) = read_map_rel(&Path::new(enc).join(format!("{id}.svg"))) {
                    sources.insert(id.clone(), src);
                }
            }
        }
        tracing::debug!(encounter = %enc, count = sources.len(), "map: built overlay library");
        Arc::new(baras_overlay::MapLibrary { sources })
    });
    st.library_key = key;
}

/// Forget the cached map context (e.g. when switching log files).
pub(crate) fn reset_map_context() {
    if let Ok(mut st) = map_state().lock() {
        st.ctx = MapContext::default();
        st.svg = None;
        st.overlay_id = None;
        st.library = None;
        st.library_key = None;
    }
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

/// Build the payload sent to the map overlay: the current base map, the timed-
/// overlay library + active id, plus the edit-mode placeholder (only loaded in
/// `edit_mode`).
fn map_data(
    svg: Option<Arc<String>>,
    overlay_library: Option<Arc<baras_overlay::MapLibrary>>,
    overlay_id: Option<String>,
    markers: Vec<baras_overlay::ResolvedMarker>,
    edit_mode: bool,
) -> MapData {
    MapData {
        svg,
        overlay_library,
        overlay_id,
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
    let (svg, library, overlay_id) = match map_state().lock() {
        Ok(st) => (st.svg.clone(), st.library.clone(), st.overlay_id.clone()),
        Err(_) => (None, None, None),
    };
    map_data(svg, library, overlay_id, Vec::new(), edit_mode)
}

/// A cleared map payload (no SVG/library/markers), keeping the edit-mode
/// placeholder. Sent on ClearAllData so the overlay blanks between encounters.
pub(crate) fn cleared_map_data(edit_mode: bool) -> MapData {
    map_data(None, None, None, Vec::new(), edit_mode)
}

/// Update the persistent map state from a combat `MapUpdate` and build the
/// overlay payload. Always refreshes the state (so an overlay spawned later is
/// fed the right map); returns `None` only if the state lock is poisoned.
pub(crate) fn build_map_update(
    update: &MapUpdate,
    icon_cache: Option<&Arc<IconCache>>,
    edit_mode: bool,
) -> Option<MapData> {
    // Re-resolve the base only when its context changes; rebuild the library
    // only when the encounter or its id set changes.
    let (svg, library, overlay_id) = {
        let mut st = map_state().lock().ok()?;
        let before = st.ctx.clone();
        st.ctx.encounter = update.encounter_slug.clone();
        st.ctx.phase = update.phase_slug.clone();
        st.ctx.base = update.map_base.clone();
        if st.ctx != before {
            st.svg = resolve_map(&st.ctx);
        }
        ensure_library(&mut st, update.encounter_slug.as_deref(), &update.library_ids);
        st.overlay_id = update.map_overlay.clone();
        (st.svg.clone(), st.library.clone(), st.overlay_id.clone())
    };
    let markers = resolve_markers(&update.markers, icon_cache);
    Some(map_data(svg, library, overlay_id, markers, edit_mode))
}
