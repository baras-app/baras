//! Shared filter toolbar at the top of each view's sidebar: difficulty,
//! group size and role up front; character, kills, flashpoints, dates,
//! discipline, server, patch and tag behind "More filters". Option lists come from the overview
//! resource; the filter itself lives in the parent so it survives tab switches.

use crate::components::toggle_switch::ToggleSwitch;
use crate::components::multi_select::{MultiSelect, discipline_entries, plain_entries};
use dioxus::prelude::*;
use wasm_bindgen::prelude::*;

use baras_types::history::PullFilter;

use crate::api;
use crate::components::class_icons::get_role_icon;

/// Roles: filter value, label, role icon file (see `class_icons::get_role_icon`)
const ROLES: [(&str, &str, &str); 3] = [("Tank", "Tank", "icon_tank.png"), ("Healer", "Healer", "icon_heal.png"), ("Dps", "DPS", "icon_dps.png")];
/// Difficulty tiers: value, icon, CSS class (colours shared with the Library chips)
const TIERS: [(&str, &str, &str); 3] = [
    ("Story", "fa-solid fa-book-open", "tier-story"),
    ("Veteran", "fa-solid fa-shield-halved", "tier-veteran"),
    ("Master", "fa-solid fa-skull", "tier-master"),
];
/// Group sizes: value, icon
const SIZES: [(&str, &str); 2] = [("8", "fa-solid fa-user-group"), ("16", "fa-solid fa-people-group")];

/// The difficulty subset in view, as coloured chips (for section headers)
#[component]
pub fn SubsetChips(filter: Signal<PullFilter>) -> Element {
    let f = filter.read();
    let tier = TIERS.iter().find(|(t, _, _)| *t == f.tier);
    let size = SIZES.iter().find(|(s, _)| *s == f.group_size);
    rsx! {
        span { class: "history-subset",
            match tier {
                Some((t, icon, class)) => rsx! { span { class: "subset-chip {class}", i { class: "{icon}" } " {t}" } },
                None => rsx! { span { class: "subset-chip any", "Any difficulty" } },
            }
            match size {
                Some((s, icon)) => rsx! { span { class: "subset-chip size", i { class: "{icon}" } " {s}-man" } },
                None => rsx! { span { class: "subset-chip any", "Any size" } },
            }
        }
    }
}
/// Quick date-range presets: label and days back (None = start of this year)
const DATE_PRESETS: [(&str, Option<i32>); 4] = [("7d", Some(7)), ("30d", Some(30)), ("90d", Some(90)), ("Year", None)];

/// `YYYY-MM-DD` for `days` ago in local time, or Jan 1 of this year
fn preset_start(days: Option<i32>) -> String {
    const DAY_MS: f64 = 86_400_000.0;
    let d = js_sys::Date::new(&JsValue::from_f64(js_sys::Date::now() - days.unwrap_or(0) as f64 * DAY_MS));
    match days {
        Some(_) => format!("{:04}-{:02}-{:02}", d.get_full_year(), d.get_month() + 1, d.get_date()),
        None => format!("{:04}-01-01", d.get_full_year()),
    }
}

/// Filters behind the toggle that are narrowing the view, for its badge
fn extra_active(f: &PullFilter, runs: bool) -> usize {
    [&f.date_from, &f.date_to].iter().filter(|v| !v.is_empty()).count()
        + [&f.characters, &f.disciplines, &f.servers, &f.game_versions, &f.tags].iter().filter(|v| !v.is_empty()).count()
        + usize::from(f.kills_only)
        + usize::from(runs && f.flashpoints)
}

/// Everything narrowing the view, segmented toggles included (Clear button)
fn active_count(f: &PullFilter, runs: bool) -> usize {
    extra_active(f, runs) + [&f.tier, &f.group_size, &f.role].iter().filter(|v| !v.is_empty()).count()
}

