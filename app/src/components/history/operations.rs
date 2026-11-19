//! Operations sub-tab: a rail of recent runs and areas, the selected area's
//! run table, and the recap of the run picked from either.

use dioxus::prelude::*;

use baras_types::history::{BackfillEvent, PullFilter, RunSummary};

use super::Backfill;
use super::overview::short_ts;
use super::scroll::{save_scroll, use_restore_scroll};
use super::toolbar::HistoryToolbar;
use super::run_detail::{RunDetailView, fmt_secs};
use crate::api;
use crate::components::toggle_switch::ToggleSwitch;
use crate::components::{ToastSeverity, use_toast};
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
pub(super) fn tier_label(tier: &str, size: u8, raw: &str) -> String {
    match (tier, size) {
        ("", 0) => raw.to_string(),
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
                span { class: "{tier_class(&r.tier)}", {tier_label(&r.tier, r.group_size, &r.difficulty)} }
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
                        span { class: "run-badge cleared", title: "Every boss killed", i { class: "fa-solid fa-check" } " Clear" }
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

/// Column the area's run table is sorted by
#[derive(Clone, Copy, PartialEq)]
enum RunSort {
    Date,
    Result,
    Pulls,
    Deaths,
    Time,
}

/// Every run of one area under the filters; a row opens the run's recap
#[component]
fn RunTable(operation: String, runs: Vec<RunSummary>, on_pick: EventHandler<RunKey>) -> Element {
    let mut sort = use_signal(|| (RunSort::Date, true));
    let (key, desc) = *sort.read();
    let mut rows: Vec<&RunSummary> = runs.iter().collect();
    rows.sort_by(|a, b| {
        let ord = match key {
            RunSort::Date => a.start_ms.cmp(&b.start_ms),
            RunSort::Result => (a.cleared, a.kills).cmp(&(b.cleared, b.kills)),
            RunSort::Pulls => a.pulls.cmp(&b.pulls),
            RunSort::Deaths => a.deaths.cmp(&b.deaths),
            RunSort::Time => a.timer.elapsed_secs.cmp(&b.timer.elapsed_secs),
        };
        if desc { ord.reverse() } else { ord }
    });
    let mut on_sort = move |k: RunSort| sort.set(if key == k { (k, !desc) } else { (k, true) });
    let sort_class = move |k: RunSort| match (key == k, desc) {
        (false, _) => "sortable-header",
        (true, true) => "sortable-header sorted desc",
        (true, false) => "sortable-header sorted asc",
    };
    rsx! {
        div { class: "pull-history-header",
            div { class: "pull-history-title",
                h3 { "{operation}" }
                span { class: "pull-history-operation", "{runs.len()} runs" }
                span { class: "pull-history-operation", "· Click a row to view its details" }
            }
        }
        div { class: "pull-history-table-wrap",
            table { class: "pull-history-table",
                thead {
                    tr {
                        th { class: sort_class(RunSort::Date), onclick: move |_| on_sort(RunSort::Date), "Date" }
                        th { "Character" }
                        th { "Difficulty" }
                        th { class: sort_class(RunSort::Result), onclick: move |_| on_sort(RunSort::Result), "Result" }
                        th { class: sort_class(RunSort::Pulls), onclick: move |_| on_sort(RunSort::Pulls), "Pulls" }
                        th { class: sort_class(RunSort::Deaths), onclick: move |_| on_sort(RunSort::Deaths), "Deaths" }
                        th { class: sort_class(RunSort::Time), onclick: move |_| on_sort(RunSort::Time), "Time" }
                        th { "Tags" }
                    }
                }
                tbody {
                    for r in rows {
                        {
                            let pick = key_of(r);
                            rsx! {
                                tr {
                                    key: "{r.filename}:{r.start_ms}",
                                    class: if r.hidden { "run-row pull-hidden" } else { "run-row" },
                                    onclick: move |_| on_pick.call(pick.clone()),
                                    td { {short_ts(&r.started)} }
                                    td {
                                        span { class: "pull-discipline", title: "{r.discipline.clone().unwrap_or_default()}",
                                            if let Some(icon) = r.role_icon.as_deref().and_then(get_role_icon) {
                                                img { class: "role-icon", src: *icon, alt: "" }
                                            }
                                            if let Some(icon) = r.discipline_icon.as_deref().and_then(get_class_icon) {
                                                img { class: "discipline-icon", src: *icon, alt: "" }
                                            }
                                            "{r.character}"
                                        }
                                    }
                                    td { span { class: "{tier_class(&r.tier)}", {tier_label(&r.tier, r.group_size, &r.difficulty)} } }
                                    td {
                                        if r.cleared {
                                            span { class: "run-badge cleared", title: "Every boss killed", i { class: "fa-solid fa-check" } " Clear" }
                                        } else if r.bosses_total > 0 {
                                            "{r.kills}/{r.bosses_total}"
                                        } else {
                                            "{r.kills} kills"
                                        }
                                    }
                                    td { "{r.pulls}" }
                                    td { "{r.deaths}" }
                                    td {
                                        span {
                                            class: if r.timer.achieved == Some(true) { "run-time ok" } else { "run-time" },
                                            title: if r.timer.achieved == Some(true) { "Timed run achieved" } else if r.cleared { "Time to the final kill" } else { "Time to the last pull" },
                                            {fmt_secs(r.timer.elapsed_secs)}
                                        }
                                    }
                                    td {
                                        for t in r.tags.iter() {
                                            span { class: "run-tag small", "{t}" }
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

#[component]
pub fn OperationsTab(props: OperationsProps) -> Element {
    let mut filter = props.filter;
    let mut toast = use_toast();
    let mut selected = use_signal(|| props.state.read().history.selected_run.clone());
    let mut search = use_signal(|| props.state.read().history.run_search.clone());
    // The area whose run table fills the main pane while no run is open
    let mut area = use_signal(|| props.state.read().history.run_area.clone());
    // The newest runs across every operation, folded until asked for
    let mut recent_open = use_signal(|| false);
    let mut state = props.state;
    use_effect(move || {
        let mut s = state.write();
        s.history.selected_run = selected.read().clone();
        s.history.run_search = search.read().clone();
        s.history.run_area = area.read().clone();
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
    // Where "back" leads: the open run's own area
    let current_area = current
        .as_ref()
        .and_then(|k| visible.iter().find(|r| key_of(r) == *k))
        .map(|r| r.operation.clone());
    let open_area = area.read().clone();
    let area_runs: Vec<RunSummary> =
        visible.iter().filter(|r| Some(&r.operation) == open_area.as_ref()).map(|r| (*r).clone()).collect();

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
                    div { class: "history-controls",
                        ToggleSwitch {
                            label: "Include flashpoints",
                            class: "boss-filter-toggle",
                            checked: filter.read().flashpoints,
                            on_change: move |checked: bool| {
                                filter.write().flashpoints = checked;
                                // Remembered across restarts, like "Bosses Only"
                                spawn(async move {
                                    if let Some(mut cfg) = api::get_config().await {
                                        cfg.raid_summaries_include_flashpoints = checked;
                                        if let Err(err) = api::update_config(&cfg).await {
                                            toast.show(format!("Failed to save settings: {err}"), ToastSeverity::Normal);
                                        }
                                    }
                                });
                            },
                        }
                    }
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
                                            // The open run's area stays marked so the way back is visible
                                            let active = current_area.as_deref().or(open_area.as_deref()) == Some(*op);
                                            rsx! {
                                                div {
                                                    class: if active { "sidebar-encounter-item run-area selected" } else { "sidebar-encounter-item run-area" },
                                                    onclick: move |_| {
                                                        area.set(Some(op_key.clone()));
                                                        selected.set(None);
                                                    },
                                                    div { class: "encounter-main",
                                                        span { class: "encounter-name", "{op}" }
                                                        span { class: "section-count", "{group.len()}" }
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
                if let Some(op) = current_area {
                    div { class: "run-back",
                        button {
                            class: "btn btn-sm btn-ghost",
                            title: "Back to the {op} runs",
                            onclick: move |_| {
                                area.set(Some(op.clone()));
                                selected.set(None);
                            },
                            i { class: "fa-solid fa-chevron-left" }
                            " Return to Table"
                        }
                    }
                    RunDetailView {
                        detail,
                        european: props.european,
                        known_tags,
                        on_change: move |_| {
                            list.restart();
                            detail.restart();
                        },
                    }
                } else if let Some(op) = open_area.filter(|_| !area_runs.is_empty()) {
                    RunTable { operation: op, runs: area_runs, on_pick: move |k| selected.set(Some(k)) }
                } else if !loading {
                    div { class: "sidebar-empty",
                        p { "Select an operation or a recent run" }
                    }
                }
            }
        }
    }
}
