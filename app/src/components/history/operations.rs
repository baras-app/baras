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

/// Runs listed under "Recent" at the top of the rail
const RECENT: usize = 6;

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

/// The run to show: the user's pick while it is still listed
fn effective(selected: Option<RunKey>, runs: &[&RunSummary]) -> Option<RunKey> {
    selected.filter(|k| runs.iter().any(|r| key_of(r) == *k))
}

/// One run in the rail; `show_operation` names the operation (Recent list)
#[component]
fn RunItem(run: RunSummary, selected: bool, on_pick: EventHandler<RunKey>, #[props(default)] show_operation: bool) -> Element {
    let r = &run;
    let key = key_of(r);
    let date = r.started.get(..10).unwrap_or(&r.started);
    rsx! {
        div {
            class: if selected { "sidebar-encounter-item run-item selected" } else { "sidebar-encounter-item run-item" },
            onclick: move |_| on_pick.call(key.clone()),
            if show_operation {
                div { class: "run-item-op", "{r.operation}" }
            }
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
                    if r.hidden {
                        span { class: "run-badge", title: "Hidden run", i { class: "fa-solid fa-eye-slash" } }
                    }
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

#[component]
pub fn OperationsTab(props: OperationsProps) -> Element {
    let filter = props.filter;
    let mut selected = use_signal(|| props.state.read().history.selected_run.clone());
    let mut search = use_signal(|| props.state.read().history.run_search.clone());
    // Operations the user has opened in the rail (collapsed by default, like the Overview tree)
    let mut expanded = use_signal(|| props.state.read().history.run_expanded.clone());
    // Operations whose every run is listed; others show the newest PAGE
    let mut unfolded = use_signal(std::collections::HashSet::<String>::new);
    // The newest runs across every operation, open by default
    let mut recent_open = use_signal(|| true);
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
    // A pull just landed in a history file: the newest run may have grown or appeared
    use_effect(move || {
        if *backfill.history.read() > 0 {
            list.restart();
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
    // Operations / Flashpoints, each split by operation
    let groups: Vec<(usize, Vec<(&str, Vec<&RunSummary>)>)> = (0..super::CONTENT_GROUPS.len())
        .map(|g| {
            let in_group: Vec<&RunSummary> = visible.iter().copied().filter(|r| super::content_group(!r.is_flashpoint, r.is_flashpoint) == g).collect();
            (g, group_by_operation(&in_group))
        })
        .filter(|(_, ops)| !ops.is_empty())
        .collect();
    let closed_groups = use_signal(std::collections::HashSet::<usize>::new);
    use_restore_scroll(&[OPS_RAIL, OPS_MAIN], !loading && (current.is_none() || detail.read().is_some()));

    let collapsed = props.state.read().history.sidebar_collapsed;
    rsx! {
        div { class: "pull-history-body",
            aside { class: if collapsed { "explorer-sidebar collapsed" } else { "explorer-sidebar" },
                super::SidebarHeader { state: props.state, icon: "fa-solid fa-dungeon", title: "Runs" }
                if !collapsed {
                    HistoryToolbar { filter, runs: true }
                    div { class: "sidebar-search",
                        input {
                            class: "search-input",
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
                                p { class: "hint", "Widen the filters, or re-scan your logs from the Logs tab." }
                            }
                        }
                        if !visible.is_empty() && query.is_empty() {
                            {
                                let chevron = if *recent_open.read() { "fa-chevron-down" } else { "fa-chevron-right" };
                                rsx! {
                                    div { class: "sidebar-section-header run-recent-header", onclick: move |_| recent_open.toggle(),
                                        i { class: "fa-solid {chevron} collapse-icon" }
                                        span { class: "section-area", i { class: "fa-solid fa-clock-rotate-left" } " Recent" }
                                        span { class: "section-count", "{visible.len().min(RECENT)}" }
                                    }
                                    if *recent_open.read() {
                                        for r in visible.iter().take(RECENT) {
                                            RunItem { run: (*r).clone(), selected: current.as_ref() == Some(&key_of(r)), on_pick: move |k| selected.set(Some(k)), show_operation: true }
                                        }
                                    }
                                }
                            }
                        }
                        for (g, ops) in groups.iter() {
                            super::ContentGroupHeader { group: *g, count: ops.iter().map(|(_, runs)| runs.len()).sum::<usize>(), closed: closed_groups }
                            // A search opens every group so hits are visible
                            if !query.is_empty() || !closed_groups.read().contains(g) {
                                div { class: "sidebar-group-body",
                                    for (op, group) in ops.iter() {
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
                                                        RunItem { run: (*r).clone(), selected: current.as_ref() == Some(&key_of(r)), on_pick: move |k| selected.set(Some(k)) }
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
