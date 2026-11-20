//! Entity and ability loading for the live buffer and the Detailed/Rotation/Usage/Charts tabs.

use dioxus::prelude::*;
use std::collections::HashMap;

use super::ctx::{ExplorerCtx, LoadState};
use crate::api::{self, AbilityBreakdown, DamageTakenSummary, EntityBreakdown, TimeRange};
use crate::types::{DataTab, ViewMode};

/// Entity list for Rotation/Usage/Charts: Damage + Healing totals merged, plus every
/// other actor from the Charts roster (buff-only adds, healers with 0 dmg) at 0.
/// Returns (sorted entities, per-entity damage totals for color coding).
async fn load_actor_entities(idx: Option<u32>, tr: Option<&TimeRange>) -> (Vec<EntityBreakdown>, HashMap<String, f64>) {
    let (dmg, heal, roster) = futures_util::future::join3(
        api::query_entity_breakdown(DataTab::Damage, idx, tr),
        api::query_entity_breakdown(DataTab::Healing, idx, tr),
        api::query_entity_breakdown(DataTab::Charts, idx, tr),
    )
    .await;
    let (dmg, heal, roster) = (
        dmg.unwrap_or_default(),
        heal.unwrap_or_default(),
        roster.unwrap_or_default(),
    );
    let dmg_map: HashMap<String, f64> = dmg.iter().map(|e| (e.source_name.clone(), e.total_value)).collect();
    // Keyed by (name, instance id) so same-named NPC spawns stay separate rows
    let mut merged: HashMap<(String, i64), EntityBreakdown> = HashMap::new();
    for e in dmg.into_iter().chain(heal) {
        merged
            .entry((e.source_name.clone(), e.source_id))
            .and_modify(|existing| {
                existing.total_value += e.total_value;
                existing.abilities_used = existing.abilities_used.max(e.abilities_used);
            })
            .or_insert(e);
    }
    for e in roster {
        merged.entry((e.source_name.clone(), e.source_id)).or_insert(e);
    }
    let mut result: Vec<_> = merged.into_values().collect();
    result.sort_by(|a, b| {
        b.total_value
            .partial_cmp(&a.total_value)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.source_name.cmp(&b.source_name))
    });
    (result, dmg_map)
}

