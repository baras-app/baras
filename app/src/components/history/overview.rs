//! Overview sub-tab: Operation → Boss tree, per-boss header cards and the
//! enriched pull table (result with HP %, phase reached, deaths).

use dioxus::prelude::*;

use baras_types::formatting;
use baras_types::history::{BossNode, PullEnd, PullFilter, PullRow, PullSortKey as SortKey};

use super::Backfill;
use super::charts::{ChartPick, PullCharts};
use crate::api;
use crate::components::class_icons::{get_class_icon, get_role_icon};
use crate::components::use_file_opener;
use crate::types::UiSessionState;

#[derive(Props, Clone, PartialEq)]
pub struct OverviewProps {
    pub european: bool,
    pub state: Signal<UiSessionState>,
    pub filter: Signal<PullFilter>,
    pub backfill: Backfill,
}

/// Tree selection: (operation, boss)
type BossKey = (String, String);

fn short_ts(iso: &str) -> String {
    iso.get(..16).unwrap_or(iso).replacen('T', " ", 1)
}

/// The boss to show: the user's pick if it still exists, else the first boss
fn effective_selection(selected: Option<BossKey>, bosses: &[BossNode]) -> Option<BossKey> {
    selected
        .filter(|(op, boss)| bosses.iter().any(|n| n.operation == *op && n.boss == *boss))
        .or_else(|| bosses.first().map(|n| (n.operation.clone(), n.boss.clone())))
}

/// Group consecutive nodes (already sorted by operation) into tree sections
fn group_by_operation<'a>(bosses: impl Iterator<Item = &'a BossNode>) -> Vec<(&'a str, Vec<&'a BossNode>)> {
    let mut tree: Vec<(&'a str, Vec<&'a BossNode>)> = Vec::new();
    for node in bosses {
        match tree.last_mut() {
            Some((op, nodes)) if *op == node.operation => nodes.push(node),
            _ => tree.push((&node.operation, vec![node])),
        }
    }
    tree
}

/// Result badge: label and CSS class for one pull
fn result_badge(row: &PullRow, eu: bool) -> (String, &'static str) {
    match row.end_reason {
        PullEnd::Kill => ("Kill".into(), "pull-result kill"),
        PullEnd::Wipe | PullEnd::Unknown => {
            let label = match row.boss_hp_pct {
                Some(pct) => format!("Wipe · {} %", formatting::format_decimal(pct, 1, eu)),
                None => "Wipe".into(),
            };
            (label, "pull-result wipe")
        }
        PullEnd::Reset => ("Reset".into(), "pull-result reset"),
        PullEnd::LogEnd => ("Cut off".into(), "pull-result reset"),
    }
}

