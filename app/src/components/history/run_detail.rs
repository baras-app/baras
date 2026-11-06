//! One operation run: headline cards, bosses in order, per-player deaths and
//! averages, the user's tags and note, and the pull list.

use dioxus::prelude::*;

use baras_types::formatting;
use baras_types::history::{RunDetail, RunMeta};

use super::operations::{tier_class, tier_label};
use super::overview::{result_badge, short_ts};
use crate::api;
use crate::components::class_icons::{get_class_icon, get_role_icon};
use crate::components::{ToastSeverity, use_file_opener, use_toast};

fn fmt_secs(secs: i64) -> String {
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
    let start_ms = run
        .timer
        .as_ref()
        .filter(|t| !t.from_first_pull)
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
        segs.push((pct(dur), class, format!("{} · {what} · {} · {}", p.boss, fmt_secs(p.duration_seconds), clock(&p.timestamp))));
        cursor = (p.start_ms + dur).max(cursor);
    }

    // Elapsed-time ticks: the coarsest step that still gives a few labels
    let total_secs = (end_ms - start_ms) / 1000;
    let step = [60, 300, 600, 900, 1800, 3600, 7200]
        .into_iter()
        .find(|s| total_secs / s <= 8)
        .unwrap_or(7200);
    let ticks: Vec<(f64, String)> = (0..=total_secs / step)
        .map(|i| (pct(i * step * 1000), fmt_secs(i * step)))
        .collect();
    let clock_start = run
        .timer
        .as_ref()
        .filter(|t| !t.from_first_pull)
        .map_or(run.started.as_str(), |t| t.started.as_str());

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

    let detail_read = detail.read();
    let Some(d) = detail_read.as_ref().and_then(|d| d.as_ref()) else {
        return rsx! {
            div { class: "sidebar-empty",
                if detail_read.is_none() { p { "Loading…" } } else { p { "Select a run" } }
            }
        };
    };
    let run = &d.run;

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
            save(RunMeta { tags, note: note.clone() });
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
            if let Some(path) = run.path.clone() {
                button {
                    class: "btn btn-sm btn-ghost",
                    title: "Open this log in Encounter Analysis",
                    disabled: opener.busy(),
                    onclick: move |_| opener.open(path.clone(), None),
                    i { class: "fa-solid fa-eye" }
                    " Open log"
                }
            }
        }

        if run.area_id == 0 {
            div { class: "history-pick",
                i { class: "fa-solid fa-triangle-exclamation" }
                " This log's history predates area ids, so boss order, clear status and the timed run are unknown. Re-scan it from Log Management."
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
            if let Some(t) = run.timer.as_ref() {
                div {
                    class: match t.achieved { Some(true) => "history-card ok", Some(false) => "history-card warn", None => "history-card" },
                    title: if t.from_first_pull { "No start line crossing was logged; the clock runs from the first pull" } else { "Clock from the timer start line" },
                    div { class: "k", i { class: "fa-solid fa-stopwatch" } " Timed run" }
                    div { class: "v",
                        match t.elapsed_secs {
                            Some(s) => fmt_secs(s),
                            None => "—".to_string(),
                        }
                    }
                    div { class: "sub",
                        match (t.limit_secs, t.achieved) {
                            (Some(l), Some(true)) => format!("under {}", fmt_secs(i64::from(l))),
                            (Some(l), Some(false)) if run.kills < run.bosses_total => format!("{}/{} bosses · limit {}", run.kills, run.bosses_total, fmt_secs(i64::from(l))),
                            (Some(l), Some(false)) => format!("over {}", fmt_secs(i64::from(l))),
                            (Some(l), None) => format!("limit {}", fmt_secs(i64::from(l))),
                            (None, _) => if t.from_first_pull { "from first pull".into() } else { "no limit set".into() },
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
                        move |_| save(RunMeta { tags: tags.clone(), note: note.clone() })
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
                    move |e| save(RunMeta { tags: tags.clone(), note: e.value().trim().chars().take(140).collect() })
                },
            }
            span { class: "run-note-count", "{run.note.chars().count()}/140" }
        }

        div { class: "run-section",
            h4 { "Bosses" }
            div { class: "pull-history-table-wrap",
                table { class: "pull-history-table",
                    thead { tr { th { "#" } th { "Boss" } th { "Pulls" } th { "Result" } th { "Deaths" } } }
                    tbody {
                        for (i, b) in d.bosses.iter().enumerate() {
                            tr { key: "{b.name}",
                                td { "{i + 1}" }
                                td { class: if b.skipped { "run-boss-skipped" } else { "" }, "{b.name}" }
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
                        }
                    }
                }
            }
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
                                        if let Some(icon) = p.role_icon.as_deref().and_then(get_role_icon) {
                                            img { class: "role-icon", src: *icon, alt: "" }
                                        }
                                        if let Some(icon) = p.discipline_icon.as_deref().and_then(get_class_icon) {
                                            img { class: "discipline-icon", src: *icon, alt: "" }
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

        div { class: "run-section",
            h4 { "Pulls" }
            div { class: "pull-history-table-wrap",
                table { class: "pull-history-table",
                    thead { tr { th { "Time" } th { "Boss" } th { "Result" } th { "Phase" } th { "Duration" } th { "Deaths" } th { "" } } }
                    tbody {
                        for row in d.pulls.iter() {
                            {
                                let (label, class) = result_badge(row, eu);
                                let encounter_id = row.encounter_id;
                                let path = row.path.clone();
                                rsx! {
                                    tr { key: "{row.start_ms}",
                                        td { {short_ts(&row.timestamp)} }
                                        td { "{row.boss}" }
                                        td { span { class: "{class}", "{label}" } }
                                        td { {row.phase.clone().unwrap_or_else(|| "-".into())} }
                                        td { {formatting::format_duration(row.duration_seconds)} }
                                        td { "{row.deaths}" }
                                        td {
                                            if let Some(path) = path {
                                                button {
                                                    class: "btn btn-xs btn-ghost",
                                                    title: "Open this pull in Encounter Analysis",
                                                    disabled: opener.busy(),
                                                    onclick: move |_| opener.open(path.clone(), Some(encounter_id)),
                                                    i { class: "fa-solid fa-eye" }
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
