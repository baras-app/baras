//! Dummy Parses: every training dummy kill, by discipline. Abandoned parses
//! never reach this view, and hidden ones never count. One fetch of compact rows; the cards, the
//! discipline table and the parse list are derived from them locally.

use dioxus::prelude::*;

use baras_types::formatting;
use baras_types::history::{BackfillEvent, DummyRow, keeps};
use baras_types::percentile_label;

use super::Backfill;
use super::dummy_chart::DummyTrend;
use super::overview::short_ts;
use super::scroll::use_paged;
use crate::api;
use crate::components::class_icons::{get_class_icon, get_role_icon};
use crate::components::multi_select::{MultiSelect, discipline_entries, plain_entries, toggle_value};
use crate::components::overview_table::pct_tier;
use crate::components::toggle_switch::ToggleSwitch;
use crate::components::use_file_opener;

const TABLE_ID: &str = "hist-dummy-table";

/// Empty = any. Dummy sizes are matched on their formatted HP.
#[derive(Clone, PartialEq, Default)]
struct DummyFilter {
    characters: Vec<String>,
    disciplines: Vec<String>,
    sizes: Vec<String>,
}

impl DummyFilter {
    fn matches(&self, r: &DummyRow, european: bool) -> bool {
        keeps(&self.characters, &r.character)
            && (self.disciplines.is_empty() || r.discipline.as_ref().is_some_and(|d| self.disciplines.contains(d)))
            && keeps(&self.sizes, &size_label(r.dummy_hp, european))
    }

    fn active(&self) -> usize {
        [&self.characters, &self.disciplines, &self.sizes]
            .iter()
            .filter(|v| !v.is_empty())
            .count()
    }
}

#[derive(Clone, Copy, PartialEq)]
enum DummySort {
    Date,
    Dps,
    Apm,
    Ttk,
}

fn size_label(hp: i64, european: bool) -> String {
    if hp > 0 {
        formatting::format_compact(hp, european)
    } else {
        "Unknown".into()
    }
}

fn distinct(it: impl Iterator<Item = String>) -> Vec<String> {
    let mut v: Vec<String> = it.filter(|s| !s.is_empty()).collect();
    v.sort();
    v.dedup();
    v
}

/// One discipline's kills, folded
struct DisciplineLine<'a> {
    name: &'a str,
    role_icon: Option<&'a str>,
    icon: Option<&'a str>,
    kills: Vec<&'a DummyRow>,
}

impl DisciplineLine<'_> {
    fn best(&self) -> &DummyRow {
        self.kills.iter().max_by_key(|r| r.dps).expect("a line has a kill")
    }

    fn avg(&self, value: impl Fn(&DummyRow) -> f32) -> f32 {
        self.kills.iter().map(|r| value(r)).sum::<f32>() / self.kills.len() as f32
    }
}

/// Most kills first; parses without a detected discipline are left out
fn by_discipline(rows: &[DummyRow]) -> Vec<DisciplineLine<'_>> {
    let mut out: Vec<DisciplineLine> = Vec::new();
    for r in rows {
        let Some(name) = r.discipline.as_deref() else { continue };
        match out.iter_mut().find(|d| d.name == name) {
            Some(d) => d.kills.push(r),
            None => out.push(DisciplineLine {
                name,
                role_icon: r.role_icon.as_deref(),
                icon: r.discipline_icon.as_deref(),
                kills: vec![r],
            }),
        }
    }
    out.sort_by(|a, b| b.kills.len().cmp(&a.kills.len()));
    out
}

/// The cards' numbers over the counted parses
struct Stats {
    best_dps: Option<i64>,
    avg_dps: Option<i64>,
    avg_apm: Option<f32>,
    fastest: Option<i64>,
}

impl Stats {
    fn of(rows: &[DummyRow]) -> Self {
        let n = rows.len();
        Self {
            best_dps: rows.iter().map(|r| r.dps).max(),
            avg_dps: (n > 0).then(|| rows.iter().map(|r| r.dps).sum::<i64>() / n as i64),
            avg_apm: (n > 0).then(|| rows.iter().map(|r| r.apm).sum::<f32>() / n as f32),
            fastest: rows.iter().map(|r| r.duration_seconds).min(),
        }
    }
}

