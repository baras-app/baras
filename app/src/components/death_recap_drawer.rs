//! Death Recap drawer — summarizes the final seconds before a player death.
//!
//! Slides in from the right when a death is picked in the Overview tab's
//! tracker; the page beside it stays usable, so another death can be picked. Loads lazily via
//! `query_death_recap` and renders a header verdict, HP sparkline, damage
//! taken / healing received tables, and a condensed event ledger.

use baras_types::{DeathRecapDamageSource, DeathRecapEvent, DeathRecapHealSource, formatting};
use dioxus::prelude::*;
use wasm_bindgen_futures::spawn_local as spawn;

use crate::api::{self, DeathRecap, DeathRecapEventKind, PlayerDeath, RaidOverviewRow};
use crate::components::class_icons::{get_class_icon, get_role_icon};
use crate::components::editor_drawer::{ResizeHandle, window_width};

/// Width (px) the user dragged the drawer to; None = the CSS default. Kept
/// for the session so each recap opens at the same size.
static RECAP_WIDTH: GlobalSignal<Option<f64>> = Signal::global(|| None);

/// Sparkline viewBox dimensions (scaled to container via CSS).
const SPARK_W: f32 = 600.0;
const SPARK_H: f32 = 72.0;
const SPARK_PAD: f32 = 3.0;
/// Spacing of the time axis ticks, counted back from the death
const TICK_SECS: f32 = 3.0;

#[derive(Props, Clone, PartialEq)]
pub struct DeathRecapDrawerProps {
    pub encounter_idx: Option<u32>,
    pub player_name: String,
    pub death_time_secs: f32,
    pub european: bool,
    pub on_close: EventHandler<()>,
    /// The player's overview row, for the role/class icons in the header
    #[props(default)]
    pub player: Option<RaidOverviewRow>,
    /// Every death in the encounter, chronological (for prev/next navigation)
    pub deaths: Vec<PlayerDeath>,
    /// Navigated to another death
    pub on_select: EventHandler<PlayerDeath>,
    /// Payload is the recap window start (combat seconds) for the log filter
    pub on_open_combat_log: EventHandler<f32>,
}

#[component]
pub fn DeathRecapDrawer(props: DeathRecapDrawerProps) -> Element {
    // None = loading, Some(None) = query failed/empty, Some(Some) = loaded
    let mut recap = use_signal(|| None::<Option<DeathRecap>>);

    // Not keyed by death, so stepping between deaths swaps the content in place
    // (no reopen animation). The old recap stays up until the new one lands;
    // `latest` drops responses that a quicker click has already superseded.
    let mut latest = use_signal(|| (String::new(), 0.0f32));
    let (idx, name, death_time) = (props.encounter_idx, props.player_name.clone(), props.death_time_secs);
    use_effect(use_reactive!(|(idx, name, death_time)| {
        latest.set((name.clone(), death_time));
        spawn(async move {
            let result = api::query_death_recap(idx, &name, death_time).await;
            if *latest.peek() == (name, death_time) {
                recap.set(Some(result));
            }
        });
    }));

    let eu = props.european;
    let time_str = formatting::format_duration(props.death_time_secs as i64);
    let state = recap.read().clone();
    let phase_chip = state
        .as_ref()
        .and_then(|o| o.as_ref())
        .and_then(|r| r.phase_name.clone());
    let window_start = state.as_ref().and_then(|o| o.as_ref()).map(|r| r.window_start_secs);
    let role_icon = props.player.as_ref().and_then(|p| p.role_icon.as_deref()).and_then(get_role_icon);
    let class_icon = props.player.as_ref().and_then(|p| p.class_icon.as_deref()).and_then(get_class_icon);
    let discipline = props.player.as_ref().and_then(|p| p.discipline_name.clone()).unwrap_or_default();

    rsx! {
        aside {
            class: "drawer death-recap-drawer",
            style: RECAP_WIDTH().map(|w| format!("width: {w:.0}px")).unwrap_or_default(),
            tabindex: "-1",
            onkeydown: move |e| {
                if e.key() == Key::Escape {
                    props.on_close.call(());
                }
            },
            // Default matches the CSS min(760px, 92vw)
            ResizeHandle { width: RECAP_WIDTH.signal(), fallback: (window_width() * 0.92).min(760.0) }
            div { class: "recap-header",
                h3 {
                    i { class: "fa-solid fa-skull recap-header-skull" }
                    if let Some(icon) = role_icon {
                        img { class: "role-icon", src: *icon, alt: "" }
                    }
                    if let Some(icon) = class_icon {
                        img { class: "class-icon", src: *icon, title: "{discipline}", alt: "" }
                    }
                    "{props.player_name}"
                    span { class: "recap-header-time", " @ {time_str}" }
                    if let Some(phase) = phase_chip {
                        span { class: "recap-phase-chip", "{phase}" }
                    }
                }
                button {
                    class: "btn btn-xs btn-ghost",
                    title: "Close (Esc)",
                    onclick: move |_| props.on_close.call(()),
                    i { class: "fa-solid fa-xmark" }
                }
            }
            div { class: "death-recap-body",
                {
                    match state {
                        None => rsx! {
                            div { class: "recap-loading", "Loading recap..." }
                        },
                        Some(None) => rsx! {
                            div { class: "recap-loading", "No recap data available for this death." }
                        },
                        Some(Some(r)) => rsx! {
                            RecapBody {
                                key: "{r.death_time_secs}",
                                recap: r,
                                european: eu,
                            }
                        },
                    }
                }
            }
            // Pinned below the body so the controls don't move between deaths
            div { class: "recap-footer",
                {recap_nav(&props.deaths, &props.player_name, props.death_time_secs, props.on_select)}
                if let Some(start) = window_start {
                    button {
                        class: "btn",
                        onclick: move |_| props.on_open_combat_log.call(start),
                        i { class: "fa-solid fa-list" }
                        " Open in Combat Log"
                    }
                }
            }
        }
    }
}

