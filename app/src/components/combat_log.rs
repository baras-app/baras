//! Combat Log panel with virtual scrolling for the data explorer.
//!
//! Displays a virtualized table of combat events with filtering capabilities.

use dioxus::prelude::*;
use wasm_bindgen::JsCast;

use crate::api::{self, CombatLogRow, TimeRange};
use crate::components::ability_icon::AbilityIcon;

/// Row height in pixels for virtual scrolling calculations.
const ROW_HEIGHT: f64 = 24.0;
/// Number of rows to render beyond the visible viewport (buffer).
const OVERSCAN: usize = 10;
/// Page size for data fetching.
const PAGE_SIZE: u64 = 200;

#[derive(Props, Clone, PartialEq)]
pub struct CombatLogProps {
    pub encounter_idx: u32,
    pub time_range: TimeRange,
    /// Optional initial search text (e.g., player name from death tracker)
    #[props(default)]
    pub initial_search: Option<String>,
}

/// Format time as M:SS.d
fn format_time(secs: f32) -> String {
    let mins = (secs / 60.0) as u32;
    let s = secs % 60.0;
    format!("{mins}:{s:05.2}")
}

/// Format a number with thousands separators.
fn format_number(n: i32) -> String {
    if n == 0 {
        return String::new();
    }
    let s = n.abs().to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.insert(0, ',');
        }
        result.insert(0, c);
    }
    if n < 0 {
        result.insert(0, '-');
    }
    result
}

/// Get CSS class for effect type.
fn effect_type_class(effect_type: &str) -> &'static str {
    match effect_type {
        "ApplyEffect" => "log-apply",
        "RemoveEffect" => "log-remove",
        "Event" => "log-event",
        _ => "",
    }
}

/// Get CSS class for row based on content.
fn row_class(row: &CombatLogRow) -> String {
    let mut classes = vec!["log-row"];

    // Effect type based coloring
    if row.value > 0 {
        if row.effect_name.contains("Damage") || row.damage_type.is_empty() {
            classes.push("log-damage");
        } else {
            classes.push("log-heal");
        }
    }

    // Critical hit
    if row.is_crit {
        classes.push("log-crit");
    }

    // Miss/dodge/etc
    if !row.defense_type_id.is_positive() {
        classes.push("log-avoid");
    }

    classes.join(" ")
}