pub(super) fn use_live_loader(ctx: ExplorerCtx) {
    let ExplorerCtx {
        live_query_active,
        live_poll_tick,
        view_mode,
        mut selected_source,
        selected_source_id,
        breakdown_mode,
        mut time_range,
        local_player_name,
        mut abilities,
        mut entities,
        mut entity_dmg_totals,
        mut timeline_state,
        mut timeline,
        mut overview_data,
        mut player_deaths,
        mut npc_health,
        ..
    } = ctx;
    // Live query polling: re-query timeline + overview data on each poll tick
    // This is separate from the normal effects to avoid interference with historical queries
    use_effect(move || {
        let _tick = *live_poll_tick.read(); // subscribe to poll tick changes
        let is_active = *live_query_active.read();

        if !is_active {
            return;
        }

        // Query with None = live encounter buffer
        spawn(async move {
            // Bail if live query was deactivated while spawning
            if !*live_query_active.peek() {
                return;
            }

            // 1. Query timeline for live duration
            if let Some(tl) = api::query_encounter_timeline(None).await {
                let dur = tl.duration_secs;
                // Only update time_range if duration changed meaningfully (>0.5s)
                // to avoid cascading downstream effects on every poll tick
                let current_tr = *time_range.peek();
                let new_tr = TimeRange::full(dur);
                if (current_tr.end - new_tr.end).abs() > 0.5 || (current_tr.start - new_tr.start).abs() > 0.01 {
                    time_range.set(new_tr);
                }
                timeline.set(Some(tl));
                // Only notify subscribers if timeline_state actually changed
                if !matches!(*timeline_state.peek(), LoadState::Loaded) {
                    timeline_state.set(LoadState::Loaded);
                }
            } else {
                // Buffer may not be ready yet (very start of combat) — skip this tick
                return;
            }

            // 2. Query overview data
            let full_duration = timeline.read().as_ref().map(|t| t.duration_secs);
            if let Some(data) = api::query_raid_overview(None, None, full_duration).await {
                let _ = overview_data.try_write().map(|mut w| *w = data);
            }

            // 3. Query deaths + NPC health
            if let Some(deaths) = api::query_player_deaths(None).await {
                let _ = player_deaths.try_write().map(|mut w| *w = deaths);
            }
            if let Some(npcs) = api::query_npc_health(None, None).await {
                let _ = npc_health.try_write().map(|mut w| *w = npcs);
            }

            // 4. Query entity + ability breakdown for detailed/rotation/usage/charts tabs
            // Use peek() to avoid subscribing — the live_poll_tick drives re-execution
            let mode = *view_mode.peek();
            let tab = match mode {
                ViewMode::Detailed(tab) => Some(tab),
                ViewMode::Rotation | ViewMode::Usage | ViewMode::Charts | ViewMode::Uptime => Some(DataTab::Damage),
                _ => None,
            };
            if let Some(tab) = tab {
                // Bail if live query was deactivated during earlier fetches
                if !*live_query_active.peek() {
                    return;
                }

                // Load entity breakdown
                let entity_data = if matches!(
                    mode,
                    ViewMode::Rotation | ViewMode::Usage | ViewMode::Charts | ViewMode::Uptime
                ) {
                    let (result, dmg_map) = load_actor_entities(None, None).await;
                    entity_dmg_totals.set(dmg_map);
                    result
                } else {
                    api::query_entity_breakdown(tab, None, None).await.unwrap_or_default()
                };

                // Auto-select source if none selected
                if selected_source.peek().is_none() {
                    let players = entity_data
                        .iter()
                        .filter(|e| e.entity_type == "Player" || e.entity_type == "Companion");
                    let local_name = local_player_name.peek().clone();
                    let auto = if let Some(name) = local_name.as_deref() {
                        players
                            .clone()
                            .find(|e| e.source_name == name)
                            .or_else(|| players.clone().next())
                            .map(|e| e.source_name.clone())
                    } else {
                        players.clone().next().map(|e| e.source_name.clone())
                    };
                    if auto.is_some() {
                        let _ = selected_source.try_write().map(|mut w| *w = auto.clone());
                    }
                }

                let _ = entities.try_write().map(|mut w| *w = entity_data);

                // Load ability breakdown for selected source
                let src = selected_source.peek().clone();
                let src_id = *selected_source_id.peek();
                let breakdown = *breakdown_mode.peek();
                let duration = timeline.read().as_ref().map(|t| t.duration_secs);
                if let Some(data) = api::query_breakdown(
                    tab,
                    None,
                    src.as_deref(),
                    src_id,
                    None,
                    None,
                    Some(&breakdown),
                    duration,
                )
                .await
                {
                    let _ = abilities.try_write().map(|mut w| *w = data);
                }
            }
        });
    });
}

/// One fetch of the entity list and what the tab shows beside it.
pub(super) struct DetailData {
    entities: Vec<EntityBreakdown>,
    dmg_totals: Option<HashMap<String, f64>>,
    abilities: Vec<AbilityBreakdown>,
    summary: Option<DamageTakenSummary>,
    auto_selected: Option<String>,
}