/// DPS with its Parsely percentile, when rated
fn dps_cell(r: &DummyRow, european: bool) -> Element {
    rsx! {
        td {
            {formatting::format_compact(r.dps, european)}
            if let Some(pct) = r.dps_pct {
                " "
                span { class: "pct-badge {pct_tier(pct)}", title: "{percentile_label(pct)}", "{pct}" }
            }
        }
    }
}

fn discipline_cell(role_icon: Option<&str>, icon: Option<&str>, name: &str) -> Element {
    rsx! {
        td {
            span { class: "pull-discipline",
                if let Some(icon) = role_icon.and_then(get_role_icon) {
                    img { class: "role-icon", src: *icon, alt: "" }
                }
                if let Some(icon) = icon.and_then(get_class_icon) {
                    img { class: "discipline-icon", src: *icon, alt: "" }
                }
                "{name}"
            }
        }
    }
}

#[component]
pub fn DummyTab(european: bool, backfill: Backfill) -> Element {
    let eu = european;
    let opener = use_file_opener();
    let mut filter = use_signal(DummyFilter::default);
    let paged = use_paged("dummy");
    let mut sort_key = use_signal(|| DummySort::Date);
    let mut sort_desc = use_signal(|| true);
    let mut show_hidden = use_signal(|| false);

    let mut parses = use_resource(|| async move { api::get_dummy_parses().await.unwrap_or_default() });
    // A live parse just ended, or a re-scan finished
    use_effect(move || {
        if matches!(*backfill.event.read(), Some(BackfillEvent::Done { .. })) || *backfill.history.read() > 0 {
            parses.restart();
        }
    });

    let all = parses.read().clone().unwrap_or_default();
    let loaded = parses.read().is_some();
    let f = filter.read().clone();
    // Hidden parses are listed (dimmed) on request but never counted
    let (hidden, kills): (Vec<DummyRow>, Vec<DummyRow>) =
        all.iter().filter(|r| f.matches(r, eu)).cloned().partition(|r| r.hidden);
    let lines = by_discipline(&kills);
    let counted = kills.len();
    let stats = Stats::of(&kills);
    let mut rows = kills.clone();
    if show_hidden() {
        rows.extend(hidden);
    }
    rows.sort_by(|a, b| {
        let ord = match *sort_key.read() {
            DummySort::Date => a.timestamp.cmp(&b.timestamp),
            DummySort::Dps => a.dps.cmp(&b.dps),
            DummySort::Apm => a.apm.total_cmp(&b.apm),
            DummySort::Ttk => a.duration_seconds.cmp(&b.duration_seconds),
        };
        if *sort_desc.read() { ord.reverse() } else { ord }
    });
    let mut on_sort = move |key: DummySort| {
        if *sort_key.read() == key {
            sort_desc.toggle();
        } else {
            sort_key.set(key);
            // Fastest kill first; everything else largest or newest first
            sort_desc.set(key != DummySort::Ttk);
        }
    };
    let sort_class = move |k: DummySort| {
        if *sort_key.read() != k {
            "sortable-header"
        } else if *sort_desc.read() {
            "sortable-header sorted desc"
        } else {
            "sortable-header sorted asc"
        }
    };

    let characters = distinct(all.iter().map(|r| r.character.clone()));
    let disciplines = distinct(all.iter().filter_map(|r| r.discipline.clone()));
    let sizes = distinct(all.iter().map(|r| size_label(r.dummy_hp, eu)));

    let total = rows.len();
    let limit = paged.limit("dummy");
    let dec = |v: f32| formatting::format_decimal(v, 1, eu);
    let pct = |v: f32| format!("{} %", formatting::format_decimal(v, 1, eu));
    let Stats { best_dps, avg_dps, avg_apm, fastest } = stats;
    let compact = |v: Option<i64>| {
        v.map(|v| formatting::format_compact(v, eu))
            .unwrap_or_else(|| "—".into())
    };

    // Compute in its own statement: a `filter.read()` temporary inside the call
    // arguments would outlive into the `filter.write()` and panic
    let mut pick = move |field: fn(&mut DummyFilter) -> &mut Vec<String>, v: Vec<String>| {
        *field(&mut filter.write()) = v;
        paged.show_all("");
    };
    let mut pick_disc = move |name: String| {
        let v = toggle_value(&filter.read().disciplines, &name, true);
        pick(|f| &mut f.disciplines, v);
    };

    rsx! {
        div { class: "pull-history-main pvp-main",
            div { class: "pvp-head",
                div { class: "history-toolbar",
                    MultiSelect {
                        any_label: "All characters",
                        noun: "characters",
                        entries: plain_entries(&characters),
                        picked: f.characters.clone(),
                        on_change: move |v| pick(|f| &mut f.characters, v),
                    }
                    MultiSelect {
                        any_label: "Any discipline",
                        noun: "disciplines",
                        title: "Mirror disciplines count as one",
                        entries: discipline_entries(&disciplines),
                        picked: f.disciplines.clone(),
                        on_change: move |v| pick(|f| &mut f.disciplines, v),
                    }
                    if sizes.len() > 1 || !f.sizes.is_empty() {
                        MultiSelect {
                            any_label: "Any dummy HP",
                            noun: "dummy sizes",
                            title: "DPS and time to kill only compare between dummies of the same HP",
                            entries: plain_entries(&sizes),
                            picked: f.sizes.clone(),
                            on_change: move |v| pick(|f| &mut f.sizes, v),
                        }
                    }
                    button {
                        class: if f.active() > 0 { "btn btn-xs history-clear active" } else { "btn btn-xs history-clear" },
                        disabled: f.active() == 0,
                        title: "Clear filters (X)",
                        "data-shortcut": "x",
                        onclick: move |_| filter.set(DummyFilter::default()),
                        i { class: "fa-solid fa-filter-circle-xmark" } " Clear filters"
                    }
                    ToggleSwitch {
                        label: "Show hidden",
                        title: "List the parses you hid (dimmed); they stay out of the numbers",
                        class: "history-switch",
                        checked: show_hidden(),
                        on_change: move |checked: bool| show_hidden.set(checked),
                    }
                    span { class: "pull-history-count", "{counted} of {all.len()} kills" }
                }
            }

            if loaded && all.is_empty() {
                div { class: "pvp-empty",
                    i { class: "fa-solid fa-bullseye" }
                    p { "No training dummy kills recorded yet." }
                    p { class: "hint", "Only parses that kill the dummy are listed. Logs scanned before this view existed need one re-scan." }
                    button { class: "btn btn-sm", disabled: backfill.running(), onclick: move |_| backfill.start(true),
                        i { class: "fa-solid fa-rotate" } " Scan logs"
                    }
                }
            } else {
                div { class: "history-cards",
                    div { class: "history-card", div { class: "k", "Kills" } div { class: "v", "{counted}" } }
                    div { class: "history-card", div { class: "k", "Best DPS" } div { class: "v", {compact(best_dps)} } }
                    div { class: "history-card", div { class: "k", "Avg DPS" } div { class: "v", {compact(avg_dps)} } }
                    div { class: "history-card", div { class: "k", "Avg APM" } div { class: "v", {avg_apm.map(dec).unwrap_or_else(|| "—".into())} } }
                    div { class: "history-card", title: "Fastest kill",
                        div { class: "k", "Best TTK" }
                        div { class: "v", {fastest.map(formatting::format_duration).unwrap_or_else(|| "—".into())} }
                    }
                }

                DummyTrend { rows: kills.clone(), european: eu }

                if lines.len() > 1 {
                    div { class: "discipline-breakdown discipline-breakdown-table",
                        table { class: "pull-history-table",
                            thead {
                                tr {
                                    th { "Discipline" }
                                    th { "Kills" }
                                    th { title: "Highest DPS on a kill, with its Parsely percentile", "Best DPS" }
                                    th { "Avg DPS" }
                                    th { "Avg APM" }
                                    th { title: "Share of damaging hits that crit", "Avg Crit" }
                                    th { title: "Share of damage dealt by crits", "Avg Crit Dmg" }
                                    th { "Best TTK" }
                                }
                            }
                            tbody {
                                for d in lines.iter() {
                                    {
                                        let name = d.name.to_string();
                                        let fastest = d.kills.iter().map(|r| r.duration_seconds).min().unwrap_or(0);
                                        rsx! {
                                            tr { key: "{d.name}", class: "run-row", title: "Show only this discipline",
                                                onclick: move |_| pick_disc(name.clone()),
                                                {discipline_cell(d.role_icon, d.icon, d.name)}
                                                td { "{d.kills.len()}" }
                                                {dps_cell(d.best(), eu)}
                                                td { {formatting::format_compact(d.avg(|r| r.dps as f32) as i64, eu)} }
                                                td { {dec(d.avg(|r| r.apm))} }
                                                td { {pct(d.avg(|r| r.crit_pct))} }
                                                td { {pct(d.avg(|r| r.crit_damage_pct))} }
                                                td { {formatting::format_duration(fastest)} }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                div { id: TABLE_ID, class: "pull-history-table-wrap", onscroll: move |_| paged.on_scroll(TABLE_ID, "dummy", total),
                    table { class: "pull-history-table",
                        thead {
                            tr {
                                th { class: sort_class(DummySort::Date), onclick: move |_| on_sort(DummySort::Date), "Date" }
                                th { "Character" }
                                th { "Discipline" }
                                th { "Dummy HP" }
                                th { class: sort_class(DummySort::Ttk), onclick: move |_| on_sort(DummySort::Ttk), "TTK" }
                                th { class: sort_class(DummySort::Dps), onclick: move |_| on_sort(DummySort::Dps), "DPS" }
                                th { class: sort_class(DummySort::Apm), onclick: move |_| on_sort(DummySort::Apm), "APM" }
                                th { "Crit" }
                                th { "Crit Dmg" }
                                th {}
                            }
                        }
                        tbody {
                            for r in rows.iter().take(limit) {
                                {
                                    let path = r.path.clone();
                                    let encounter_id = r.encounter_id;
                                    let (filename, start_ms, is_hidden) = (r.filename.clone(), r.start_ms, r.hidden);
                                    rsx! {
                                        tr { key: "{r.filename}:{r.start_ms}",
                                            class: if path.is_some() { "run-row" },
                                            class: if is_hidden { "pull-hidden" },
                                            onclick: {
                                                let path = path.clone();
                                                move |_| {
                                                    if let Some(path) = path.clone() && !opener.busy() {
                                                        opener.open(path, Some(encounter_id));
                                                    }
                                                }
                                            },
                                            td { {short_ts(&r.timestamp)} }
                                            td { "{r.character}" }
                                            {discipline_cell(r.role_icon.as_deref(), r.discipline_icon.as_deref(), r.discipline.as_deref().unwrap_or("-"))}
                                            td { {size_label(r.dummy_hp, eu)} }
                                            td { {formatting::format_duration(r.duration_seconds)} }
                                            {dps_cell(r, eu)}
                                            td { {dec(r.apm)} }
                                            td { {pct(r.crit_pct)} }
                                            td { {pct(r.crit_damage_pct)} }
                                            td { class: "pull-row-actions", onclick: move |e| e.stop_propagation(),
                                                if let Some(path) = path.clone() {
                                                    button {
                                                        class: "btn btn-xs btn-ghost",
                                                        title: "Open in Analysis",
                                                        disabled: opener.busy(),
                                                        onclick: move |_| opener.open(path.clone(), Some(encounter_id)),
                                                        i { class: "fa-solid fa-magnifying-glass-chart" }
                                                    }
                                                } else {
                                                    span { class: "pull-log-missing", title: "Log file no longer on disk", i { class: "fa-solid fa-file-circle-xmark" } }
                                                }
                                                button {
                                                    class: "btn btn-xs btn-ghost",
                                                    title: if is_hidden { "Count this parse again" } else { "Hide this parse (wrong gear, botched opener) from the numbers" },
                                                    onclick: move |_| {
                                                        let filename = filename.clone();
                                                        spawn(async move {
                                                            if api::set_pull_hidden(&filename, start_ms, !is_hidden).await.is_ok() {
                                                                parses.restart();
                                                            }
                                                        });
                                                    },
                                                    i { class: if is_hidden { "fa-solid fa-eye" } else { "fa-solid fa-eye-slash" } }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if limit < total {
                        div { class: "history-more",
                            span { class: "history-more-count", "Showing {limit} of {total}" }
                            button { class: "btn btn-xs", onclick: move |_| paged.show_all("dummy"), "Show all" }
                        }
                    }
                }
            }
        }
    }
}
