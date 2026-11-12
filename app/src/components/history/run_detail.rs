//! One operation run: headline cards, bosses in order, per-player deaths and
//! averages, the user's tags and note, and the pull list.

use dioxus::prelude::*;

use baras_types::formatting;
use baras_types::history::{PullRow, RunDetail, RunMeta};

use super::operations::{tier_class, tier_label};
use super::overview::{result_badge, short_ts};
use crate::api;
use crate::components::class_icons::{get_class_icon, get_role_icon};
use crate::components::{ToastSeverity, use_file_opener, use_toast};

pub(super) fn fmt_secs(secs: i64) -> String {
    formatting::format_duration(secs.max(0))
}

fn clock(iso: &str) -> &str {
    iso.get(11..16).unwrap_or(iso)
}

/// Where the time went: one bar from the clock start (timer line, else first
/// pull) to the last pull's end. Pulls are coloured by result; the gaps
/// between them are the muted stretches (trash, breaks, running back).
#[component]
fn RunTimeline(detail: RunDetail) -> Element {
    let run = &detail.run;
    // The clock's own start only when a start line was crossed
    let timed = (!run.timer.from_first_pull).then_some(&run.timer);
    let start_ms = timed
        .and_then(|t| baras_types::history::iso_to_ms(&t.started))
        .unwrap_or(run.start_ms);
    let end_ms = detail
        .pulls
        .iter()
        .map(|p| p.start_ms + p.duration_seconds * 1000)
        .max()
        .unwrap_or(start_ms);
    let total = (end_ms - start_ms).max(1) as f64;
    let pct = move |ms: i64| ms as f64 * 100.0 / total;

    // Alternate gap / pull segments, chronological
    let mut segs: Vec<(f64, &'static str, String)> = Vec::new();
    let mut cursor = start_ms;
    for p in &detail.pulls {
        if p.start_ms > cursor {
            let gap = p.start_ms - cursor;
            segs.push((pct(gap), "gap", format!("Between pulls · {}", fmt_secs(gap / 1000))));
        }
        let dur = p.duration_seconds * 1000;
        let class = if p.success { "kill" } else { "wipe" };
        let what = if p.success { "Kill" } else { "Wipe" };
        segs.push((pct(dur), class, format!("{} · {what} · {}", p.boss, fmt_secs(p.duration_seconds))));
        cursor = (p.start_ms + dur).max(cursor);
    }

    // Elapsed-time ticks: the coarsest step that still gives a few labels.
    // A span past the largest step (bad timestamps) still gets ~8 ticks.
    let total_secs = (end_ms - start_ms) / 1000;
    let step = [60, 300, 600, 900, 1800, 3600, 7200]
        .into_iter()
        .find(|s| total_secs / s <= 8)
        .unwrap_or_else(|| (total_secs / 8).max(1));
    let ticks: Vec<(f64, String)> = (0..=total_secs / step)
        .map(|i| (pct(i * step * 1000), fmt_secs(i * step)))
        .collect();
    let clock_start = timed.map_or(run.started.as_str(), |t| t.started.as_str());

    rsx! {
        div { class: "run-timeline",
            div { class: "run-timeline-bar",
                for (i, (w, class, title)) in segs.iter().enumerate() {
                    div { key: "{i}", class: "run-seg {class}", style: "flex-basis: {w}%", title: "{title}" }
                }
            }
            div { class: "run-timeline-ticks",
                for (left, label) in ticks.iter() {
                    span { class: "run-tick", style: "left: {left}%", "{label}" }
                }
            }
            div { class: "run-timeline-axis",
                span { "{clock(clock_start)} → {clock(&run.ended)} · {fmt_secs(total_secs)} total" }
                span { class: "run-timeline-legend",
                    span { class: "run-seg kill" } " kill "
                    span { class: "run-seg wipe" } " wipe "
                    span { class: "run-seg gap" } " between pulls"
                }
            }
        }
    }
}

#[component]
pub fn RunDetailView(
    detail: Resource<Option<RunDetail>>,
    european: bool,
    known_tags: Vec<String>,
    on_change: EventHandler<()>,
) -> Element {
    let eu = european;
    let opener = use_file_opener();
    let mut toast = use_toast();
    let mut tag_input = use_signal(String::new);
    // Bosses whose pulls are unfolded (all folded by default)
    let mut open_bosses = use_signal(std::collections::HashSet::<String>::new);

    let detail_read = detail.read();
    let Some(d) = detail_read.as_ref().and_then(|d| d.as_ref()) else {
        return rsx! {
            div { class: "sidebar-empty",
                if detail_read.is_none() { p { "Loading…" } } else { p { "Select a run" } }
            }
        };
    };
    let run = &d.run;
    let hidden = run.hidden;

    // Saving tags or the note: the backend holds the meta, so send the whole thing
    let save_meta = {
        let filename = run.filename.clone();
        let start_ms = run.start_ms;
        move |meta: RunMeta| {
            let filename = filename.clone();
            spawn(async move {
                match api::set_run_meta(&filename, start_ms, &meta).await {
                    Ok(true) => on_change.call(()),
                    _ => toast.show("Could not save run tags", ToastSeverity::Normal),
                }
            });
        }
    };
    let add_tag = {
        let tags = run.tags.clone();
        let note = run.note.clone();
        let save = save_meta.clone();
        move |value: String| {
            let value = value.trim().to_string();
            tag_input.set(String::new());
            if value.is_empty() || tags.contains(&value) {
                return;
            }
            let mut tags = tags.clone();
            tags.push(value);
            save(RunMeta { tags, note: note.clone(), hidden });
        }
    };

    let between = run.span_secs - run.pull_secs;
    let pull_pct = if run.span_secs > 0 { run.pull_secs * 100 / run.span_secs } else { 0 };
    let fmt_opt = move |n: i64| if n > 0 { formatting::format_compact(n, eu) } else { "-".into() };
    let fmt_pct = move |p: f32| formatting::format_decimal(p, 1, eu);
    let date = short_ts(&run.started);

    rsx! {
        div { class: "pull-history-header",
            div { class: "pull-history-title",
                h3 { "{run.operation}" }
                span { class: "{tier_class(&run.tier)}", {tier_label(run)} }
                span { class: "pull-history-operation", "{date} · {run.character}" }
                if !run.server.is_empty() || !run.game_version.is_empty() {
                    span { class: "pull-history-operation", title: "Server · game patch at the time",
                        {[run.server.as_str(), run.game_version.as_str()].iter().filter(|s| !s.is_empty()).copied().collect::<Vec<_>>().join(" · ")}
                    }
                }
            }
            div { class: "run-header-actions",
                {
                    let (tags, note, save) = (run.tags.clone(), run.note.clone(), save_meta.clone());
                    rsx! {
                        button {
                            class: "btn btn-sm",
                            title: if hidden { "Show this run in Raid Summaries again" } else { "Hide this run from Raid Summaries; its pulls stay in the statistics" },
                            onclick: move |_| save(RunMeta { tags: tags.clone(), note: note.clone(), hidden: !hidden }),
                            i { class: if hidden { "fa-solid fa-eye" } else { "fa-solid fa-eye-slash" } }
                            if hidden { " Unhide" } else { " Hide run" }
                        }
                    }
                }
                if let Some(path) = run.path.clone() {
                    button {
                        class: "btn btn-sm btn-primary",
                        title: "Open this log in Encounter Analysis",
                        disabled: opener.busy(),
                        onclick: move |_| opener.open(path.clone(), None),
                        i { class: "fa-solid fa-magnifying-glass-chart" }
                        " Open log"
                    }
                } else if run.archived {
                    button {
                        class: "btn btn-sm btn-primary",
                        title: "This log is archived: restore it to the log directory, then open it in Encounter Analysis",
                        disabled: opener.busy(),
                        onclick: {
                            let filename = run.filename.clone();
                            move |_| opener.restore_and_open(filename.clone(), None)
                        },
                        i { class: "fa-solid fa-box-open" }
                        " Restore & open"
                    }
                } else {
                    span { class: "pull-history-operation", title: "The log file was deleted; its pulls stay in the statistics",
                        i { class: "fa-solid fa-file-circle-xmark" }
                        " Log missing"
                    }
                }
            }
        }

        if hidden {
            div { class: "history-pick",
                i { class: "fa-solid fa-eye-slash" }
                " This run is hidden. It only shows while \"Show hidden\" (More filters) is on; its pulls still count in the statistics."
            }
        }
        if run.area_id == 0 {
            div { class: "history-pick",
                i { class: "fa-solid fa-triangle-exclamation" }
                " This log's history predates area ids, so boss order, clear status and the timed run are unknown. Re-scan it from the Logs tab."
            }
        }
        div { class: "history-cards",
            div { class: "history-card", div { class: "k", "Pulls" } div { class: "v", "{run.pulls}" } }
            div { class: "history-card", div { class: "k", "Bosses killed" }
                div { class: if run.cleared { "v ok" } else { "v" },
                    if run.bosses_total > 0 { "{run.kills}/{run.bosses_total}" } else { "{run.kills}" }
                }
                div { class: "sub", if run.cleared { "Cleared" } else { "Not cleared" } }
            }
            div { class: "history-card", div { class: "k", "In pulls" }
                div { class: "v", {fmt_secs(run.pull_secs)} }
                div { class: "sub", "{pull_pct} % of {fmt_secs(run.span_secs)}" }
            }
            div { class: "history-card", div { class: "k", "Between pulls" }
                div { class: "v", {fmt_secs(between)} }
            }
            {
                let t = &run.timer;
                let achieved = t.achieved == Some(true);
                rsx! {
                    div {
                        class: if achieved { "history-card ok" } else { "history-card" },
                        title: if t.from_first_pull { "No start line crossing was logged; the clock runs from the first pull" } else { "Clock from the timer start line" },
                        div { class: "k", i { class: "fa-solid fa-stopwatch" } " Time" }
                        div { class: if achieved { "v ok" } else { "v" }, {fmt_secs(t.elapsed_secs)} }
                        div { class: "sub",
                            match t.limit_secs {
                                Some(l) if achieved => format!("under {}", fmt_secs(i64::from(l))),
                                Some(l) if run.kills < run.bosses_total => format!("{}/{} bosses · limit {}", run.kills, run.bosses_total, fmt_secs(i64::from(l))),
                                Some(l) => format!("over {}", fmt_secs(i64::from(l))),
                                None if run.cleared => "to the final kill".to_string(),
                                None => "to the last pull".to_string(),
                            }
                        }
                    }
                }
            }
        }

        RunTimeline { detail: d.clone() }

        div { class: "run-tags",
            i { class: "fa-solid fa-tag", title: "Tags, e.g. your prog team" }
            for t in run.tags.iter() {
                {
                    let remove = {
                        let tags: Vec<String> = run.tags.iter().filter(|x| *x != t).cloned().collect();
                        let note = run.note.clone();
                        let save = save_meta.clone();
                        move |_| save(RunMeta { tags: tags.clone(), note: note.clone(), hidden })
                    };
                    rsx! {
                        span { class: "run-tag", "{t}"
                            button { title: "Remove tag", onclick: remove, i { class: "fa-solid fa-xmark" } }
                        }
                    }
                }
            }
            input {
                class: "run-tag-input",
                r#type: "text",
                list: "run-tag-options",
                placeholder: "Add tag…",
                value: "{tag_input}",
                oninput: move |e| tag_input.set(e.value()),
                onchange: {
                    let mut add = add_tag.clone();
                    move |e| add(e.value())
                },
                onkeydown: {
                    let mut add = add_tag.clone();
                    move |e| {
                        if e.key() == Key::Enter {
                            // Copy out first: `add` clears the signal, and a live read guard would panic
                            let value = tag_input.peek().clone();
                            add(value);
                        }
                    }
                },
            }
            datalist { id: "run-tag-options",
                for t in known_tags.iter().filter(|t| !run.tags.contains(*t)) {
                    option { value: "{t}" }
                }
            }
        }
        div { class: "run-note-row",
            input {
                class: "run-note",
                r#type: "text",
                maxlength: "140",
                placeholder: "One-line summary of this run (140 characters)",
                value: "{run.note}",
                onchange: {
                    let tags = run.tags.clone();
                    let save = save_meta.clone();
                    move |e| save(RunMeta { tags: tags.clone(), note: e.value().trim().chars().take(140).collect(), hidden })
                },
            }
            span { class: "run-note-count", "{run.note.chars().count()}/140" }
        }

        div { class: "run-section",
            h4 { "Players" }
            div { class: "pull-history-table-wrap",
                table { class: "pull-history-table",
                    thead { tr { th { "Player" } th { "Pulls" } th { "Deaths" } th { title: "Died first in the pull", "First" } th { "Avg DPS" } th { "Avg HPS" } th { "Avg EHPS" } } }
                    tbody {
                        for p in d.players.iter() {
                            tr { key: "{p.name}",
                                class: if p.name == run.character { "run-player-me" } else { "" },
                                td {
                                    span { class: "pull-discipline",
                                        // Every discipline played in the run, first played first
                                        for spec in p.specs.iter() {
                                            span { class: "run-player-spec", title: "{spec.discipline}",
                                                if let Some(icon) = get_role_icon(&spec.role_icon) {
                                                    img { class: "role-icon", src: *icon, alt: "" }
                                                }
                                                if let Some(icon) = get_class_icon(&spec.discipline_icon) {
                                                    img { class: "discipline-icon", src: *icon, alt: "" }
                                                }
                                            }
                                        }
                                        "{p.name}"
                                    }
                                }
                                td { "{p.pulls}" }
                                td { "{p.deaths}" }
                                td { "{p.first_deaths}" }
                                td { {fmt_opt(p.avg_dps)} }
                                td { {fmt_opt(p.avg_hps)} }
                                td { {fmt_opt(p.avg_ehps)} }
                            }
                        }
                    }
                }
            }
        }

        // Bosses in order; each one opens onto its pulls
        div { class: "run-section",
            h4 { "Bosses" }
            div { class: "pull-history-table-wrap",
                table { class: "pull-history-table run-boss-table",
                    thead { tr { th { "" } th { "#" } th { "Boss" } th { "Pulls" } th { "Result" } th { "Deaths" } } }
                    tbody {
                        for (i, b) in d.bosses.iter().enumerate() {
                            {
                                let is_open = !b.skipped && open_bosses.read().contains(&b.name);
                                let toggle_name = b.name.clone();
                                let pulls: Vec<(usize, &PullRow)> = d.pulls.iter().enumerate().filter(|(_, p)| p.boss == b.name).collect();
                                rsx! {
                                    tr { key: "{b.name}",
                                        class: if b.skipped { "run-boss-row" } else if is_open { "run-boss-row expandable open" } else { "run-boss-row expandable" },
                                        onclick: move |_| {
                                            let mut set = open_bosses.write();
                                            if !set.remove(&toggle_name) {
                                                set.insert(toggle_name.clone());
                                            }
                                        },
                                        td { class: "run-boss-caret",
                                            if !b.skipped {
                                                i { class: if is_open { "fa-solid fa-chevron-down" } else { "fa-solid fa-chevron-right" } }
                                            }
                                        }
                                        td { "{i + 1}" }
                                        td { class: if b.skipped { "run-boss-skipped" } else { "run-boss-name" }, "{b.name}" }
                                        td { if b.skipped { "—" } else { "{b.pulls}" } }
                                        td {
                                            if b.killed {
                                                span { class: "pull-result kill", "Kill" }
                                                if let Some(s) = b.kill_secs { span { class: "sub", " at {fmt_secs(s)}" } }
                                            } else if b.skipped {
                                                span { class: "run-boss-skipped", "Not pulled" }
                                            } else {
                                                span { class: "pull-result wipe",
                                                    match (b.best_hp_pct, b.phase.as_deref()) {
                                                        (Some(p), Some(ph)) => format!("Best {} % · {ph}", fmt_pct(p)),
                                                        (Some(p), None) => format!("Best {} %", fmt_pct(p)),
                                                        (None, Some(ph)) => format!("Best {ph}"),
                                                        (None, None) => "No kill".to_string(),
                                                    }
                                                }
                                            }
                                        }
                                        td { if b.skipped { "—" } else { "{b.deaths}" } }
                                    }
                                    if is_open {
                                        tr { key: "{b.name}-pulls", class: "run-boss-pulls",
                                            td { colspan: "6",
                                                table { class: "pull-history-table run-pull-subtable",
                                                    thead { tr { th { "Time" } th { "Result" } th { "Phase" } th { "Duration" } th { "Deaths" } th { "First death" } th { "" } } }
                                                    tbody {
                                                        for (idx, row) in pulls {
                                                            {
                                                                let (label, class) = result_badge(row, eu);
                                                                let encounter_id = row.encounter_id;
                                                                let path = row.path.clone();
                                                                let archived = row.archived.then(|| row.filename.clone());
                                                                let first = d.first_deaths.get(idx).cloned().flatten();
                                                                rsx! {
                                                                    tr { key: "{row.encounter_id}",
                                                                        td { {short_ts(&row.timestamp)} }
                                                                        td { span { class: "{class}", "{label}" } }
                                                                        td { {row.phase.clone().unwrap_or_else(|| "-".into())} }
                                                                        td { {formatting::format_duration(row.duration_seconds)} }
                                                                        td { "{row.deaths}" }
                                                                        td { class: "run-first-death", {first.unwrap_or_else(|| "-".into())} }
                                                                        td {
                                                                            if let Some(path) = path {
                                                                                button {
                                                                                    class: "btn btn-xs btn-ghost",
                                                                                    title: "Open this pull in Encounter Analysis",
                                                                                    disabled: opener.busy(),
                                                                                    onclick: move |_| opener.open(path.clone(), Some(encounter_id)),
                                                                                    i { class: "fa-solid fa-magnifying-glass-chart" }
                                                                                }
                                                                            } else if let Some(filename) = archived {
                                                                                button {
                                                                                    class: "btn btn-xs btn-ghost",
                                                                                    title: "The log is archived: restore it, then open this pull in Encounter Analysis",
                                                                                    disabled: opener.busy(),
                                                                                    onclick: move |_| opener.restore_and_open(filename.clone(), Some(encounter_id)),
                                                                                    i { class: "fa-solid fa-box-open" }
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
                        }
                    }
                }
            }
        }
    }
}