#[component]
pub fn OverviewTab(props: OverviewProps) -> Element {
    let eu = props.european;
    let filter = props.filter;
    let opener = use_file_opener();

    let mut selected = use_signal(|| props.state.read().history.selected.clone());
    // Chart click → table narrowed to that pull or night; cleared with the boss
    let mut pick = use_signal(|| None::<ChartPick>);
    use_effect(move || {
        let _ = selected.read();
        pick.set(None);
    });
    let mut expanded = use_signal(|| props.state.read().history.expanded.clone());
    let mut boss_search = use_signal(|| props.state.read().history.boss_search.clone());
    let mut sort_key = use_signal(|| props.state.read().history.sort_key);
    let mut sort_desc = use_signal(|| props.state.read().history.sort_desc);
    let mut state = props.state;
    use_effect(move || {
        let mut s = state.write();
        s.history.selected = selected.read().clone();
        s.history.expanded = expanded.read().clone();
        s.history.boss_search = boss_search.read().clone();
        s.history.sort_key = *sort_key.read();
        s.history.sort_desc = *sort_desc.read();
    });

    let mut overview = use_resource(move || {
        let f = filter.read().clone();
        async move { api::get_pull_history_overview(&f).await.unwrap_or_default() }
    });
    // A finished re-scan changes every count
    let backfill = props.backfill;
    use_effect(move || {
        if matches!(*backfill.event.read(), Some(baras_types::history::BackfillEvent::Done { .. })) {
            overview.restart();
        }
    });

    let overview_read = overview.read();
    let loading = overview_read.is_none();
    let bosses: &[BossNode] = overview_read.as_ref().map(|o| o.bosses.as_slice()).unwrap_or(&[]);
    let search = boss_search.read().to_lowercase();
    let tree = group_by_operation(bosses.iter().filter(|n| {
        search.is_empty() || n.boss.to_lowercase().contains(&search) || n.operation.to_lowercase().contains(&search)
    }));
    let current = effective_selection(selected.read().clone(), bosses);
    let current_node = current
        .as_ref()
        .and_then(|(op, boss)| bosses.iter().find(|n| n.operation == *op && n.boss == *boss));

    // Reads signals (not render-local values) so the resource re-runs on change
    let pulls = use_resource(move || {
        let key = overview
            .read()
            .as_ref()
            .and_then(|o| effective_selection(selected.read().clone(), &o.bosses));
        let f = filter.read().clone();
        async move {
            let (op, boss) = key?;
            api::get_boss_pulls(&op, &boss, &f).await
        }
    });
    let pulls_read = pulls.read();
    let rows: &[PullRow] = pulls_read.as_ref().and_then(|p| p.as_deref()).unwrap_or(&[]);

    // Header cards
    let kills = rows.iter().filter(|r| r.success).count();
    let best = rows
        .iter()
        .filter(|r| !r.success)
        .filter_map(|r| r.boss_hp_pct.map(|p| (p, r)))
        .min_by(|a, b| a.0.total_cmp(&b.0));
    // Cleared: fastest kill. Otherwise: furthest progress (lowest HP, phase)
    let fastest_kill = rows.iter().filter(|r| r.success).map(|r| r.duration_seconds).min();
    let best_label = if let Some(secs) = fastest_kill {
        formatting::format_duration(secs)
    } else if let Some((pct, r)) = best {
        match &r.phase {
            Some(ph) => format!("{ph} · {} %", formatting::format_decimal(pct, 1, eu)),
            None => format!("{} %", formatting::format_decimal(pct, 1, eu)),
        }
    } else {
        "—".into()
    };
    let avg_secs = if rows.is_empty() { 0 } else { rows.iter().map(|r| r.duration_seconds).sum::<i64>() / rows.len() as i64 };
    let best_dps = rows.iter().filter_map(|r| r.dps).max();

    let key = *sort_key.read();
    let picked = pick.read().clone();
    let mut visible: Vec<&PullRow> = rows.iter().filter(|r| picked.as_ref().is_none_or(|p| p.matches(r))).collect();
    visible.sort_by(|a, b| {
        let ord = match key {
            SortKey::Date => a.timestamp.cmp(&b.timestamp),
            SortKey::Duration => a.duration_seconds.cmp(&b.duration_seconds),
            SortKey::Dps => a.dps.cmp(&b.dps),
            SortKey::Hps => a.hps.cmp(&b.hps),
        };
        if *sort_desc.read() { ord.reverse() } else { ord }
    });
    let mut on_sort = move |key: SortKey| {
        if *sort_key.read() == key {
            sort_desc.toggle();
        } else {
            sort_key.set(key);
            sort_desc.set(true);
        }
    };
    let sort_class = move |k: SortKey| {
        if *sort_key.read() != k {
            "sortable-header"
        } else if *sort_desc.read() {
            "sortable-header sorted desc"
        } else {
            "sortable-header sorted asc"
        }
    };
    let fmt_opt = move |n: Option<i64>| n.map(|v| formatting::format_compact(v, eu)).unwrap_or_else(|| "-".into());

    rsx! {
        div { class: "pull-history-body",
            aside { class: "explorer-sidebar",
                div { class: "sidebar-search",
                    input {
                        class: "boss-search-input",
                        r#type: "text",
                        placeholder: "Filter bosses...",
                        value: "{boss_search}",
                        oninput: move |e| boss_search.set(e.value()),
                    }
                }
                div { class: "sidebar-encounter-list",
                    if !loading && tree.is_empty() {
                        div { class: "sidebar-empty",
                            i { class: "fa-solid fa-clock-rotate-left" }
                            p { "No recorded pulls" }
                            p { class: "hint", "Widen the filters, or re-scan your logs from the Library tab." }
                        }
                    }
                    for (op, nodes) in tree.iter() {
                        {
                            let op_key = op.to_string();
                            let is_collapsed = search.is_empty() && !expanded.read().contains(*op);
                            let chevron = if is_collapsed { "fa-chevron-right" } else { "fa-chevron-down" };
                            let op_pulls: u32 = nodes.iter().map(|n| n.pulls).sum();
                            rsx! {
                                div {
                                    class: "sidebar-section-header",
                                    onclick: move |_| {
                                        let mut set = expanded.write();
                                        if !set.remove(&op_key) {
                                            set.insert(op_key.clone());
                                        }
                                    },
                                    i { class: "fa-solid {chevron} collapse-icon" }
                                    span { class: "section-area", "{op}" }
                                    span { class: "section-count", "{op_pulls}" }
                                }
                                if !is_collapsed {
                                    for node in nodes.iter() {
                                        {
                                            let key = (node.operation.clone(), node.boss.clone());
                                            let is_selected = current.as_ref() == Some(&key);
                                            rsx! {
                                                div {
                                                    class: if is_selected { "sidebar-encounter-item selected" } else { "sidebar-encounter-item" },
                                                    onclick: move |_| selected.set(Some(key.clone())),
                                                    div { class: "encounter-main",
                                                        span { class: "encounter-name", "{node.boss}" }
                                                        span { class: "pull-tree-count", title: "pulls · kills", "{node.pulls} · {node.kills}" }
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

            section { class: "pull-history-main",
                if let Some(node) = current_node {
                    div { class: "pull-history-header",
                        div { class: "pull-history-title",
                            h3 { "{node.boss}" }
                            span { class: "pull-history-operation", "{node.operation}" }
                        }
                    }
                    div { class: "history-cards",
                        div { class: "history-card", div { class: "k", "Pulls" } div { class: "v", "{rows.len()}" } }
                        div { class: "history-card", div { class: "k", "Kills" }
                            div { class: "v", "{kills} "
                                if !rows.is_empty() { span { class: "sub", "{kills * 100 / rows.len()} %" } }
                            }
                        }
                        div { class: "history-card", title: if fastest_kill.is_some() { "Fastest kill" } else { "Furthest progress" },
                            div { class: "k", if fastest_kill.is_some() { "Best kill" } else { "Best pull" } }
                            div { class: "v small", "{best_label}" }
                        }
                        div { class: "history-card", div { class: "k", "Avg pull" } div { class: "v", {formatting::format_duration(avg_secs)} } }
                        div { class: "history-card", div { class: "k", "Your best DPS" } div { class: "v", {fmt_opt(best_dps)} } }
                    }
                    PullCharts { pulls, pick, kills_only: filter.read().kills_only }
                    if let Some(p) = picked.as_ref() {
                        div { class: "history-pick",
                            i { class: "fa-solid fa-filter" }
                            " Showing {p.label(rows)} · {visible.len()} of {rows.len()} pulls"
                            button { class: "btn btn-xs btn-ghost", title: "Show every pull again", onclick: move |_| pick.set(None), i { class: "fa-solid fa-xmark" } " Clear" }
                        }
                    }
                    div { class: "pull-history-table-wrap",
                        table { class: "pull-history-table",
                            thead {
                                tr {
                                    th { class: sort_class(SortKey::Date), onclick: move |_| on_sort(SortKey::Date), "Date" }
                                    th { "Result" }
                                    th { "Phase" }
                                    th { class: sort_class(SortKey::Duration), onclick: move |_| on_sort(SortKey::Duration), "Duration" }
                                    th { "Deaths" }
                                    th { "Character" }
                                    th { "Discipline" }
                                    th { class: sort_class(SortKey::Dps), onclick: move |_| on_sort(SortKey::Dps), "DPS" }
                                    th { class: sort_class(SortKey::Hps), onclick: move |_| on_sort(SortKey::Hps), "HPS" }
                                    th { "Parsely" }
                                    th { "Log" }
                                }
                            }
                            tbody {
                                for row in visible.iter() {
                                    {
                                        let (label, class) = result_badge(row, eu);
                                        rsx! {
                                            tr {
                                                key: "{row.filename}:{row.start_ms}",
                                                td { {short_ts(&row.timestamp)} }
                                                td {
                                                    span { class: "{class}", "{label}" }
                                                    if row.partial { span { class: "pull-partial", title: "Log ended before combat did", " ⚠" } }
                                                }
                                                td { {row.phase.clone().unwrap_or_else(|| "-".into())} }
                                                td { {formatting::format_duration(row.duration_seconds)} }
                                                td { "{row.deaths}" }
                                                td { "{row.character}" }
                                                td {
                                                    span { class: "pull-discipline",
                                                        if let Some(icon) = row.role_icon.as_deref().and_then(get_role_icon) {
                                                            img { class: "role-icon", src: *icon, alt: "" }
                                                        }
                                                        if let Some(icon) = row.discipline_icon.as_deref().and_then(get_class_icon) {
                                                            img { class: "discipline-icon", src: *icon, alt: "" }
                                                        }
                                                        {row.discipline.clone().unwrap_or_else(|| "-".into())}
                                                    }
                                                }
                                                td { {fmt_opt(row.dps)} }
                                                td { {fmt_opt(row.hps)} }
                                                td {
                                                    if let Some(link) = row.parsely.clone() {
                                                        button {
                                                            class: "btn btn-xs btn-ghost",
                                                            title: "View on Parsely",
                                                            onclick: move |_| {
                                                                let link = link.clone();
                                                                spawn(async move { api::open_url(&link).await });
                                                            },
                                                            i { class: "fa-solid fa-up-right-from-square" }
                                                        }
                                                    } else {
                                                        span { class: "pull-log-missing", title: "Not uploaded to Parsely", "—" }
                                                    }
                                                }
                                                td {
                                                    if let Some(path) = row.path.clone() {
                                                        button {
                                                            class: "btn btn-xs btn-ghost",
                                                            title: "Open this pull in the Data Explorer",
                                                            disabled: opener.busy(),
                                                            onclick: {
                                                                let encounter_id = row.encounter_id;
                                                                move |_| opener.open(path.clone(), Some(encounter_id))
                                                            },
                                                            i { class: "fa-solid fa-eye" }
                                                        }
                                                    } else {
                                                        span { class: "pull-log-missing", title: "Log file no longer exists", "—" }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                } else if !loading {
                    div { class: "panel-placeholder",
                        i { class: "fa-solid fa-clock-rotate-left" }
                        p { "Select a boss to see its pulls." }
                    }
                }
            }
        }
    }
}
