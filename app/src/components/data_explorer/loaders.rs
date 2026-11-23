//! Session-level data loading: encounter list, session events, timeline and overview.

use dioxus::prelude::*;
use wasm_bindgen::prelude::*;

use super::ctx::{ExplorerCtx, LoadState};
use super::detail_loaders::{
    apply_detail, fetch_detail, use_ability_reload, use_detailed_loader, use_dt_summary, use_live_loader,
};
use crate::api::{self, TimeRange};
use crate::types::ViewMode;

/// All of the panel's loading effects. They react to selection changes whichever
/// view is shown, so this runs from `DataExplorerPanel` itself; the call order is
/// the effects' registration order.
pub(super) fn use_loaders(ctx: ExplorerCtx) {
    use_local_player_name(ctx);
    use_recap_reset(ctx);
    use_encounter_list(ctx);
    use_session_events(ctx);
    use_parsely_refresh(ctx);
    use_ttk_ratings(ctx);
    use_live_tick(ctx);
    use_timeline_loader(ctx);
    use_overview_loader(ctx);
    use_live_loader(ctx);
    use_detailed_loader(ctx);
    use_ability_reload(ctx);
    use_dt_summary(ctx);
}

// Local player name (fetched once on mount for auto-selection)
fn use_local_player_name(ctx: ExplorerCtx) {
    let mut local_player_name = ctx.local_player_name;
    use_future(move || async move {
        if let Some(info) = api::get_session_info().await {
            let _ = local_player_name.try_write().map(|mut w| *w = info.player_name);
        }
    });
}

// Kill time percentiles for the sidebar: asked again only when the number of
// kills changes, so they sit beside every kill instead of following the view
fn use_ttk_ratings(ctx: ExplorerCtx) {
    let ExplorerCtx {
        encounters,
        state,
        mut ttk_ratings,
        ..
    } = ctx;
    let kills = use_memo(move || encounters.read().iter().filter(|e| e.success).count());
    let enabled = use_memo(move || state.read().parsely_percentiles);
    use_effect(move || {
        let wanted = enabled() && kills() > 0;
        spawn(async move {
            let ratings = if wanted {
                api::query_parsely_ttk_ratings().await
            } else {
                Default::default()
            };
            let _ = ttk_ratings.try_write().map(|mut w| *w = ratings);
        });
    });
}

fn use_recap_reset(ctx: ExplorerCtx) {
    let ExplorerCtx {
        selected_encounter,
        mut recap_death,
        ..
    } = ctx;
    // A recap belongs to one fight: close it when another encounter is picked
    use_effect(move || {
        selected_encounter();
        recap_death.set(None);
    });
}

// Load encounter list on mount (auto-select latest encounter unless user had one selected)
fn use_encounter_list(ctx: ExplorerCtx) {
    let ExplorerCtx {
        mut encounters,
        mut selected_encounter,
        auto_follow,
        ..
    } = ctx;
    use_effect(move || {
        spawn(async move {
            if let Some(list) = api::get_encounter_history().await {
                let latest_id = list.last().map(|e| e.encounter_id as u32);
                let _ = encounters.try_write().map(|mut w| *w = list);
                if *auto_follow.peek()
                    && selected_encounter.peek().is_none()
                    && let Some(id) = latest_id
                {
                    let _ = selected_encounter.try_write().map(|mut w| *w = Some(id));
                }
            }
        });
    });
}

