//! Overview of a training dummy parse: the numbers Parsely ranks with their
//! percentiles, the damage chart, and the time between global cooldowns.

use dioxus::prelude::*;
use wasm_bindgen::JsCast;

use baras_types::formatting;
use baras_types::{
    DummyParse, GcdPress, LONG_GCD_MARGIN_SECS, RatingSource, is_long_gap, percentile_label,
};

use super::ctx::ExplorerCtx;
use super::dummy_charts::{IntervalDensity, PressTimeline, ability_gaps, quantile};
use crate::api;
use crate::components::ability_icon::AbilityIcon;
use crate::components::ability_naming::AbilityNamingPills;
use crate::components::class_icons::{get_class_icon, get_role_icon};
use crate::components::mirror_names::{display_id, display_name};
use crate::components::overview_table::pct_tier;

/// Height the press list scrolls within
const PRESSES_MAX_HEIGHT: &str = "max-height: 330px;";
/// The two tables sharing the column beside the press list fill the same height
const ABILITIES_ABOVE_CLIPS_HEIGHT: &str = "max-height: 185px; margin-bottom: var(--space-md);";
const CLIPS_MAX_HEIGHT: &str = "max-height: 110px;";
/// DOM id prefix of a press row, followed by its index in `gcd.presses`
const PRESS_ROW_ID: &str = "gcd-press-";

/// The player a dummy parse view summarizes
#[derive(Clone, PartialEq)]
pub(super) struct DummyPlayer {
    name: String,
    discipline: String,
    role_icon: Option<String>,
    class_icon: Option<String>,
}

/// The dummy parse being shown and the player to summarize: the selected
/// source when it took part, else the top damage dealer.
pub(super) fn dummy_target(ctx: ExplorerCtx) -> Option<(u32, DummyPlayer)> {
    let idx = (ctx.shown_encounter)()?;
    let encounters = ctx.encounters.read();
    let enc = encounters
        .iter()
        .find(|e| e.encounter_id == idx as u64 && e.encounter_type == "DummyParse")?;
    let selected = (ctx.selected_source)();
    let p = enc
        .player_metrics
        .iter()
        .find(|p| Some(&p.name) == selected.as_ref())
        .or_else(|| enc.player_metrics.iter().max_by_key(|p| p.dps))?;
    Some((
        idx,
        DummyPlayer {
            name: p.name.clone(),
            discipline: p.discipline_name.clone().unwrap_or_default(),
            role_icon: p.role_icon.clone(),
            class_icon: p.class_icon.clone(),
        },
    ))
}

/// Value card with its Parsely percentile, when rated
#[component]
fn StatCard(label: &'static str, value: String, pct: Option<u8>, source: RatingSource, count: u32) -> Element {
    rsx! {
        div { class: "history-card",
            div { class: "k", "{label}" }
            div { class: "v",
                "{value} "
                if let Some(pct) = pct.filter(|p| *p > 0) {
                    span {
                        class: "pct-badge {pct_tier(pct)}",
                        title: "{percentile_label(pct)}\n~{count} parses\n{source.updated_note()}",
                        "{pct}"
                    }
                }
            }
        }
    }
}

/// `[icon] name` cell, as in the effect tables, under the naming preference
fn ability_cell(id: i64, name: &str) -> Element {
    let (id, name) = (display_id(id), display_name(id, name));
    rsx! {
        td { class: "effect-name-cell", title: "{name}",
            div { class: "effect-name-inner",
                AbilityIcon { key: "{id}", ability_id: id, size: 16 }
                span { class: "effect-name-text", "{name}" }
            }
        }
    }
}