#[component]
fn RecapBody(recap: DeathRecap, european: bool) -> Element {
    let r = &recap;
    let eu = european;
    let fmt = move |n: i64| formatting::format_compact(n, eu);

    rsx! {
        div { class: "recap-content",
            // Killing blow verdict
            if r.killing_blow.is_none() {
                div { class: "recap-verdict",
                    span { class: "recap-verdict-label", "Killed by " }
                    span { class: "recap-verdict-ability", "UNKNOWN" }
                }
            }
            if let Some(kb) = r.killing_blow.clone() {
                div { class: "recap-verdict",
                    span { class: "recap-verdict-label", "Killed by " }
                    span { class: "recap-verdict-ability", "{kb.ability_name}" }
                    if !kb.source_name.is_empty() {
                        span { class: "recap-verdict-source", " ({kb.source_name})" }
                    }
                    if kb.amount > 0 {
                        span { class: "recap-verdict-dmg",
                            if kb.is_crit {
                                " — {fmt(kb.amount as i64)}*"
                            } else {
                                " — {fmt(kb.amount as i64)}"
                            }
                        }
                    }
                    if !kb.damage_type.is_empty() {
                        span { class: "recap-dmg-type", "{kb.damage_type}" }
                    }
                    if kb.overkill > 0 {
                        span { class: "recap-verdict-overkill", " ({fmt(kb.overkill as i64)} overkill)" }
                    }
                }
            }

            // Summary stat chips
            div { class: "recap-stats",
                div { class: "recap-stat",
                    span { class: "recap-stat-value stat-damage", "{fmt(r.damage_total)}" }
                    span { class: "recap-stat-label", "damage taken" }
                }
                div { class: "recap-stat",
                    span { class: "recap-stat-value stat-heal", "{fmt(r.healing_total)}" }
                    span { class: "recap-stat-label", "healing received" }
                }
                div { class: "recap-stat",
                    span { class: "recap-stat-value stat-absorb", "{fmt(r.absorbed_total)}" }
                    span { class: "recap-stat-label", "absorbed" }
                }
            }

            // HP sparkline over the recap window
            {hp_sparkline(r)}

            // Damage taken / healing received tables
            div { class: "recap-tables",
                DamageTakenTable { rows: r.damage_rows.clone(), european: eu }
                HealingReceivedTable { rows: r.healing_rows.clone(), european: eu }
            }

            // Event ledger — final events with running HP%
            div { class: "recap-ledger-wrap",
                h5 { "Final Events" }
                div { class: "recap-ledger",
                    for ev in r.events.iter().cloned() {
                        {event_row(&ev, r.death_time_secs, eu)}
                    }
                }
            }
        }
    }
}

