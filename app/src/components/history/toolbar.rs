//! Shared filter toolbar at the top of each view's sidebar: difficulty,
//! group size and role up front; character, kills, flashpoints, dates,
//! discipline, server, patch and tag behind "More filters". Option lists come from the overview
//! resource; the filter itself lives in the parent so it survives tab switches.

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
fn extra_active(f: &PullFilter) -> usize {
    [
        &f.character,
        &f.discipline,
        &f.server,
        &f.game_version,
        &f.tag,
        &f.date_from,
        &f.date_to,
    ]
    .iter()
    .filter(|v| !v.is_empty())
    .count()
        + usize::from(f.kills_only)
        + usize::from(f.flashpoints)
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
    let extra = extra_active(&f);
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
            if !runs {
                div { class: "history-seg", title: "Your role on the pull",
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
            }
            button {
                class: if *more.read() { "btn btn-xs history-more active" } else { "btn btn-xs history-more" },
                onclick: move |_| more.toggle(),
                i { class: if *more.read() { "fa-solid fa-chevron-up" } else { "fa-solid fa-sliders" } }
                " More filters"
                if extra > 0 { span { class: "history-more-count", "{extra}" } }
            }
        }
        if *more.read() {
            div { class: "history-toolbar secondary",
                select {
                    value: "{f.character}",
                    onchange: move |e| set(|f, v| f.character = v, e.value()),
                    option { value: "", "All characters" }
                    for c in characters.iter() {
                        option { value: "{c}", "{c}" }
                    }
                }
                label { class: "history-toggle",
                    input {
                        r#type: "checkbox",
                        checked: f.kills_only,
                        onchange: move |e| filter.write().kills_only = e.checked(),
                    }
                    if runs { " Full clears only" } else { " Kills only" }
                }
                label { class: "history-toggle", title: "Flashpoint bosses and runs alongside operations",
                    input {
                        r#type: "checkbox",
                        checked: f.flashpoints,
                        onchange: move |e| filter.write().flashpoints = e.checked(),
                    }
                    " Include flashpoints"
                }
                div { class: "pull-history-result-tabs", title: "Date range",
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
                select {
                    value: "{f.discipline}",
                    onchange: move |e| set(|f, v| f.discipline = v, e.value()),
                    option { value: "", "Any discipline" }
                    for d in disciplines.iter() {
                        option { value: "{d}", "{d}" }
                    }
                }
                if servers.len() > 1 || !f.server.is_empty() {
                    select {
                        title: "Server the log was recorded on",
                        value: "{f.server}",
                        onchange: move |e| set(|f, v| f.server = v, e.value()),
                        option { value: "", "Any server" }
                        for s in servers.iter() {
                            option { value: "{s}", "{s}" }
                        }
                    }
                }
                if versions.len() > 1 || !f.game_version.is_empty() {
                    select {
                        title: "Game patch live at the time of the pull",
                        value: "{f.game_version}",
                        onchange: move |e| set(|f, v| f.game_version = v, e.value()),
                        option { value: "", "Any patch" }
                        for v in versions.iter() {
                            option { value: "{v}", "Patch {v}" }
                        }
                    }
                }
                if !tags.is_empty() || !f.tag.is_empty() {
                    select {
                        class: if f.tag.is_empty() { "" } else { "history-tag-active" },
                        title: "Only pulls and runs with this tag",
                        value: "{f.tag}",
                        onchange: move |e| set(|f, v| f.tag = v, e.value()),
                        option { value: "", "Any tag" }
                        for t in tags.iter() {
                            option { value: "{t}", "{t}" }
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