// Listen for session updates (refresh on combat end, file load, combat start for live query)
fn use_session_events(ctx: ExplorerCtx) {
    let ExplorerCtx {
        mut encounters,
        mut selected_encounter,
        auto_live,
        mut auto_follow,
        mut live_query_active,
        mut transitioning_from_live,
        mut live_poll_tick,
        mut selected_source,
        ..
    } = ctx;
    api::use_tauri_event("session-updated", move |event: JsValue| {
        // Extract payload from event object (Tauri events have { payload: "..." } structure)
        if let Ok(payload) = js_sys::Reflect::get(&event, &JsValue::from_str("payload"))
            && let Some(event_type) = payload.as_string()
        {
            // Live query: activate when combat starts (if auto-follow is on)
            if event_type.contains("CombatStarted") {
                if *auto_follow.peek() {
                    let _ = live_query_active.try_write().map(|mut w| *w = true);
                    let _ = selected_encounter.try_write().map(|mut w| *w = None);
                    let _ = live_poll_tick.try_write().map(|mut w| *w = 0);
                }
                return;
            }

            if event_type.contains("CombatEnded") || event_type.contains("FileLoaded") {
                // Mark transition so timeline effect preserves existing data
                if event_type.contains("CombatEnded") && *live_query_active.peek() {
                    let _ = transitioning_from_live.try_write().map(|mut w| *w = true);
                }

                // Deactivate live polling
                let _ = live_query_active.try_write().map(|mut w| *w = false);

                // Reset selection on file load (new file invalidates old encounter indices)
                if event_type.contains("FileLoaded") {
                    let _ = selected_encounter.try_write().map(|mut w| *w = None);
                    let _ = selected_source.try_write().map(|mut w| *w = None);
                    // Only re-enable auto-follow if auto-live preference is on
                    let should_auto_follow = *auto_live.peek();
                    let _ = auto_follow.try_write().map(|mut w| *w = should_auto_follow);
                }
                // JS callback: outside the Dioxus runtime, so not scoped `spawn`.
                // The task can outlive the component, hence try_* accessors only.
                wasm_bindgen_futures::spawn_local(async move {
                    // Refresh encounter list and auto-select latest if auto-follow
                    if let Some(list) = api::get_encounter_history().await {
                        let latest_id = list.last().map(|e| e.encounter_id as u32);
                        let _ = encounters.try_write().map(|mut w| *w = list);
                        if auto_follow.try_peek().is_ok_and(|v| *v)
                            && let Some(id) = latest_id
                        {
                            let _ = selected_encounter.try_write().map(|mut w| *w = Some(id));
                        }
                    }
                    // Safety: clear transition flag after encounter is set, in case
                    // the timeline effect didn't fire (e.g., encounter didn't change)
                    if transitioning_from_live.try_peek().is_ok_and(|v| *v) {
                        let _ = transitioning_from_live.try_write().map(|mut w| *w = false);
                    }
                });
            }
        }
    });
}

fn use_parsely_refresh(ctx: ExplorerCtx) {
    let mut encounters = ctx.encounters;
    // Listen for parsely upload success (DOM CustomEvent from ParselyUploadModal).
    // Registered once per mount and removed on unmount; handing the closure to JS
    // (into_js_value) lets the GC free it once the listener is gone.
    let parsely_listener = use_hook(|| {
        let window = web_sys::window()?;
        let callback = Closure::<dyn Fn(web_sys::Event)>::new(move |_event: web_sys::Event| {
            // Refresh encounter list to pick up persisted parsely links
            // (JS callback: outside the Dioxus runtime, so not scoped `spawn`)
            wasm_bindgen_futures::spawn_local(async move {
                if let Some(list) = api::get_encounter_history().await {
                    let _ = encounters.try_write().map(|mut w| *w = list);
                }
            });
        })
        .into_js_value()
        .unchecked_into::<js_sys::Function>();
        window
            .add_event_listener_with_callback("parsely-upload-success", &callback)
            .ok()?;
        Some(callback)
    });

    // Cleanup on component unmount
    use_drop(move || {
        if let Some(callback) = &parsely_listener
            && let Some(window) = web_sys::window()
        {
            let _ = window.remove_event_listener_with_callback("parsely-upload-success", callback);
        }
    });
}

fn use_live_tick(ctx: ExplorerCtx) {
    let ExplorerCtx {
        live_query_active,
        mut live_poll_tick,
        view_mode,
        mut shown_mode,
        selected_encounter,
        mut shown_encounter,
        ..
    } = ctx;
    // Live query polling loop — bumps live_poll_tick once a second while active.
    // Faster than this and the numbers change too quickly to read.
    use_future(move || async move {
        loop {
            gloo_timers::future::TimeoutFuture::new(1_000).await;
            if *live_query_active.peek() {
                let tick = *live_poll_tick.peek();
                live_poll_tick.set(tick.wrapping_add(1));
            }
        }
    });

    // Live data streams in continuously, so the painted view follows the tab at once.
    use_effect(move || {
        let mode = *view_mode.read();
        if *live_query_active.read() {
            shown_mode.set(Some(mode));
            shown_encounter.set(*selected_encounter.read());
        }
    });
}