/// Per-source damage taken; click a source row to expand its ability breakdown.
#[component]
fn DamageTakenTable(rows: Vec<DeathRecapDamageSource>, european: bool) -> Element {
    let mut expanded = use_signal(|| None::<i64>);
    let fmt = move |n: i64| formatting::format_compact(n, european);

    rsx! {
        div { class: "recap-table-wrap",
            h5 { "Damage Taken" }
            if rows.is_empty() {
                div { class: "recap-empty", "No damage taken in window" }
            } else {
                table { class: "recap-table",
                    thead {
                        tr {
                            th { class: "col-left", "Source" }
                            th { "Hits" }
                            th { "Total" }
                            th { "Max" }
                            th { "Absorbed" }
                            th { class: "col-left", "Type" }
                        }
                    }
                    tbody {
                        for src in rows.iter().take(8).cloned() {
                            {
                                let id = src.source_id;
                                let is_open = *expanded.read() == Some(id);
                                let caret = if is_open { "fa-caret-down" } else { "fa-caret-right" };
                                rsx! {
                                    tr {
                                        class: "recap-row-source",
                                        onclick: move |_| expanded.set((!is_open).then_some(id)),
                                        td { class: "col-left",
                                            i { class: "recap-caret fa-solid {caret}" }
                                            span { class: "recap-ability", "{src.source_name}" }
                                        }
                                        td { "{src.hits}" }
                                        td { class: "stat-damage", "{fmt(src.total)}" }
                                        td { "{fmt(src.max_hit as i64)}" }
                                        td { class: "stat-absorb", "{fmt(src.absorbed)}" }
                                        td {}
                                    }
                                    if is_open {
                                        for ab in src.abilities.iter().cloned() {
                                            tr { class: "recap-row-ability",
                                                td { class: "col-left", "{ab.ability_name}" }
                                                td { "{ab.hits}" }
                                                td { class: "stat-damage", "{fmt(ab.total)}" }
                                                td { "{fmt(ab.max_hit as i64)}" }
                                                td { class: "stat-absorb", "{fmt(ab.absorbed)}" }
                                                td { class: "col-left recap-dmg-type-cell",
                                                    if ab.damage_type.is_empty() { "-" } else { "{ab.damage_type}" }
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

/// Per-healer healing received; click a source row to expand its ability breakdown.
#[component]
fn HealingReceivedTable(rows: Vec<DeathRecapHealSource>, european: bool) -> Element {
    let mut expanded = use_signal(|| None::<i64>);
    let fmt = move |n: i64| formatting::format_compact(n, european);

    rsx! {
        div { class: "recap-table-wrap",
            h5 { "Healing Received" }
            if rows.is_empty() {
                div { class: "recap-empty", "No healing received in window" }
            } else {
                table { class: "recap-table",
                    thead {
                        tr {
                            th { class: "col-left", "Source" }
                            th { "Ticks" }
                            th { "Effective" }
                        }
                    }
                    tbody {
                        for src in rows.iter().take(8).cloned() {
                            {
                                let id = src.source_id;
                                let is_open = *expanded.read() == Some(id);
                                let caret = if is_open { "fa-caret-down" } else { "fa-caret-right" };
                                rsx! {
                                    tr {
                                        class: "recap-row-source",
                                        onclick: move |_| expanded.set((!is_open).then_some(id)),
                                        td { class: "col-left",
                                            i { class: "recap-caret fa-solid {caret}" }
                                            span { class: "recap-ability", "{src.source_name}" }
                                        }
                                        td { "{src.casts}" }
                                        td { class: "stat-heal", "{fmt(src.effective)}" }
                                    }
                                    if is_open {
                                        for ab in src.abilities.iter().cloned() {
                                            tr { class: "recap-row-ability",
                                                td { class: "col-left", "{ab.ability_name}" }
                                                td { "{ab.casts}" }
                                                td { class: "stat-heal", "{fmt(ab.effective)}" }
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

/// Render the HP% sparkline as inline SVG (no charting library needed).
fn hp_sparkline(r: &DeathRecap) -> Element {
    if r.hp_timeline.len() < 2 {
        return rsx! {};
    }
    let t0 = r.window_start_secs;
    let span = (r.death_time_secs - t0).max(0.001);
    let x = move |t: f32| ((t - t0) / span * (SPARK_W - SPARK_PAD * 2.0) + SPARK_PAD).max(0.0);
    // Top edge = 100% HP, bottom edge = 0 — no vertical dead space
    let y = move |pct: f32| SPARK_H - (pct.clamp(0.0, 100.0) / 100.0) * SPARK_H;

    // Step function anchored at the left edge — HP holds its value between
    // samples, so each point draws a horizontal hold then a vertical jump
    let mut prev_y = y(r.hp_timeline[0].hp_pct);
    let mut line_pts = format!("{:.1},{:.1} ", SPARK_PAD, prev_y);
    for p in &r.hp_timeline {
        let px = x(p.time_secs);
        let py = y(p.hp_pct);
        line_pts.push_str(&format!("{px:.1},{prev_y:.1} {px:.1},{py:.1} "));
        prev_y = py;
    }
    // Closed polygon for the area fill under the line
    let area_pts = format!(
        "{:.1},{SPARK_H} {}{:.1},{SPARK_H}",
        SPARK_PAD,
        line_pts,
        x(r.death_time_secs),
    );
    let death_x = x(r.death_time_secs);
    // Death marker only spans the final HP drop, not the full chart height
    let death_top = y(r
        .hp_timeline
        .iter()
        .rev()
        .nth(1)
        .map(|p| p.hp_pct)
        .unwrap_or(0.0));
    let window_label = format!("{}s", (r.death_time_secs - t0).round() as i64);
    // Ticks every TICK_SECS counting back from the death: (x in viewBox, label)
    let ticks: Vec<(f32, String)> = (0..)
        .map(|k| (k, r.death_time_secs - k as f32 * TICK_SECS))
        .take_while(|&(_, t)| t >= t0 - 0.01)
        .map(|(k, t)| (x(t), if k == 0 { "death".to_string() } else { format!("-{}s", k * TICK_SECS as i32) }))
        .collect();

    rsx! {
        div { class: "recap-sparkline-wrap", title: "HP over the final {window_label}",
            // Y-axis is absolute 0-100% HP
            span { class: "spark-axis-label spark-axis-100", "100" }
            span { class: "spark-axis-label spark-axis-0", "0" }
            svg {
                class: "recap-sparkline",
                view_box: "0 0 {SPARK_W} {SPARK_H}",
                preserve_aspect_ratio: "none",
                for (tx, _) in ticks.iter() {
                    line {
                        class: "spark-tick",
                        x1: "{tx}",
                        y1: "0",
                        x2: "{tx}",
                        y2: "{SPARK_H}",
                        vector_effect: "non-scaling-stroke",
                    }
                }
                polygon { points: "{area_pts}", fill: "rgba(95, 217, 142, 0.12)" }
                polyline {
                    points: "{line_pts}",
                    fill: "none",
                    stroke: "#5fd98e",
                    stroke_width: "2",
                }
                line {
                    x1: "{death_x}",
                    y1: "{death_top}",
                    x2: "{death_x}",
                    y2: "{SPARK_H}",
                    stroke: "#ff4a4a",
                    stroke_width: "1.5",
                }
            }
            div { class: "recap-sparkline-labels",
                for (i, (tx, label)) in ticks.iter().enumerate() {
                    span {
                        class: if i == 0 { "spark-tick-label stat-damage" } else { "spark-tick-label" },
                        style: "left: {tx / SPARK_W * 100.0:.2}%",
                        "{label}"
                    }
                }
            }
        }
    }
}

/// One row in the event ledger.
fn event_row(ev: &DeathRecapEvent, death_time: f32, european: bool) -> Element {
    // Negative = before death; trailing events (damage logged after the
    // death event) show as positive
    let rel = ev.time_secs - death_time;
    let time_label = if matches!(ev.kind, DeathRecapEventKind::Death) || rel.abs() < 0.05 {
        "0.0s".to_string()
    } else if rel > 0.0 {
        format!("+{}s", formatting::format_decimal(rel, 1, european))
    } else {
        format!("-{}s", formatting::format_decimal(-rel, 1, european))
    };

    let (icon, icon_class) = match ev.kind {
        DeathRecapEventKind::Damage => ("fa-solid fa-burst", "ledger-icon stat-damage"),
        DeathRecapEventKind::Heal => ("fa-solid fa-heart", "ledger-icon stat-heal"),
        DeathRecapEventKind::Death => ("fa-solid fa-skull", "ledger-icon stat-damage"),
    };

    let detail = match ev.kind {
        DeathRecapEventKind::Damage => {
            if ev.avoided && ev.value == 0 {
                "avoided".to_string()
            } else {
                let mut s = format!("-{}", formatting::format_compact(ev.value as i64, european));
                if ev.is_crit {
                    s.push('*');
                }
                if ev.absorbed > 0 {
                    s.push_str(&format!(
                        " ({} absorbed)",
                        formatting::format_compact(ev.absorbed as i64, european)
                    ));
                }
                s
            }
        }
        DeathRecapEventKind::Heal => {
            format!("+{}", formatting::format_compact(ev.value as i64, european))
        }
        DeathRecapEventKind::Death => "DIED".to_string(),
    };
    let detail_class = match ev.kind {
        DeathRecapEventKind::Damage | DeathRecapEventKind::Death => "ledger-detail stat-damage",
        DeathRecapEventKind::Heal => "ledger-detail stat-heal",
    };
    let hp_label = format!("{}%", ev.hp_pct.round() as i64);
    let hp_width = format!("{}%", ev.hp_pct.clamp(0.0, 100.0));

    rsx! {
        div { class: "ledger-row",
            span { class: "ledger-time", "{time_label}" }
            i { class: "{icon_class} {icon}" }
            span { class: "ledger-ability",
                "{ev.ability_name}"
                if !ev.source_name.is_empty() {
                    span { class: "recap-source", " — {ev.source_name}" }
                }
            }
            span { class: "{detail_class}",
                if !ev.damage_type.is_empty() && !(ev.avoided && ev.value == 0) {
                    span { class: "recap-dmg-type", "{ev.damage_type}" }
                }
                "{detail}"
            }
            span { class: "ledger-hp",
                span { class: "ledger-hp-bar",
                    span { class: "ledger-hp-fill", style: "width: {hp_width}" }
                }
                span { class: "ledger-hp-label", "{hp_label}" }
            }
        }
    }
}

/// Footer navigation: `‹ 2/3 ›` steps through this player's deaths (hidden
/// for a single death); the outer buttons jump to the previous/next player by
/// first death, so next walks a wipe in the order it happened
fn recap_nav(deaths: &[PlayerDeath], name: &str, time: f32, on_select: EventHandler<PlayerDeath>) -> Element {
    let Some(cur) = deaths.iter().find(|d| d.name == name && d.death_time_secs == time) else {
        return rsx! {};
    };
    let same = |a: &PlayerDeath, b: &PlayerDeath| a.name == b.name && a.faction == b.faction;
    let own: Vec<&PlayerDeath> = deaths.iter().filter(|d| same(d, cur)).collect();
    let k = own.iter().position(|d| d.death_time_secs == time).unwrap_or(0);
    let mut firsts: Vec<&PlayerDeath> = Vec::new();
    for d in deaths {
        if !firsts.iter().any(|f| same(f, d)) {
            firsts.push(d);
        }
    }
    let p = firsts.iter().position(|f| same(f, cur)).unwrap_or(0);

    // Icon before the label when going back, after it when going forward
    let btn = |target: Option<&PlayerDeath>, icon: &'static str, label: String, title: &'static str, back: bool| {
        let target = target.cloned();
        rsx! {
            button {
                class: "btn",
                title: "{title}",
                disabled: target.is_none(),
                onclick: move |_| if let Some(d) = target.clone() { on_select.call(d) },
                if back { i { class: "fa-solid {icon}" } }
                if !label.is_empty() { span { "{label}" } }
                if !back { i { class: "fa-solid {icon}" } }
            }
        }
    };
    let player_btn = |i: Option<usize>, back: bool| {
        let d = i.and_then(|i| firsts.get(i)).copied();
        let label = d.map_or(String::new(), |d| d.name.clone());
        let (icon, title) = if back { ("fa-backward-step", "Previous player to die") } else { ("fa-forward-step", "Next player to die") };
        btn(d, icon, label, title, back)
    };
    rsx! {
        div { class: "recap-nav",
            if firsts.len() > 1 {
                {player_btn(p.checked_sub(1), true)}
            }
            if own.len() > 1 {
                {btn(k.checked_sub(1).and_then(|i| own.get(i)).copied(), "fa-chevron-left", String::new(), "Previous death", true)}
                span { class: "recap-nav-count", "Death {k + 1}/{own.len()}" }
                {btn(own.get(k + 1).copied(), "fa-chevron-right", String::new(), "Next death", false)}
            }
            if firsts.len() > 1 {
                {player_btn(Some(p + 1), false)}
            }
        }
    }
}