#[component]
pub fn CombatLog(props: CombatLogProps) -> Element {
    // Mirror time_range prop into a signal for reactivity
    let mut time_range_signal = use_signal(|| props.time_range.clone());

    // Update signal when props change (runs on every render with new props)
    if *time_range_signal.read() != props.time_range {
        time_range_signal.set(props.time_range.clone());
    }

    // Filter state - initialize search from props if provided (e.g., death tracker)
    let mut source_filter = use_signal(|| None::<String>);
    let mut target_filter = use_signal(|| None::<String>);
    let mut search_text = use_signal(|| props.initial_search.clone().unwrap_or_default());

    // Data state
    let mut rows = use_signal(Vec::<CombatLogRow>::new);
    let mut total_count = use_signal(|| 0u64);
    let mut source_names = use_signal(Vec::<String>::new);
    let mut target_names = use_signal(Vec::<String>::new);

    // Virtual scroll state
    let mut scroll_top = use_signal(|| 0.0f64);
    let mut container_height = use_signal(|| 500.0f64);
    let mut loaded_offset = use_signal(|| 0u64);

    // Debounced search
    let mut search_debounce = use_signal(String::new);

    // Load source/target names on mount
    use_effect({
        let idx = props.encounter_idx;
        move || {
            spawn(async move {
                if let Some(sources) = api::query_source_names(Some(idx)).await {
                    source_names.set(sources);
                }
                if let Some(targets) = api::query_target_names(Some(idx)).await {
                    target_names.set(targets);
                }
            });
        }
    });

    // Load data when filters or time range change
    use_effect(move || {
        let idx = props.encounter_idx;
        let tr = time_range_signal.read().clone();
        let source = source_filter.read().clone();
        let target = target_filter.read().clone();
        let search = search_debounce.read().clone();
        let search_opt = if search.is_empty() {
            None
        } else {
            Some(search)
        };

        spawn(async move {
            // Reset scroll position
            scroll_top.set(0.0);
            loaded_offset.set(0);

            let tr_opt = if tr.start == 0.0 && tr.end == 0.0 {
                None
            } else {
                Some(&tr)
            };

            // Get total count
            if let Some(count) = api::query_combat_log_count(
                Some(idx),
                source.as_deref(),
                target.as_deref(),
                search_opt.as_deref(),
                tr_opt,
            )
            .await
            {
                total_count.set(count);
            }

            // Load first page
            if let Some(data) = api::query_combat_log(
                Some(idx),
                0,
                PAGE_SIZE,
                source.as_deref(),
                target.as_deref(),
                search_opt.as_deref(),
                tr_opt,
            )
            .await
            {
                rows.set(data);
            }
        });
    });

    // Debounce search input
    use_effect({
        move || {
            let text = search_text.read().clone();
            spawn(async move {
                gloo_timers::future::TimeoutFuture::new(300).await;
                if *search_text.read() == text {
                    search_debounce.set(text);
                }
            });
        }
    });

    // Calculate virtual scroll window (for rendering)
    let total = *total_count.read() as usize;
    let scroll = *scroll_top.read();
    let height = *container_height.read();

    let total_height = total as f64 * ROW_HEIGHT;
    let start_idx = ((scroll / ROW_HEIGHT) as usize).saturating_sub(OVERSCAN);
    let visible_count = ((height / ROW_HEIGHT) as usize) + OVERSCAN * 2;
    let end_idx = (start_idx + visible_count).min(total);

    // Load more data when scrolling beyond current buffer
    // Must read signals INSIDE the effect for Dioxus to track them as dependencies
    use_effect({
        let idx = props.encounter_idx;
        move || {
            // Read scroll signals inside effect so Dioxus tracks them
            let total = *total_count.read() as usize;
            let scroll = *scroll_top.read();
            let height = *container_height.read();

            let start_idx = ((scroll / ROW_HEIGHT) as usize).saturating_sub(OVERSCAN);
            let visible_count = ((height / ROW_HEIGHT) as usize) + OVERSCAN * 2;
            let end_idx = (start_idx + visible_count).min(total);

            let offset = *loaded_offset.read() as usize;
            let rows_len = rows.read().len();
            let need_load = start_idx < offset || end_idx > offset + rows_len;

            if need_load && rows_len > 0 {
                let tr = time_range_signal.read().clone();
                let source = source_filter.read().clone();
                let target = target_filter.read().clone();
                let search = search_debounce.read().clone();
                let new_offset = start_idx.saturating_sub(OVERSCAN) as u64;

                spawn(async move {
                    let search_opt = if search.is_empty() {
                        None
                    } else {
                        Some(search)
                    };
                    let tr_opt = if tr.start == 0.0 && tr.end == 0.0 {
                        None
                    } else {
                        Some(&tr)
                    };

                    if let Some(data) = api::query_combat_log(
                        Some(idx),
                        new_offset,
                        PAGE_SIZE,
                        source.as_deref(),
                        target.as_deref(),
                        search_opt.as_deref(),
                        tr_opt,
                    )
                    .await
                    {
                        loaded_offset.set(new_offset);
                        rows.set(data);
                    }
                });
            }
        }
    });

    // Slice visible rows from loaded data (with bounds safety)
    let current_rows = rows.read();
    let offset = *loaded_offset.read() as usize;
    let visible_rows: Vec<CombatLogRow> = if !current_rows.is_empty() {
        let rel_start = start_idx.saturating_sub(offset).min(current_rows.len());
        let rel_end = end_idx.saturating_sub(offset).min(current_rows.len());
        // Ensure start <= end
        if rel_start < rel_end {
            current_rows[rel_start..rel_end].to_vec()
        } else {
            vec![]
        }
    } else {
        vec![]
    };

    let sources_list = source_names.read().clone();
    let targets_list = target_names.read().clone();

    rsx! {
        div { class: "combat-log-panel",
            // Filter bar
            div { class: "log-filters",
                // Source filter
                select {
                    class: "log-filter-select",
                    value: source_filter.read().as_deref().unwrap_or(""),
                    onchange: move |e| {
                        let val = e.value();
                        source_filter.set(if val.is_empty() { None } else { Some(val) });
                    },
                    option { value: "", "All Sources" }
                    for name in sources_list.iter() {
                        option { value: "{name}", "{name}" }
                    }
                }

                // Target filter
                select {
                    class: "log-filter-select",
                    value: target_filter.read().as_deref().unwrap_or(""),
                    onchange: move |e| {
                        let val = e.value();
                        target_filter.set(if val.is_empty() { None } else { Some(val) });
                    },
                    option { value: "", "All Targets" }
                    for name in targets_list.iter() {
                        option { value: "{name}", "{name}" }
                    }
                }

                // Search input
                input {
                    class: "log-search",
                    r#type: "text",
                    placeholder: "Search...",
                    value: "{search_text}",
                    oninput: move |e| search_text.set(e.value()),
                }

                // Row count
                span { class: "log-count", "{total} events" }
            }

            // Table container with virtual scrolling
            div {
                class: "log-table-container",
                id: "combat-log-scroll",
                onscroll: move |_| {
                    // Get scroll position from DOM element
                    if let Some(window) = web_sys::window()
                        && let Some(doc) = window.document()
                            && let Some(elem) = doc.get_element_by_id("combat-log-scroll")
                                && let Some(html_elem) = elem.dyn_ref::<web_sys::HtmlElement>() {
                                    scroll_top.set(html_elem.scroll_top() as f64);
                                    container_height.set(html_elem.client_height() as f64);
                    }
                },
                // Header row (sticky)
                div { class: "log-header",
                    div { class: "log-cell log-time", "Time" }
                    div { class: "log-cell log-source", "Source" }
                    div { class: "log-cell log-type", "Type" }
                    div { class: "log-cell log-target", "Target" }
                    div { class: "log-cell log-ability", "Ability" }
                    div { class: "log-cell log-value", "Value" }
                    div { class: "log-cell log-absorbed", "Abs" }
                    div { class: "log-cell log-overheal", "Over" }
                    div { class: "log-cell log-threat", "Threat" }
                }

                // Virtual scroll container
                div {
                    class: "log-virtual-container",
                    style: "height: {total_height}px; position: relative;",

                    // Rendered rows
                    div {
                        style: "position: absolute; top: {start_idx as f64 * ROW_HEIGHT}px; width: 100%;",
                        for row in visible_rows.iter() {
                            div {
                                key: "{row.row_idx}",
                                class: "{row_class(&row)}",
                                div { class: "log-cell log-time", "{format_time(row.time_secs)}" }
                                div { class: "log-cell log-source", "{row.source_name}" }
                                div { class: "log-cell log-type {effect_type_class(&row.effect_type)}", "{row.effect_type}" }
                                div { class: "log-cell log-target", "{row.target_name}" }
                                div { class: "log-cell log-ability",
                                    if row.ability_id != 0 {
                                        AbilityIcon { key: "{row.ability_id}", ability_id: row.ability_id, size: 16 }
                                    }
                                    if !row.ability_name.is_empty() {
                                        "{row.ability_name}"
                                    } else {
                                        "{row.effect_name}"
                                    }
                                }
                                div { class: "log-cell log-value",
                                    if row.is_crit { "*" } else { "" }
                                    "{format_number(row.value)}"
                                }
                                div { class: "log-cell log-absorbed", "{format_number(row.absorbed)}" }
                                div { class: "log-cell log-overheal", "{format_number(row.overheal)}" }
                                div { class: "log-cell log-threat",
                                    {
                                        let threat_str = if row.threat > 0.0 {
                                            format!("{:.0}", row.threat)
                                        } else {
                                            String::new()
                                        };
                                        rsx! { "{threat_str}" }
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
