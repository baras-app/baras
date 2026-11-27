//! Bar above the content area: selected-encounter strip and the view tab selector.

use dioxus::prelude::*;

use super::ctx::ExplorerCtx;
use crate::components::overview_table::pct_tier;
use crate::types::{DataTab, ViewMode};
use baras_types::formatting;

/// View tabs in display order
const VIEW_TABS: [(ViewMode, &str); 12] = [
    (ViewMode::Overview, "Overview"),
    (ViewMode::Charts, "Charts"),
    (ViewMode::Detailed(DataTab::Damage), "Damage"),
    (ViewMode::Detailed(DataTab::Healing), "Healing"),
    (ViewMode::Detailed(DataTab::DamageTaken), "Damage Taken"),
    (ViewMode::Detailed(DataTab::HealingTaken), "Healing Taken"),
    (ViewMode::CombatLog, "Combat Log"),
    (ViewMode::Usage, "Ability Usage"),
    (ViewMode::Rotation, "Rotation"),
    (ViewMode::Uptime, "Effect Uptime"),
    (ViewMode::RaidDebuffs, "Raid Debuffs"),
    (ViewMode::Aggro, "Aggro"),
];

pub(super) fn encounter_strip(ctx: ExplorerCtx) -> Element {
    let ExplorerCtx {
        encounters,
        selected_encounter,
        live_query_active,
        sidebar_collapsed,
        overview_fullscreen,
        ttk_ratings,
        ..
    } = ctx;

    rsx! {
        if *sidebar_collapsed.read() || *overview_fullscreen.read() {
            if *live_query_active.read() {
                div { class: "selected-entity-indicator live-query-indicator",
                    span { class: "status-dot watching" }
                    span { " Live Encounter" }
                }
            } else if let Some(enc_idx) = *selected_encounter.read() {
                if let Some(enc) = encounters().iter().find(|e| e.encounter_id as u32 == enc_idx) {
                    div { class: "selected-encounter-strip",
                        span { class: "enc-strip-name", title: "{enc.display_name}", "{enc.display_name}" }
                        if enc.boss_name.is_some() {
                            span { class: if enc.success { "enc-strip-result kill" } else { "enc-strip-result wipe" },
                                if enc.success { "Kill" } else { "Wipe" }
                            }
                        }
                        span { class: "enc-strip-meta",
                            if !enc.area_name.is_empty() { span { "{enc.area_name}" } }
                            if let Some(d) = enc.difficulty.as_ref() { span { "{d}" } }
                            span { class: "enc-strip-duration",
                                i { class: "fa-regular fa-clock" }
                                " {formatting::format_duration(enc.duration_seconds)}"
                            }
                            if let Some(r) = ttk_ratings.read().get(&enc.encounter_id) {
                                span {
                                    class: "pct-badge {pct_tier(r.pct)}",
                                    title: "{baras_types::percentile_label(r.pct)}\n~{r.source.total} total kills\n{r.source.updated_note()}",
                                    "{r.pct}"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

pub(super) fn view_tab_bar(ctx: ExplorerCtx) -> Element {
    let ExplorerCtx {
        mut view_mode,
        mut overview_fullscreen,
        mut death_target_filter,
        encounters,
        selected_encounter,
        ..
    } = ctx;
    // Replay needs positions on a known layout: operation boss fights only
    let replay_available = selected_encounter().is_some_and(|idx| {
        encounters
            .read()
            .iter()
            .any(|e| e.encounter_id == idx as u64 && e.boss_name.is_some() && e.encounter_type == "Raid")
    });
    let tabs = VIEW_TABS
        .into_iter()
        .chain(replay_available.then_some((ViewMode::Replay, "Replay")));

    rsx! {
        div { class: "data-tab-selector explorer-views",
            for (mode, label) in tabs {
                button {
                    class: if view_mode() == mode { "data-tab active" } else { "data-tab" },
                    onclick: move |_| {
                        if mode == ViewMode::CombatLog {
                            death_target_filter.set(None);
                        }
                        view_mode.set(mode);
                    },
                    "{label}"
                }
            }
            button {
                class: "tab-fullscreen-btn",
                title: if overview_fullscreen() { "Exit fullscreen (F / Esc)" } else { "Fullscreen (F)" },
                "data-shortcut": "f",
                "data-esc-close": overview_fullscreen().then_some("true"),
                onclick: move |_| overview_fullscreen.toggle(),
                i { class: if overview_fullscreen() { "fa-solid fa-down-left-and-up-right-to-center" } else { "fa-solid fa-up-right-and-down-left-from-center" } }
            }
        }
    }
}
