//! Shared filter toolbar: character, difficulty, group size, role,
//! discipline and date range. Option lists come from the overview resource;
//! the filter itself lives in the parent so it survives sub-tab switches.

use dioxus::prelude::*;
use wasm_bindgen::prelude::*;

use baras_types::history::PullFilter;

use crate::api;

const ROLES: [(&str, &str); 3] = [("Tank", "Tank"), ("Healer", "Healer"), ("Dps", "DPS")];
const TIERS: [&str; 3] = ["Master", "Veteran", "Story"];
const SIZES: [&str; 2] = ["8", "16"];
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

#[component]
pub fn HistoryToolbar(filter: Signal<PullFilter>) -> Element {
    // Option lists are unfiltered, so they only need fetching once per mount
    let options = use_resource(move || async move {
        api::get_pull_history_overview(&PullFilter::default()).await.unwrap_or_default()
    });
    let options = options.read();
    let (characters, disciplines): (&[String], &[String]) = options
        .as_ref()
        .map(|o| (o.characters.as_slice(), o.disciplines.as_slice()))
        .unwrap_or((&[], &[]));

    let mut set = move |update: fn(&mut PullFilter, String), value: String| update(&mut filter.write(), value);
    let f = filter.read().clone();

    rsx! {
        div { class: "history-toolbar",
            select {
                value: "{f.character}",
                onchange: move |e| set(|f, v| f.character = v, e.value()),
                option { value: "", "All characters" }
                for c in characters.iter() {
                    option { value: "{c}", "{c}" }
                }
            }
            select {
                value: "{f.tier}",
                onchange: move |e| set(|f, v| f.tier = v, e.value()),
                for t in TIERS {
                    option { value: "{t}", "{t}" }
                }
                option { value: "", "Any difficulty" }
            }
            select {
                value: "{f.group_size}",
                onchange: move |e| set(|f, v| f.group_size = v, e.value()),
                for s in SIZES {
                    option { value: "{s}", "{s}-man" }
                }
                option { value: "", "Any size" }
            }
            select {
                value: "{f.role}",
                onchange: move |e| set(|f, v| f.role = v, e.value()),
                option { value: "", "Any role" }
                for (value, label) in ROLES {
                    option { value: "{value}", "{label}" }
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
            label { class: "history-toggle",
                input {
                    r#type: "checkbox",
                    checked: f.kills_only,
                    onchange: move |e| filter.write().kills_only = e.checked(),
                }
                " Kills only"
            }
            div { class: "history-dates",
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