fn use_timeline_loader(ctx: ExplorerCtx) {
    let ExplorerCtx {
        selected_encounter,
        live_query_active,
        mut transitioning_from_live,
        mut time_range,
        mut prev_encounter,
        mut timeline_state,
        mut shown_mode,
        mut load_generation,
        mut timeline,
        mut last_overview_fetch,
        mut shown_encounter,
        view_mode,
        mut preloaded_detail,
        ..
    } = ctx;
    // Load timeline when encounter changes - prerequisite for all data loading
    // Uses generation counter to discard stale async results on rapid switching
    use_effect(move || {
        let idx = *selected_encounter.read();
        let prev_idx = *prev_encounter.peek();

        // Check if this is an actual encounter change vs. initial mount with same encounter
        let is_encounter_change = idx != prev_idx;
        if is_encounter_change {
            prev_encounter.set(idx);
        }

        let is_live = *live_query_active.read();

        // During live query, the live poll effect handles all data loading — skip here
        if is_live {
            return;
        }

        // Check if we're transitioning from live → historical (combat just ended)
        let is_transition = *transitioning_from_live.peek();
        if is_transition {
            transitioning_from_live.set(false);
        }

        // The previous encounter's data stays painted until the new encounter's
        // lands (each loader overwrites it and then moves `shown_mode`), so only
        // the fetch marker is reset here. Skipped during the live→historical
        // transition: that is the same encounter, just promoted to a file.
        if !is_transition && is_encounter_change {
            last_overview_fetch.set(None);
        }

        // Increment generation to invalidate any in-flight requests
        let generation = *load_generation.peek() + 1;
        load_generation.set(generation);

        // Load timeline for selected encounter
        timeline_state.set(LoadState::Loading);
        preloaded_detail.set(None);

        // Capture whether we should restore time_range after timeline loads
        let restore_time_range = !is_encounter_change;
        let saved_time_range = *time_range.peek();

        spawn(async move {
            let is_stale = move || *load_generation.peek() != generation;
            if is_stale() {
                return;
            }

            // Retry up to 2 times with backoff — the session may still be loading
            let mut loaded = api::query_encounter_timeline(idx).await;
            for delay_ms in [500, 1000] {
                if loaded.is_some() || is_stale() {
                    break;
                }
                gloo_timers::future::TimeoutFuture::new(delay_ms).await;
                if is_stale() {
                    return;
                }
                loaded = api::query_encounter_timeline(idx).await;
            }
            if is_stale() {
                return;
            }
            let Some(tl) = loaded else {
                timeline_state.set(LoadState::Idle);
                // Nothing loaded for this encounter: don't leave the old one up
                shown_mode.set(None);
                shown_encounter.set(idx);
                return;
            };

            // Fetch the page for the open tab before anything is painted, so the
            // timeline, sidebar and tables of the new encounter land in one update.
            let dur = tl.duration_secs;
            let tr = if !restore_time_range || saved_time_range.end <= 0.0 {
                TimeRange::full(dur)
            } else {
                saved_time_range
            };
            let mode = *view_mode.peek();
            let is_overview = matches!(mode, ViewMode::Overview);
            let (overview, detail) = futures_util::future::join(
                fetch_overview(ctx, idx, tr, Some(dur), is_overview),
                fetch_detail(ctx, idx, mode, tr, Some(dur)),
            )
            .await;
            if is_stale() {
                return;
            }

            shown_encounter.set(idx);
            time_range.set(tr);
            timeline.set(Some(tl));
            // A tab picked while this ran is loaded by its own effect instead
            let same_tab = *view_mode.peek() == mode;
            apply_overview(ctx, idx, tr, is_overview && same_tab, overview);
            if same_tab && let Some(detail) = detail {
                apply_detail(ctx, mode, detail);
                preloaded_detail.set(Some((idx, mode, tr)));
            }
            timeline_state.set(LoadState::Loaded);
        });
    });
}

struct OverviewData {
    rows: Option<Vec<api::RaidOverviewRow>>,
    deaths: Option<Vec<api::PlayerDeath>>,
    npcs: Option<Vec<api::NpcHealthRow>>,
    /// Encounter the percentiles rate; None when this view isn't rated
    rated: Option<u32>,
    /// None while the rating table still has to be downloaded
    percentiles: Option<api::EncounterRatings>,
}

/// Raid overview (class icons for every view), plus deaths, NPC health and
/// already-downloaded Parsely percentiles when the Overview tab itself is
/// open; they don't depend on each other, so all run at once.
async fn fetch_overview(
    ctx: ExplorerCtx,
    idx: Option<u32>,
    tr: TimeRange,
    full_duration: Option<f32>,
    is_overview: bool,
) -> OverviewData {
    let tr_opt = if tr.start == 0.0 && tr.end == 0.0 {
        None
    } else {
        Some(tr)
    };
    // Use selected time range duration for rate calculations, or full fight duration
    let duration = if let Some(ref range) = tr_opt {
        Some(range.end - range.start)
    } else {
        full_duration
    };
    // Percentiles rate whole finished fights only; the backend decides the rest
    let whole_fight = tr_opt.is_none() || full_duration.is_some_and(|d| tr.is_full(d));
    let rated = idx.filter(|_| is_overview && whole_fight && ctx.state.peek().parsely_percentiles);
    if is_overview {
        let (rows, deaths, npcs, percentiles) = futures_util::future::join4(
            api::query_raid_overview(idx, tr_opt.as_ref(), duration),
            api::query_player_deaths(idx),
            api::query_npc_health(idx, tr_opt.as_ref()),
            async {
                match rated {
                    Some(i) => api::query_parsely_percentiles(i, true).await,
                    None => Some(Default::default()),
                }
            },
        )
        .await;
        OverviewData {
            rows,
            deaths,
            npcs,
            rated,
            percentiles,
        }
    } else {
        OverviewData {
            rows: api::query_raid_overview(idx, tr_opt.as_ref(), duration).await,
            deaths: None,
            npcs: None,
            rated,
            percentiles: Some(Default::default()),
        }
    }
}