#[component]
pub(super) fn DummyParseView(
    encounter_idx: u32,
    player: DummyPlayer,
    european: bool,
    /// The encounter's damage chart, shown under the stat cards
    chart: Element,
) -> Element {
    let eu = european;

    // Props are plain values, so the fetch is keyed on a memo of them
    let name = player.name.clone();
    let key = use_memo(use_reactive!(|(encounter_idx, name)| (encounter_idx, name)));
    let resource = use_resource(move || {
        let (idx, name) = key();
        async move { api::query_dummy_parse(idx, &name).await }
    });
    // Index into `gcd.presses`, shared by the timeline and the press table
    let mut selected = use_signal(|| None::<usize>);
    // Ability hovered in the by-ability table: its presses stand out on the
    // timeline, and the density chart shows its histogram
    let mut hovered = use_signal(|| None::<i64>);
    // Clicked in the same table: stays shown until clicked again
    let mut pinned = use_signal(|| None::<i64>);
    let focused = use_memo(move || hovered().or(pinned()));
    use_effect(move || {
        key();
        selected.set(None);
        hovered.set(None);
        pinned.set(None);
    });
    // Picked on the timeline: bring its row into view
    use_effect(move || {
        if let Some(i) = selected()
            && let Some(row) = web_sys::window()
                .and_then(|w| w.document())
                .and_then(|d| d.get_element_by_id(&format!("{PRESS_ROW_ID}{i}")))
                .and_then(|el| el.dyn_into::<web_sys::HtmlElement>().ok())
        {
            crate::shortcuts::scroll_into_view_nearest(&row);
        }
    });
    let parse_read = resource.read();
    let Some(parse): Option<&DummyParse> = parse_read.as_ref().and_then(|p| p.as_ref()) else {
        return rsx! { div { class: "overview-section", p { class: "hint", "Loading parse…" } } };
    };

    let pcts = parse.percentiles.as_ref();
    let source_info = pcts.map(|p| p.source.clone()).unwrap_or_default();
    let count = pcts.map_or(0, |p| p.count);
    let dec = |v: f32| formatting::format_decimal(v, 1, eu);
    let secs = |v: f32| format!("{} s", formatting::format_decimal(v, 3, eu));
    let gcd = &parse.gcd;

    // Longest first, each with its index in the chronological list
    let mut presses: Vec<(usize, &GcdPress)> = gcd.presses.iter().enumerate().collect();
    presses.sort_by(|a, b| b.1.interval_secs.total_cmp(&a.1.interval_secs));
    let longest = presses.first().map_or(0.0, |(_, p)| p.interval_secs);
    let mean = gcd.intervals_secs.iter().sum::<f32>() / gcd.intervals_secs.len().max(1) as f32;
    let long_from = gcd.typical_gcd_secs + LONG_GCD_MARGIN_SECS;
    let long_class = |long: bool| if long { "num text-error" } else { "num" };
    let press_class = |p: &GcdPress| match (p.is_clipped(), p.is_long()) {
        (true, _) => "num text-warning",
        (_, long) => long_class(long),
    };
    // Channels cut short by the next press, chronological
    let clips: Vec<(usize, &GcdPress)> = gcd.presses.iter().enumerate().filter(|(_, p)| p.is_clipped()).collect();
    let channels = gcd.presses.iter().filter(|p| p.channel.is_some()).count();
    // The hovered or pinned ability's presses, when it has any
    let focus = focused()
        .and_then(|id| gcd.abilities.iter().find(|a| a.ability_id == id))
        .map(|a| (a, ability_gaps(gcd, a.ability_id)))
        .filter(|(_, g)| !g.is_empty());
    // Count label, count, median, mean, longest, and what a press is (for titles)
    let stats = match &focus {
        Some((a, g)) => (
            "Presses",
            g.len() as u32,
            quantile(g, 0.5),
            g.iter().sum::<f32>() / g.len() as f32,
            g[g.len() - 1],
            format!("a {} press", display_name(a.ability_id, &a.ability_name)),
        ),
        None => (
            "GCDs Used",
            gcd.gcd_count,
            gcd.typical_gcd_secs,
            mean,
            longest,
            "one on-GCD press".to_string(),
        ),
    };
    let picked = selected();
    let pin = pinned();

    rsx! {
        div { class: "overview-section",
            div { class: "overview-column-header",
                if let Some(icon) = player.role_icon.as_deref().and_then(get_role_icon) {
                    img { class: "role-icon", src: *icon, alt: "" }
                }
                if let Some(icon) = player.class_icon.as_deref().and_then(get_class_icon) {
                    img { class: "class-icon", src: *icon, title: "{player.discipline}", alt: "" }
                }
                " {player.name}"
                span { class: "text-muted", " {player.discipline}" }
            }
            div { class: "history-cards",
                StatCard { label: "DPS", value: formatting::format_compact(parse.dps, eu), pct: pcts.map(|p| p.dps), source: source_info.clone(), count }
                StatCard { label: "Crit Rate", value: format!("{} %", dec(parse.crit_pct)), pct: pcts.map(|p| p.crit), source: source_info.clone(), count }
                StatCard { label: "Crit Dmg", value: format!("{} %", dec(parse.crit_damage_pct)), pct: pcts.map(|p| p.crit_damage), source: source_info.clone(), count }
                StatCard { label: "APM", value: dec(parse.apm), pct: pcts.map(|p| p.apm), source: source_info.clone(), count }
                StatCard { label: "TTK", value: formatting::format_duration(parse.duration_secs), pct: pcts.map(|p| p.ttk), source: source_info.clone(), count: source_info.total }
            }
            if !parse.killed {
                p { class: "overview-ratings-note", "Only parses that kill the dummy are rated on Parsely." }
            } else if pcts.is_none() {
                p { class: "overview-ratings-note", "Parsely percentiles unavailable" }
            }

            {chart}

            div { class: "overview-columns",
                div { class: "overview-column",
                    div { class: "overview-column-header",
                        i { class: "fa-solid fa-stopwatch" }
                        " Global Cooldown"
                        AbilityNamingPills {}
                    }
                    div { class: "dummy-summary",
                    div { class: "history-cards",
                        div { class: "history-card", div { class: "k", "{stats.0}" } div { class: "v", "{stats.1}" } }
                        div { class: "history-card", title: "Median time from {stats.5} to the next",
                            div { class: "k", "Median" } div { class: "v", {secs(stats.2)} }
                        }
                        div { class: "history-card", title: "Mean time from {stats.5} to the next",
                            div { class: "k", "Mean" } div { class: "v", {secs(stats.3)} }
                        }
                        div { class: "history-card", div { class: "k", "Longest" } div { class: "v", {secs(stats.4)} } }
                    }
                    if !gcd.presses.is_empty() {
                        IntervalDensity { parse: resource, hovered: focused, european: eu }
                    }
                    }
                    if !gcd.presses.is_empty() {
                        PressTimeline { parse: resource, selected, hovered: focused, european: eu }
                    }
                }
            }

            if !gcd.presses.is_empty() {
                div { class: "overview-columns",
                    div { class: "overview-column",
                        div { class: "overview-column-header",
                            i { class: "fa-solid fa-hourglass-half" }
                            " Presses"
                            if let Some(a) = pin.and_then(|id| gcd.abilities.iter().find(|a| a.ability_id == id)) {
                                span { class: "text-muted", " {display_name(a.ability_id, &a.ability_name)}" }
                            }
                        }
                        div { class: "effect-table-wrapper", style: PRESSES_MAX_HEIGHT,
                            table { class: "effect-table",
                                thead {
                                    tr {
                                        th { class: "num col-apps", "Time" }
                                        th { class: "num col-uptime", title: "Time until the next press; red from {secs(long_from)}, or for a full channel {secs(LONG_GCD_MARGIN_SECS)} past that channel's median; amber when a channel was clipped", "To Next" }
                                        th { "Ability" }
                                        th { "Next" }
                                    }
                                }
                                tbody {
                                    // Only the pinned ability's presses, plus one picked on the timeline
                                    for (i, p) in presses.into_iter().filter(|(i, p)| pin.is_none_or(|id| p.ability_id == id) || picked == Some(*i)) {
                                        {
                                            let t = p.time_secs;
                                            rsx! {
                                                tr {
                                                    key: "{i}",
                                                    id: "{PRESS_ROW_ID}{i}",
                                                    class: if picked == Some(i) { "selected" },
                                                    title: "Click to mark on the timeline",
                                                    onclick: move |_| selected.set((picked != Some(i)).then_some(i)),
                                                    td { class: "num", {formatting::format_duration(t as i64)} }
                                                    td { class: press_class(p), {secs(p.interval_secs)} }
                                                    {ability_cell(p.ability_id, &p.ability)}
                                                    {ability_cell(p.next_id, &p.next)}
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    div { class: "overview-column",
                        div { class: "overview-column-header",
                            i { class: "fa-solid fa-list" }
                            " Time To Next Press by Ability"
                        }
                        div { class: "effect-table-wrapper", style: if clips.is_empty() { PRESSES_MAX_HEIGHT } else { ABILITIES_ABOVE_CLIPS_HEIGHT },
                            table { class: "effect-table",
                                thead {
                                    tr {
                                        th { "Ability" }
                                        th { class: "num col-uptime", "Uses" }
                                        th { class: "num col-uptime", "Median" }
                                        th { class: "num col-uptime", "Longest" }
                                    }
                                }
                                tbody {
                                    for a in gcd.abilities.iter() {
                                        tr { key: "{a.ability_id}",
                                            class: if pin == Some(a.ability_id) { "selected" },
                                            style: "cursor: pointer;",
                                            onclick: {
                                                let id = a.ability_id;
                                                move |_| pinned.set(if pinned() == Some(id) { None } else { Some(id) })
                                            },
                                            onmouseenter: {
                                                let id = a.ability_id;
                                                move |_| hovered.set(Some(id))
                                            },
                                            onmouseleave: move |_| hovered.set(None),
                                            {ability_cell(a.ability_id, &a.ability_name)}
                                            td { class: "num", "{a.uses}" }
                                            td { class: "num", {secs(a.median_secs)} }
                                            td { class: long_class(is_long_gap(a.longest_secs, a.expected_secs)), {secs(a.longest_secs)} }
                                        }
                                    }
                                }
                            }
                        }
                        if !clips.is_empty() {
                            div { class: "overview-column-header",
                                i { class: "fa-solid fa-scissors" }
                                " Clipped Channels"
                                span { class: "text-muted", " {clips.len()} of {channels} channels" }
                            }
                            div { class: "effect-table-wrapper", style: CLIPS_MAX_HEIGHT,
                                table { class: "effect-table",
                                    thead {
                                        tr {
                                            th { class: "num col-apps", "Time" }
                                            th { "Channel" }
                                            th { class: "num col-uptime", title: "Ticks landed before the next press, of a full channel's", "Ticks" }
                                            th { class: "num col-uptime", title: "Time until the next press", "To Next" }
                                            th { "Next" }
                                        }
                                    }
                                    tbody {
                                        for (i, p) in clips.iter().copied() {
                                            {
                                                let t = p.time_secs;
                                                let ticks = p.channel.map_or(String::new(), |c| match c.full_ticks {
                                                    0 => c.ticks.to_string(),
                                                    full => format!("{}/{full}", c.ticks),
                                                });
                                                rsx! {
                                                    tr {
                                                        key: "{i}",
                                                        class: if picked == Some(i) { "selected" },
                                                        title: "Click to mark on the timeline",
                                                        onclick: move |_| selected.set((picked != Some(i)).then_some(i)),
                                                        td { class: "num", {formatting::format_duration(t as i64)} }
                                                        {ability_cell(p.ability_id, &p.ability)}
                                                        td { class: "num", "{ticks}" }
                                                        td {
                                                            class: "num text-warning",
                                                            title: "{secs(p.expected_secs)} is the median after this ability's full channels",
                                                            {secs(p.interval_secs)}
                                                        }
                                                        {ability_cell(p.next_id, &p.next)}
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
