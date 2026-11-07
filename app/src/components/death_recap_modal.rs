//! Death Recap modal — summarizes the final seconds before a player death.
//!
//! Opened from the death tracker on the Overview tab. Loads lazily via
//! `query_death_recap` and renders a header verdict, HP sparkline, damage
//! taken / healing received tables, and a condensed event ledger.

use baras_types::{DeathRecapDamageSource, DeathRecapEvent, DeathRecapHealSource, formatting};
use dioxus::prelude::*;
use wasm_bindgen_futures::spawn_local as spawn;

use crate::api::{self, DeathRecap, DeathRecapEventKind};

/// Sparkline viewBox dimensions (scaled to container via CSS).
const SPARK_W: f32 = 600.0;
const SPARK_H: f32 = 72.0;
const SPARK_PAD: f32 = 3.0;

#[derive(Props, Clone, PartialEq)]
pub struct DeathRecapModalProps {
    pub encounter_idx: Option<u32>,
    pub player_name: String,
    pub death_time_secs: f32,
    pub european: bool,
    pub on_close: EventHandler<()>,
    /// Payload is the recap window start (combat seconds) for the log filter
    pub on_open_combat_log: EventHandler<f32>,
}

#[component]
pub fn DeathRecapModal(props: DeathRecapModalProps) -> Element {
    // None = loading, Some(None) = query failed/empty, Some(Some) = loaded
    let mut recap = use_signal(|| None::<Option<DeathRecap>>);

    use_effect({
        let name = props.player_name.clone();
        let idx = props.encounter_idx;
        let death_time = props.death_time_secs;
        move || {
            let name = name.clone();
            spawn(async move {
                let result = api::query_death_recap(idx, &name, death_time).await;
                recap.set(Some(result));
            });
        }
    });

    let eu = props.european;
    let time_str = formatting::format_duration(props.death_time_secs as i64);
    let state = recap.read().clone();
    let phase_chip = state
        .as_ref()
        .and_then(|o| o.as_ref())
        .and_then(|r| r.phase_name.clone());

    rsx! {
        div {
            class: "modal-backdrop",
            onclick: move |_| props.on_close.call(()),
            div {
                class: "death-recap-modal",
                onclick: move |e| e.stop_propagation(),
                div { class: "recap-header",
                    h3 {
                        i { class: "fa-solid fa-skull" }
                        " Death Recap: {props.player_name}"
                        span { class: "recap-header-time", " @ {time_str}" }
                        if let Some(phase) = phase_chip {
                            span { class: "recap-phase-chip", "{phase}" }
                        }
                    }
                    button {
                        class: "btn btn-close",
                        onclick: move |_| props.on_close.call(()),
                        "X"
                    }
                }
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
                                recap: r,
                                european: eu,
                                on_open_combat_log: props.on_open_combat_log,
                            }
                        },
                    }
                }
            }
        }
    }
}

#[component]
fn RecapBody(recap: DeathRecap, european: bool, on_open_combat_log: EventHandler<f32>) -> Element {
    let r = &recap;
    let eu = european;
    let fmt = move |n: i64| formatting::format_compact(n, eu);
    let window_start = r.window_start_secs;

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

            div { class: "recap-footer",
                button {
                    class: "btn",
                    onclick: move |_| on_open_combat_log.call(window_start),
                    i { class: "fa-solid fa-list" }
                    " Open in Combat Log"
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
    let window_label = format!("-{}s", (r.death_time_secs - t0).round() as i64);

    rsx! {
        div { class: "recap-sparkline-wrap",
            // Y-axis is absolute 0-100% HP
            span { class: "spark-axis-label spark-axis-100", "100" }
            span { class: "spark-axis-label spark-axis-0", "0" }
            svg {
                class: "recap-sparkline",
                view_box: "0 0 {SPARK_W} {SPARK_H}",
                preserve_aspect_ratio: "none",
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
                span { "{window_label}" }
                span { "HP over final {window_label.trim_start_matches('-')}" }
                span { class: "stat-damage", "death" }
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