fn apply_overview(ctx: ExplorerCtx, idx: Option<u32>, tr: TimeRange, is_overview: bool, data: OverviewData) {
    let ExplorerCtx {
        view_mode,
        mut shown_mode,
        mut overview_data,
        mut player_deaths,
        mut npc_health,
        mut last_overview_fetch,
        mut percentiles,
        ..
    } = ctx;
    let OverviewData {
        rows: overview,
        deaths,
        npcs,
        rated,
        percentiles: local_percentiles,
    } = data;
    // None typically means no data available (no encounters dir, etc.) - not an error.
    // Not recorded in last_overview_fetch, so the next effect trigger retries.
    let loaded = overview.is_some();
    let _ = overview_data.try_write().map(|mut w| *w = overview.unwrap_or_default());
    if is_overview {
        let _ = player_deaths.try_write().map(|mut w| *w = deaths.unwrap_or_default());
        let _ = npc_health.try_write().map(|mut w| *w = npcs.unwrap_or_default());
        if *view_mode.peek() == ViewMode::Overview {
            shown_mode.set(Some(ViewMode::Overview));
        }
    }
    // Percentiles land with the table when their rating table is already
    // downloaded; otherwise they follow once it is
    let rated = rated.filter(|_| loaded);
    let download = rated.filter(|_| local_percentiles.is_none());
    percentiles.set((rated, local_percentiles.unwrap_or_default()));
    if let Some(i) = download {
        spawn(async move {
            let p = api::query_parsely_percentiles(i, false).await.unwrap_or_default();
            if percentiles.peek().0 == rated {
                percentiles.set((rated, p));
            }
        });
    }
    if loaded {
        // The bool tracks whether overview-specific data was loaded
        let _ = last_overview_fetch
            .try_write()
            .map(|mut w| *w = Some((idx, tr, is_overview)));
    }
}

fn use_overview_loader(ctx: ExplorerCtx) {
    let ExplorerCtx {
        selected_encounter,
        live_query_active,
        transitioning_from_live,
        view_mode,
        time_range,
        mut shown_mode,
        timeline,
        last_overview_fetch,
        ..
    } = ctx;
    // Raid Debuffs loads its own rows: painted as soon as the timeline is in
    use_effect(move || {
        let mode = *view_mode.read();
        if mode == ViewMode::RaidDebuffs
            && !*live_query_active.read()
            && ctx.timeline_ready(*selected_encounter.read())
        {
            shown_mode.set(Some(mode));
        }
    });
    // Load overview data when timeline is loaded and view_mode/time_range changes
    // Overview data provides class icons for all views + full data for Overview tab
    use_effect(move || {
        let idx = *selected_encounter.read();
        let mode = *view_mode.read();
        let is_overview = matches!(mode, ViewMode::Overview);
        let tr = time_range();

        // During live query, the live poll effect handles all data loading — skip here
        // to avoid racing with live poll writes and causing data "waffle" between values.
        // Also skip during live→historical transition (combat just ended, encounter not yet set)
        if *live_query_active.read() || *transitioning_from_live.read() {
            return;
        }

        if !ctx.timeline_ready(idx) {
            return;
        }

        // Check if we've already fetched for this (encounter, time_range) combo
        let last = *last_overview_fetch.read();
        if let Some((last_idx, last_tr, had_overview)) = last {
            // Non-overview tabs only need class icons — any loaded data for this encounter is fine
            if !is_overview && last_idx == idx {
                return;
            }
            // Overview tab: skip if same encounter+range AND deaths/NPC HP were already loaded
            if is_overview && last_idx == idx && last_tr == tr && had_overview {
                shown_mode.set(Some(mode));
                return;
            }
        }

        spawn(async move {
            let full_duration = timeline.peek().as_ref().map(|t| t.duration_secs);
            let data = fetch_overview(ctx, idx, tr, full_duration, is_overview).await;
            // Superseded while the queries ran: the newer request owns the signals
            if *selected_encounter.peek() == idx && *time_range.peek() == tr {
                apply_overview(ctx, idx, tr, is_overview && *view_mode.peek() == mode, data);
            }
        });
    });
}