/// Fetch entities + abilities for `mode` without touching the painted signals.
/// `None` for the views that load themselves (Overview, CombatLog).
pub(super) async fn fetch_detail(
    ctx: ExplorerCtx,
    idx: Option<u32>,
    mode: ViewMode,
    tr: TimeRange,
    duration: Option<f32>,
) -> Option<DetailData> {
    let ExplorerCtx {
        selected_source,
        selected_source_id,
        breakdown_mode,
        local_player_name,
        ..
    } = ctx;
    let tab = match mode {
        ViewMode::Detailed(tab) => tab,
        ViewMode::Rotation | ViewMode::Usage | ViewMode::Charts | ViewMode::Uptime => DataTab::Damage,
        // Overview has its own loader; the combat log loads its own rows and
        // reports in through `on_ready`
        ViewMode::CombatLog | ViewMode::Overview => return None,
    };
    let tr_opt = if tr.start == 0.0 && tr.end == 0.0 {
        None
    } else {
        Some(tr)
    };

    let (entities, dmg_totals) = if matches!(mode, ViewMode::Detailed(_)) {
        // None = no data available: shown as an empty tab
        let list = api::query_entity_breakdown(tab, idx, tr_opt.as_ref())
            .await
            .unwrap_or_default();
        (list, None)
    } else {
        let (list, dmg_map) = load_actor_entities(idx, tr_opt.as_ref()).await;
        (list, Some(dmg_map))
    };

    // Auto-select player: keep the held selection if it exists in this encounter,
    // otherwise fall back to the local player, then the first player
    let auto_selected = {
        let current = selected_source.peek().clone();
        let mut players = entities
            .iter()
            .filter(|e| e.entity_type == "Player" || e.entity_type == "Companion");
        if current
            .as_ref()
            .is_some_and(|name| players.clone().any(|e| &e.source_name == name))
        {
            current
        } else {
            let local_name = local_player_name.peek();
            local_name
                .as_deref()
                .and_then(|name| players.clone().find(|e| e.source_name == name))
                .or_else(|| players.next())
                .map(|e| e.source_name.clone())
        }
    };

    // Instance id only applies while the held selection survived
    let auto_id = if auto_selected == *selected_source.peek() {
        *selected_source_id.peek()
    } else {
        None
    };

    let breakdown = *breakdown_mode.peek();
    // The DamageTaken summary sits above the table, so it has to land with
    // it: arriving later would push the table down.
    let summary_source = auto_selected
        .as_deref()
        .filter(|_| mode == ViewMode::Detailed(DataTab::DamageTaken));
    // Only the Detailed tabs show the ability table
    let wants_abilities = matches!(mode, ViewMode::Detailed(_));
    let (abilities, summary) = futures_util::future::join(
        async {
            if !wants_abilities {
                return None;
            }
            api::query_breakdown(
                tab,
                idx,
                auto_selected.as_deref(),
                auto_id,
                tr_opt.as_ref(),
                None, // No entity filter when source is selected
                Some(&breakdown),
                duration,
            )
            .await
        },
        async {
            match summary_source {
                Some(name) => api::query_damage_taken_summary(idx, name, auto_id, tr_opt.as_ref(), None).await,
                None => None,
            }
        },
    )
    .await;

    Some(DetailData {
        entities,
        dmg_totals,
        abilities: abilities.unwrap_or_default(),
        summary,
        auto_selected,
    })
}

/// Entities, abilities and the painted view change in one update.
pub(super) fn apply_detail(ctx: ExplorerCtx, mode: ViewMode, data: DetailData) {
    let ExplorerCtx {
        mut selected_source,
        mut abilities,
        mut entities,
        mut entity_dmg_totals,
        mut shown_mode,
        mut dt_summary,
        ..
    } = ctx;
    if let Some(totals) = data.dmg_totals {
        let _ = entity_dmg_totals.try_write().map(|mut w| *w = totals);
    }
    let _ = entities.try_write().map(|mut w| *w = data.entities);
    let _ = abilities.try_write().map(|mut w| *w = data.abilities);
    let _ = dt_summary.try_write().map(|mut w| *w = data.summary);
    if selected_source.peek().is_none() && data.auto_selected.is_some() {
        let _ = selected_source.try_write().map(|mut w| *w = data.auto_selected);
    }
    shown_mode.set(Some(mode));
}

pub(super) fn use_detailed_loader(ctx: ExplorerCtx) {
    let ExplorerCtx {
        selected_encounter,
        live_query_active,
        transitioning_from_live,
        view_mode,
        time_range,
        timeline_state,
        timeline,
        mut preloaded_detail,
        ..
    } = ctx;
    // Lazy load: Detailed tab data (entities + abilities) for Damage/Healing/etc tabs
    use_effect(move || {
        let idx = *selected_encounter.read();
        let mode = *view_mode.read();
        let tr = time_range();
        let tl_state = timeline_state();

        // During live query, the live poll effect handles data loading — skip here
        // to avoid racing with live poll writes and causing data "waffle" between values.
        // Also skip during live→historical transition (combat just ended, encounter not yet set)
        if *live_query_active.read() || *transitioning_from_live.read() {
            return;
        }

        // Only load when timeline is loaded
        if !matches!(tl_state, LoadState::Loaded) {
            return;
        }

        // The encounter load already brought this exact page in with the timeline
        if *preloaded_detail.peek() == Some((idx, mode, tr)) {
            return;
        }
        if preloaded_detail.peek().is_some() {
            preloaded_detail.set(None);
        }

        spawn(async move {
            let duration = timeline.peek().as_ref().map(|t| t.duration_secs);
            let Some(data) = fetch_detail(ctx, idx, mode, tr, duration).await else {
                return;
            };
            // Superseded while the queries ran: the newer request owns the signals
            if *view_mode.peek() != mode || *selected_encounter.peek() != idx || *time_range.peek() != tr {
                return;
            }
            apply_detail(ctx, mode, data);
        });
    });
}