/// `runs`: the Raid Summaries view, where the role played is meaningless
/// across a night and "kills only" means fully cleared runs.
#[component]
pub fn HistoryToolbar(filter: Signal<PullFilter>, runs: bool) -> Element {
    // Option lists are unfiltered, so they only need fetching once per mount
    let options = use_resource(move || async move {
        api::get_pull_history_overview(&PullFilter::default()).await.unwrap_or_default()
    });
    let options = options.read();
    let o = options.as_ref();
    let characters: &[String] = o.map_or(&[], |o| o.characters.as_slice());
    let disciplines: &[String] = o.map_or(&[], |o| o.disciplines.as_slice());
    let tags: &[String] = o.map_or(&[], |o| o.tags.as_slice());
    let servers: &[String] = o.map_or(&[], |o| o.servers.as_slice());
    let versions: &[String] = o.map_or(&[], |o| o.versions.as_slice());

    let mut set = move |update: fn(&mut PullFilter, String), value: String| update(&mut filter.write(), value);
    let f = filter.read().clone();
    let extra = extra_active(&f, runs);
    let active = active_count(&f, runs);
    // Starts open when a hidden filter is already narrowing the view
    let mut more = use_signal(|| extra > 0);

    // Segmented toggles: nothing lit means "any"; clicking the lit one clears it
    let mut toggle = move |pick: fn(&mut PullFilter) -> &mut String, value: &str| {
        let mut w = filter.write();
        let slot = pick(&mut w);
        if slot == value {
            slot.clear()
        } else {
            *slot = value.to_string()
        }
    };

    rsx! {
        div { class: "history-toolbar",
            div { class: "history-seg", title: "Difficulty",
                for (t, icon, class) in TIERS {
                    button {
                        class: if f.tier == t { "seg active {class}" } else { "seg {class}" },
                        onclick: move |_| toggle(|f| &mut f.tier, t),
                        i { class: "{icon}" }
                        " {t}"
                    }
                }
            }
            div { class: "history-seg", title: "Group size",
                for (s, icon) in SIZES {
                    button {
                        class: if f.group_size == s { "seg active size" } else { "seg size" },
                        onclick: move |_| toggle(|f| &mut f.group_size, s),
                        i { class: "{icon}" }
                        " {s}"
                    }
                }
            }
            div { class: "history-seg", title: if runs { "Your role on the run" } else { "Your role on the pull" },
                for (value, label, icon) in ROLES {
                    button {
                        class: if f.role == value { "seg active role" } else { "seg role" },
                        onclick: move |_| toggle(|f| &mut f.role, value),
                        if let Some(src) = get_role_icon(icon) {
                            img { class: "role-icon", src: *src, alt: "" }
                        }
                        " {label}"
                    }
                }
            }
            div { class: "history-actions",
                button {
                    class: if *more.read() { "btn btn-xs history-more active" } else { "btn btn-xs history-more" },
                    onclick: move |_| more.toggle(),
                    i { class: if *more.read() { "fa-solid fa-chevron-up" } else { "fa-solid fa-sliders" } }
                    " More filters"
                    if extra > 0 { span { class: "history-more-count", "{extra}" } }
                }
                button {
                    class: if active > 0 { "btn btn-xs history-clear active" } else { "btn btn-xs history-clear" },
                    disabled: active == 0,
                    title: "Show everything: any difficulty, size and role, and nothing under More filters",
                    onclick: move |_| filter.set(PullFilter::default()),
                    i { class: "fa-solid fa-filter-circle-xmark" }
                    " Clear filters"
                    if active > 0 { span { class: "history-more-count", "{active}" } }
                }
            }
        }
        if *more.read() {
            div { class: "history-toolbar secondary",
                // Switches first, then the dropdowns, then the date range
                div { class: "history-switches",
                    ToggleSwitch {
                        label: (if runs { "Full clears only" } else { "Kills only" }).to_string(),
                        class: "history-switch",
                        checked: f.kills_only,
                        on_change: move |checked: bool| filter.write().kills_only = checked,
                    }
                    // Boss Statistics always lists flashpoints, under their own sidebar group
                    if runs {
                        ToggleSwitch {
                            label: "Include flashpoints",
                            class: "history-switch",
                            checked: f.flashpoints,
                            on_change: move |checked: bool| filter.write().flashpoints = checked,
                        }
                    }
                }
                MultiSelect {
                    any_label: "All characters",
                    noun: "characters",
                    entries: plain_entries(characters),
                    picked: f.characters.clone(),
                    on_change: move |v: Vec<String>| filter.write().characters = v,
                }
                MultiSelect {
                    any_label: "Any discipline",
                    noun: "disciplines",
                    title: "Mirror disciplines count as one",
                    entries: discipline_entries(disciplines),
                    picked: f.disciplines.clone(),
                    on_change: move |v: Vec<String>| filter.write().disciplines = v,
                }
                if servers.len() > 1 || !f.servers.is_empty() {
                    MultiSelect {
                        any_label: "Any server",
                        noun: "servers",
                        title: "Server the log was recorded on",
                        entries: plain_entries(servers),
                        picked: f.servers.clone(),
                        on_change: move |v: Vec<String>| filter.write().servers = v,
                    }
                }
                if versions.len() > 1 || !f.game_versions.is_empty() {
                    MultiSelect {
                        any_label: "Any patch",
                        noun: "patches",
                        title: "Game patch live at the time of the pull",
                        entries: plain_entries(versions),
                        picked: f.game_versions.clone(),
                        on_change: move |v: Vec<String>| filter.write().game_versions = v,
                    }
                }
                if !tags.is_empty() || !f.tags.is_empty() {
                    MultiSelect {
                        any_label: "Any tag",
                        noun: "tags",
                        title: "Only pulls and runs with any of these tags",
                        entries: plain_entries(tags),
                        picked: f.tags.clone(),
                        on_change: move |v: Vec<String>| filter.write().tags = v,
                    }
                }
                div { class: "history-range", title: "Date range",
                    div { class: "pull-history-result-tabs",
                        button {
                            class: if f.date_from.is_empty() && f.date_to.is_empty() { "filter-tab active" } else { "filter-tab" },
                            onclick: move |_| {
                                let mut w = filter.write();
                                w.date_from.clear();
                                w.date_to.clear();
                            },
                            "All"
                        }
                        for (label, days) in DATE_PRESETS {
                            {
                                let start = preset_start(days);
                                let active = f.date_to.is_empty() && f.date_from == start;
                                rsx! {
                                    button {
                                        class: if active { "filter-tab active" } else { "filter-tab" },
                                        onclick: move |_| {
                                            let mut w = filter.write();
                                            w.date_from = start.clone();
                                            w.date_to.clear();
                                        },
                                        "{label}"
                                    }
                                }
                            }
                        }
                    }
                    div { class: "history-dates",
                        input {
                            r#type: "text",
                            class: "pull-history-date",
                            placeholder: "from YYYY-MM-DD",
                            maxlength: "10",
                            value: "{f.date_from}",
                            onchange: move |e| set(|f, v| f.date_from = v, e.value()),
                        }
                        span { "–" }
                        input {
                            r#type: "text",
                            class: "pull-history-date",
                            placeholder: "to YYYY-MM-DD",
                            maxlength: "10",
                            value: "{f.date_to}",
                            onchange: move |e| set(|f, v| f.date_to = v, e.value()),
                        }
                    }
                }
            }
        }
    }
}
