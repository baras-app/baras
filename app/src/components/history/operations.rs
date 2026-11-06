//! Operations sub-tab: a rail of runs (one log, one area, one difficulty,
//! split after the final boss) and the selected run's recap.

use dioxus::prelude::*;

use baras_types::history::{BackfillEvent, PullFilter, RunSummary};

use super::Backfill;
use super::scroll::{PAGE, save_scroll, use_restore_scroll};
use super::toolbar::HistoryToolbar;
use super::run_detail::{RunDetailView, fmt_secs};
use crate::api;
use crate::components::class_icons::{get_class_icon, get_role_icon};
use crate::types::UiSessionState;

const OPS_RAIL: &str = "hist-ops-rail";
const OPS_MAIN: &str = "hist-ops-main";

#[derive(Props, Clone, PartialEq)]
pub struct OperationsProps {
    pub european: bool,
    pub state: Signal<UiSessionState>,
    pub filter: Signal<PullFilter>,
    pub backfill: Backfill,
}

/// Rail key: (filename, first pull start_ms)
type RunKey = (String, i64);

fn key_of(r: &RunSummary) -> RunKey {
    (r.filename.clone(), r.start_ms)
}

/// Tag class shared with the Library's content chips
pub(super) fn tier_class(tier: &str) -> &'static str {
    match tier {
        "Master" => "lib-tag nim",
        "Veteran" => "lib-tag hm",
        "Story" => "lib-tag sm",
        _ => "lib-tag misc",
    }
}

/// "Master 8" / "8-man" when the tier is unknown
pub(super) fn tier_label(r: &RunSummary) -> String {
    match (r.tier.as_str(), r.group_size) {
        ("", 0) => r.difficulty.clone(),
        ("", n) => format!("{n}-man"),
        (t, 0) => t.to_string(),
        (t, n) => format!("{t} {n}"),
    }
}

/// Rail search: tags first, then operation, character and date
fn run_hit(r: &RunSummary, q: &str) -> bool {
    q.is_empty()
        || r.tags.iter().any(|t| t.to_lowercase().contains(q))
        || r.operation.to_lowercase().contains(q)
        || r.character.to_lowercase().contains(q)
        || r.started.starts_with(q)
}

/// Consecutive runs of the same operation become one rail section
fn group_by_operation<'a>(runs: &[&'a RunSummary]) -> Vec<(&'a str, Vec<&'a RunSummary>)> {
    let mut groups: Vec<(&'a str, Vec<&'a RunSummary>)> = Vec::new();
    for r in runs {
        match groups.iter_mut().find(|(op, _)| *op == r.operation) {
            Some((_, g)) => g.push(r),
            None => groups.push((&r.operation, vec![r])),
        }
    }
    groups
}

/// The run to show: the user's pick if still listed, else the newest
fn effective(selected: Option<RunKey>, runs: &[&RunSummary]) -> Option<RunKey> {
    selected
        .filter(|k| runs.iter().any(|r| key_of(r) == *k))
        .or_else(|| runs.first().map(|r| key_of(r)))
}

