//! Calendar heatmap: one cell per day over the last year, shaded by a level
//! and tinted by a tone class. Clicking a day (or a month label) selects a
//! day range the caller filters on. `LogHeatmap` is the Log Management
//! flavour (bytes logged, hardest content); PvP Statistics builds its own
//! days from matches.

use std::collections::HashMap;

use dioxus::prelude::*;

use baras_types::history::{ContentTag, LibraryRow, iso_to_ms};

use super::library::fmt_bytes;

const DAY_MS: i64 = 86_400_000;
const WEEKS: i64 = 53;
const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const WEEKDAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

/// Inclusive `YYYY-MM-DD` bounds picked on the calendar
pub type DayRange = (String, String);

#[derive(Clone, PartialEq, Default)]
pub struct DayStat {
    /// Days since the Unix epoch (local date)
    pub day: i64,
    pub logs: u32,
    pub bytes: u64,
    pub kills: u32,
    pub tags: Vec<ContentTag>,
}

/// Days since epoch for a `YYYY-MM-DD` prefix
pub fn day_of(date: &str) -> Option<i64> {
    iso_to_ms(&format!("{}T00:00:00", date.get(..10)?)).map(|ms| ms.div_euclid(DAY_MS))
}

/// Howard Hinnant's civil_from_days: (year, month, day)
fn civil(day: i64) -> (i64, u32, u32) {
    let z = day + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

fn iso(day: i64) -> String {
    let (y, m, d) = civil(day);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Monday = 0 (1970-01-01 was a Thursday)
fn weekday(day: i64) -> i64 {
    (day + 3).rem_euclid(7)
}

fn today() -> i64 {
    let now = js_sys::Date::new_0();
    day_of(&format!("{:04}-{:02}-{:02}", now.get_full_year(), now.get_month() + 1, now.get_date())).unwrap_or(0)
}

/// Per-day totals over every log, newest irrelevant: order is by day
pub fn day_stats(rows: &[LibraryRow]) -> Vec<DayStat> {
    let mut days: HashMap<i64, DayStat> = HashMap::new();
    for r in rows {
        let Some(day) = day_of(&r.date) else { continue };
        let s = days.entry(day).or_insert_with(|| DayStat { day, ..Default::default() });
        s.logs += 1;
        s.bytes += r.size;
        s.kills += r.kills;
        for t in &r.content {
            if !s.tags.contains(t) {
                s.tags.push(*t);
            }
        }
    }
    let mut v: Vec<DayStat> = days.into_values().collect();
    v.sort_by_key(|s| s.day);
    v
}

/// The hardest content of the day decides the hue (same classes as the tags)
fn tone(tags: &[ContentTag]) -> &'static str {
    let has = |f: fn(&ContentTag) -> bool| tags.iter().any(f);
    if has(|t| matches!(t, ContentTag::MasterOps | ContentTag::MasterFlashpoint)) {
        "nim"
    } else if has(|t| matches!(t, ContentTag::VeteranOps | ContentTag::VeteranFlashpoint)) {
        "hm"
    } else if has(|t| matches!(t, ContentTag::StoryOps)) {
        "sm"
    } else if has(|t| matches!(t, ContentTag::Pvp)) {
        "pvp"
    } else if has(|t| matches!(t, ContentTag::WorldBoss)) {
        "wb"
    } else {
        "misc"
    }
}

/// Does a `YYYY-MM-DD HH:MM` stamp fall inside the picked range?
pub fn in_range(date: &str, range: &DayRange) -> bool {
    date.get(..10).is_some_and(|d| d >= range.0.as_str() && d <= range.1.as_str())
}

/// One rendered day: `level` 1–4 picks the shade, `tone` the hue class
/// (`nim`, `hm`, `sm`, `pvp`, `wb`, `misc`, `win`, `loss`, `even`).
#[derive(Clone, PartialEq)]
pub struct HeatDay {
    pub day: i64,
    pub level: u8,
    pub tone: &'static str,
    pub title: String,
}

/// Shade 1–4 for `value` against the largest value, square-root scaled so a
/// single heavy day does not flatten every other one
pub fn level(value: f64, max: f64) -> u8 {
    ((value / max.max(1.0)).sqrt() * 4.0).ceil().clamp(1.0, 4.0) as u8
}

#[component]
pub fn LogHeatmap(days: Vec<DayStat>, selected: Signal<Option<DayRange>>) -> Element {
    let max_bytes = days.iter().map(|s| s.bytes).max().unwrap_or(1) as f64;
    let cells: Vec<HeatDay> = days
        .iter()
        .map(|s| {
            let tags: Vec<&str> = s.tags.iter().map(|t| t.label()).collect();
            HeatDay {
                day: s.day,
                level: level(s.bytes as f64, max_bytes),
                tone: tone(&s.tags),
                title: format!(
                    "{} log{} · {}{}{}",
                    s.logs,
                    if s.logs == 1 { "" } else { "s" },
                    fmt_bytes(s.bytes),
                    if s.kills > 0 { format!(" · {} kills", s.kills) } else { String::new() },
                    if tags.is_empty() { String::new() } else { format!("\n{}", tags.join(", ")) }
                ),
            }
        })
        .collect();
    rsx! {
        Heatmap { days: cells, selected, title: "Logging activity", hint: "Click a day or month to filter the table", legend: "pvp" }
    }
}

#[component]
pub fn Heatmap(
    days: Vec<HeatDay>,
    selected: Signal<Option<DayRange>>,
    title: &'static str,
    hint: &'static str,
    /// Tone class the legend swatches use
    legend: &'static str,
) -> Element {
    let by_day: HashMap<i64, &HeatDay> = days.iter().map(|s| (s.day, s)).collect();
    // Window: the last 53 weeks, columns starting on a Monday
    let end = today();
    let start = end - weekday(end) - 7 * (WEEKS - 1);
    let columns = WEEKS as usize;
    let range = selected.read().clone();

    // Month label on the first column whose week holds the 1st
    let months: Vec<(usize, &str)> = (0..columns)
        .filter_map(|col| {
            let monday = start + col as i64 * 7;
            (0..7).map(|i| monday + i).find(|d| civil(*d).2 == 1).map(|d| (col, MONTHS[(civil(d).1 - 1) as usize]))
        })
        .collect();

    rsx! {
        div { class: "log-heatmap",
            div { class: "log-heatmap-head",
                span { class: "log-heatmap-title", i { class: "fa-solid fa-calendar-days" } " {title}" }
                if !hint.is_empty() { span { class: "hint", "{hint}" } }
                div { class: "log-heatmap-legend",
                    span { "Less" }
                    for lvl in 1..=4 { span { class: "heatmap-cell {legend} l{lvl}" } }
                    span { "More" }
                }
            }
            div { class: "log-heatmap-months", style: "grid-template-columns: repeat({columns}, minmax(0, 1fr))",
                for (col, label) in months.iter() {
                    {
                        let monday = start + *col as i64 * 7;
                        let first = (0..7).map(|i| monday + i).find(|d| civil(*d).2 == 1).unwrap_or(monday);
                        let (y, m, _) = civil(first);
                        let last = (first..first + 32).take_while(|d| civil(*d).1 == m).last().unwrap_or(first);
                        let month_range = (iso(first), iso(last.min(end)));
                        let active = range.as_ref() == Some(&month_range);
                        // Never spill past the last week column (it would add implicit columns)
                        let span = (columns - col).min(4);
                        rsx! {
                            button {
                                class: if active { "log-heatmap-month active" } else { "log-heatmap-month" },
                                style: "grid-column: {col + 1} / span {span}",
                                title: "Only {label} {y}",
                                onclick: move |_| {
                                    let r = month_range.clone();
                                    let mut sel = selected.write();
                                    if sel.as_ref() == Some(&r) { *sel = None } else { *sel = Some(r) }
                                },
                                "{label}"
                            }
                        }
                    }
                }
            }
            div { class: "log-heatmap-body",
                div { class: "log-heatmap-weekdays",
                    for (i, w) in WEEKDAYS.iter().enumerate() {
                        span { if i % 2 == 0 { "{w}" } }
                    }
                }
                div { class: "log-heatmap-grid", style: "grid-template-columns: repeat({columns}, minmax(0, 1fr))",
                    for col in 0..columns {
                        for row in 0..7i64 {
                            {
                                let day = start + col as i64 * 7 + row;
                                let date = iso(day);
                                let cell = by_day.get(&day);
                                let future = day > end;
                                let picked = range.as_ref().is_some_and(|r| date >= r.0 && date <= r.1);
                                let class = match cell {
                                    Some(c) => format!("heatmap-cell {} l{}{}", c.tone, c.level, if picked { " picked" } else { "" }),
                                    None if future => "heatmap-cell future".to_string(),
                                    None => format!("heatmap-cell l0{}", if picked { " picked" } else { "" }),
                                };
                                let title = match cell {
                                    Some(c) => format!("{} {date}\n{}", WEEKDAYS[row as usize], c.title),
                                    None => format!("{} {date}", WEEKDAYS[row as usize]),
                                };
                                rsx! {
                                    div {
                                        key: "{day}",
                                        class: "{class}",
                                        title: "{title}",
                                        onclick: move |_| {
                                            if future { return }
                                            let r = (date.clone(), date.clone());
                                            let mut sel = selected.write();
                                            if sel.as_ref() == Some(&r) { *sel = None } else { *sel = Some(r) }
                                        },
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