pub(super) fn use_ability_reload(ctx: ExplorerCtx) {
    let ExplorerCtx {
        selected_encounter,
        live_query_active,
        transitioning_from_live,
        selected_source,
        selected_source_id,
        breakdown_mode,
        show_players_only,
        time_range,
        mut abilities,
        timeline_state,
        shown_mode,
        timeline,
        ..
    } = ctx;
    // NOTE: Time range changes are now handled by the tab-specific effects above
    // They read time_range() which triggers reload when it changes

    // Reload abilities when entity filter or breakdown mode changes
    use_effect(move || {
        let players_only = *show_players_only.read();
        let breakdown = *breakdown_mode.read();
        let idx = *selected_encounter.read();
        // The painted view, not the clicked tab: its abilities must not change
        // under the old page while the new tab is still loading
        let Some(view) = *shown_mode.read() else {
            return;
        };
        let src = selected_source.read().clone();
        let src_id = *selected_source_id.read();
        let tr = time_range();
        let tl_state = timeline_state();

        // During live query, the live poll effect handles data loading — skip here.
        // Also skip during live→historical transition (combat just ended, encounter not yet set)
        if *live_query_active.read() || *transitioning_from_live.read() {
            return;
        }

        // Extract tab if in detailed mode
        let Some(tab) = view.tab() else {
            return;
        };

        // Skip if timeline not loaded
        if !matches!(tl_state, LoadState::Loaded) {
            return;
        }

        spawn(async move {
            // Apply entity filter only when no specific source is selected
            let entity_filter: Option<&[&str]> = if src.is_none() {
                if players_only {
                    Some(&["Player", "Companion"])
                } else {
                    Some(&["Npc"])
                }
            } else {
                None
            };
            let tr_opt = if tr.start == 0.0 && tr.end == 0.0 {
                None
            } else {
                Some(tr)
            };
            let duration = timeline.read().as_ref().map(|t| t.duration_secs);
            if let Some(data) = api::query_breakdown(
                tab,
                idx,
                src.as_deref(),
                src_id,
                tr_opt.as_ref(),
                entity_filter,
                Some(&breakdown),
                duration,
            )
            .await
            {
                let _ = abilities.try_write().map(|mut w| *w = data);
            }
        });
    });
}

pub(super) fn use_dt_summary(ctx: ExplorerCtx) {
    let ExplorerCtx {
        selected_encounter,
        live_query_active,
        transitioning_from_live,
        selected_source,
        selected_source_id,
        time_range,
        timeline_state,
        shown_mode,
        mut dt_summary,
        ..
    } = ctx;
    // DamageTaken summary: refetched when the source or time range changes on that tab
    use_effect(move || {
        let view = *shown_mode.read();
        let idx = *selected_encounter.read();
        let src = selected_source.read().clone();
        let src_id = *selected_source_id.read();
        let tr = time_range();
        let tl_state = timeline_state();

        // Skip during live query to avoid cascade race conditions
        if *live_query_active.read() || *transitioning_from_live.read() {
            return;
        }

        // Only fetch for DamageTaken tab with a selected source
        if view != Some(ViewMode::Detailed(DataTab::DamageTaken)) || src.is_none() {
            dt_summary.set(None);
            return;
        }
        // A new encounter is loading: its summary arrives with the rest of the page
        if !matches!(tl_state, LoadState::Loaded) {
            return;
        }

        let entity_name = src.unwrap();
        spawn(async move {
            let tr_opt = if tr.start == 0.0 && tr.end == 0.0 {
                None
            } else {
                Some(tr)
            };
            // Don't filter source entity types — the target_name filter already scopes the query
            let result = api::query_damage_taken_summary(idx, &entity_name, src_id, tr_opt.as_ref(), None).await;
            let _ = dt_summary.try_write().map(|mut w| *w = result);
        });
    });
}