#[component]
pub fn OperationsTab(props: OperationsProps) -> Element {
    let filter = props.filter;
    let mut selected = use_signal(|| props.state.read().history.selected_run.clone());
    let mut search = use_signal(|| props.state.read().history.run_search.clone());
    // Operations the user has opened in the rail (collapsed by default, like the Overview tree)
    let mut expanded = use_signal(|| props.state.read().history.run_expanded.clone());
    // Operations whose every run is listed; others show the newest PAGE
    let mut unfolded = use_signal(std::collections::HashSet::<String>::new);
    let mut state = props.state;
    use_effect(move || {
        let mut s = state.write();
        s.history.selected_run = selected.read().clone();
        s.history.run_search = search.read().clone();
        s.history.run_expanded = expanded.read().clone();
    });

    let mut list = use_resource(move || {
        let f = filter.read().clone();
        async move { api::get_operation_runs(&f).await.unwrap_or_default() }
    });
    let backfill = props.backfill;
    use_effect(move || {
        if matches!(*backfill.event.read(), Some(BackfillEvent::Done { .. })) {
            list.restart();
        }
    });

    // Nothing picked (first visit, or the pick fell out of the filter): take
    // the newest run and open its operation so the highlighted row is visible
    use_effect(move || {
        let read = list.read();
        let Some(l) = read.as_ref() else { return };
        let q = search.read().to_lowercase();
        let visible: Vec<&RunSummary> = l.runs.iter().filter(|r| run_hit(r, &q)).collect();
        let picked = selected.read().clone();
        if let Some(key) = effective(picked.clone(), &visible)
            && picked.as_ref() != Some(&key)
        {
            if let Some(run) = visible.iter().find(|r| key_of(r) == key) {
                expanded.write().insert(run.operation.clone());
            }
            selected.set(Some(key));
        }
    });

    let list_read = list.read();
    let loading = list_read.is_none();
    let runs: &[RunSummary] = list_read.as_ref().map(|l| l.runs.as_slice()).unwrap_or(&[]);
    let tags: &[String] = list_read.as_ref().map(|l| l.tags.as_slice()).unwrap_or(&[]);
    let query = search.read().to_lowercase();
    let visible: Vec<&RunSummary> = runs.iter().filter(|r| run_hit(r, &query)).collect();
    let current = effective(selected.read().clone(), &visible);

    // Reads signals, not render values, so it re-runs on selection and refresh
    let mut detail = use_resource(move || {
        let key = list.read().as_ref().and_then(|l| {
            let q = search.read().to_lowercase();
            let visible: Vec<&RunSummary> = l.runs.iter().filter(|r| run_hit(r, &q)).collect();
            effective(selected.read().clone(), &visible)
        });
        async move {
            let (filename, start_ms) = key?;
            api::get_operation_run(&filename, start_ms).await
        }
    });
    let known_tags = tags.to_vec();
    let groups = group_by_operation(&visible);
    use_restore_scroll(&[OPS_RAIL, OPS_MAIN], !loading && (current.is_none() || detail.read().is_some()));

    rsx! {
        div { class: "pull-history-body",
            aside { class: "explorer-sidebar",
                HistoryToolbar { filter, runs: true }
                div { class: "sidebar-search",
                    input {
                        class: "boss-search-input",
                        r#type: "text",
                        placeholder: "Search tags, operation, character…",
                        value: "{search}",
                        oninput: move |e| search.set(e.value()),
                    }
                }
                div { id: OPS_RAIL, class: "sidebar-encounter-list", onscroll: move |_| save_scroll(OPS_RAIL),
                    if !loading && visible.is_empty() {
                        div { class: "sidebar-empty",
                            i { class: "fa-solid fa-dungeon" }
                            p { "No operation runs" }
                            p { class: "hint", "Widen the filters, or re-scan your logs from Log Management." }
                        }
                    }
                    for (op, group) in groups.iter() {
                        {
                            let op_key = op.to_string();
                            // A search opens every group so hits are visible
                            let is_collapsed = query.is_empty() && !expanded.read().contains(*op);
                            let chevron = if is_collapsed { "fa-chevron-right" } else { "fa-chevron-down" };
                            let cap = if unfolded.read().contains(*op) { group.len() } else { group.len().min(PAGE) };
                            let unfold_key = op.to_string();
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
                                    span { class: "section-count", "{group.len()}" }
                                }
                                if !is_collapsed {
                                    for r in group[..cap].iter() {
                                        {
                                            let key = key_of(r);
                                            let is_selected = current.as_ref() == Some(&key);
                                            let date = r.started.get(..10).unwrap_or(&r.started);
                                            rsx! {
                                                div {
                                                    class: if is_selected { "sidebar-encounter-item run-item selected" } else { "sidebar-encounter-item run-item" },
                                    onclick: move |_| selected.set(Some(key.clone())),
                                    div { class: "encounter-main",
                                        span { class: "encounter-name run-who",
                                            if let Some(icon) = r.role_icon.as_deref().and_then(get_role_icon) {
                                                img { class: "role-icon", src: *icon, alt: "" }
                                            }
                                            if let Some(icon) = r.discipline_icon.as_deref().and_then(get_class_icon) {
                                                img { class: "discipline-icon", src: *icon, alt: "" }
                                            }
                                            span { title: "{r.discipline.clone().unwrap_or_default()}", "{r.character}" }
                                        }
                                        span { class: "{tier_class(&r.tier)}", {tier_label(r)} }
                                    }
                                    div { class: "run-item-meta",
                                        span { "{date}" }
                                        span { "·" }
                                        span {
                                            class: if r.timer.achieved == Some(true) { "run-time ok" } else { "run-time" },
                                            title: if r.cleared { "Time to the final kill" } else { "Time to the last pull" },
                                            {fmt_secs(r.timer.elapsed_secs)}
                                        }
                                        // Outcome sits at the right edge, under the tier badge. A full
                                        // clear replaces the kill count; partial runs show how far they got
                                        span { class: "run-outcome",
                                            // A missed timer shows nothing; only the achievement is worth a badge
                                            if r.timer.achieved == Some(true) {
                                                span { class: "run-badge timer-ok", title: "Timed run achieved", i { class: "fa-solid fa-stopwatch" } }
                                            }
                                            if r.cleared {
                                                span { class: "run-badge cleared", title: "Final boss killed", i { class: "fa-solid fa-check" } " Clear" }
                                            } else {
                                                span { class: "run-badge", title: "kills / bosses",
                                                    if r.bosses_total > 0 { "{r.kills}/{r.bosses_total}" } else { "{r.kills} kills" }
                                                }
                                            }
                                        }
                                    }
                                    if !r.tags.is_empty() {
                                        div { class: "run-item-meta",
                                            for t in r.tags.iter() {
                                                span { class: "run-tag small", "{t}" }
                                            }
                                        }
                                    }
                                }
                                            }
                                        }
                                    }
                                    if cap < group.len() {
                                        div { class: "sidebar-encounter-item run-more",
                                            onclick: move |_| { unfolded.write().insert(unfold_key.clone()); },
                                            i { class: "fa-solid fa-angles-down" }
                                            " Show {group.len() - cap} more"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            section { id: OPS_MAIN, class: "pull-history-main", onscroll: move |_| save_scroll(OPS_MAIN),
                RunDetailView {
                    detail,
                    european: props.european,
                    known_tags,
                    on_change: move |_| {
                        list.restart();
                        detail.restart();
                    },
                }
            }
        }
    }
}
