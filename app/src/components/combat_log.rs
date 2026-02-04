//! Combat Log panel with virtual scrolling for the data explorer.
//!
//! Displays a virtualized table of combat events with filtering capabilities.

use dioxus::prelude::*;
use wasm_bindgen::JsCast;

use crate::api::{self, CombatLogFilters, CombatLogFindMatch, CombatLogRow, GroupedEntityNames, TimeRange};
use crate::components::ability_icon::AbilityIcon;
use crate::types::CombatLogSessionState;

/// Row height in pixels for virtual scrolling calculations.
const ROW_HEIGHT: f64 = 24.0;
/// Number of rows to render beyond the visible viewport (buffer).
const OVERSCAN: usize = 10;
/// Page size for data fetching.
const PAGE_SIZE: u64 = 200;

// Effect type IDs for mapping to readable names
const EFFECT_TYPE_APPLYEFFECT: i64 = 836045448945477;
const EFFECT_TYPE_REMOVEEFFECT: i64 = 836045448945478;
const EFFECT_TYPE_EVENT: i64 = 836045448945472;
const EFFECT_TYPE_SPEND: i64 = 836045448945473;
const EFFECT_TYPE_RESTORE: i64 = 836045448945476;

// Effect IDs for mapping
const EFFECT_DAMAGE: i64 = 836045448945501;
const EFFECT_HEAL: i64 = 836045448945500;
const EFFECT_ABILITYACTIVATE: i64 = 836045448945479;
const EFFECT_ABILITYDEACTIVATE: i64 = 836045448945480;
const EFFECT_ABILITYINTERRUPT: i64 = 836045448945482;
const EFFECT_DEATH: i64 = 836045448945493;
const EFFECT_REVIVED: i64 = 836045448945494;

// Defense type IDs
const DEFENSE_SHIELD: i64 = 836045448945509;
const DEFENSE_IMMUNE: i64 = 836045448945506;
const DEFENSE_DEFLECT: i64 = 836045448945508;
const DEFENSE_PARRY: i64 = 836045448945503;
const DEFENSE_DODGE: i64 = 836045448945505;
const DEFENSE_MISS: i64 = 836045448945502;
const DEFENSE_RESIST: i64 = 836045448945507;
const DEFENSE_COVER: i64 = 836045448945510;
const DEFENSE_ABSORBED: i64 = 836045448945511;
const DEFENSE_REFLECTED: i64 = 836045448953649;

#[derive(Props, Clone, PartialEq)]
pub struct CombatLogProps {
    pub encounter_idx: u32,
    pub time_range: TimeRange,
    /// Optional initial search text (e.g., player name from death tracker)
    #[props(default)]
    pub initial_search: Option<String>,
    /// Persisted state signal (survives tab switches, includes show_ids!)
    pub state: Signal<CombatLogSessionState>,
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

/// Get readable event type from effect_type_id and effect_id.
fn readable_event_type(row: &CombatLogRow) -> &'static str {
    match row.effect_type_id {
        EFFECT_TYPE_APPLYEFFECT => match row.effect_id {
            EFFECT_DAMAGE => "Damage",
            EFFECT_HEAL => "Healing",
            _ => "Effect gained",
        },
        EFFECT_TYPE_REMOVEEFFECT => "Effect lost",
        EFFECT_TYPE_EVENT => match row.effect_id {
            EFFECT_ABILITYACTIVATE => "Activation",
            EFFECT_ABILITYDEACTIVATE => "Deactivation",
            EFFECT_ABILITYINTERRUPT => "Interrupt",
            EFFECT_DEATH => "Death",
            EFFECT_REVIVED => "Revive",
            _ => "Event",
        },
        EFFECT_TYPE_SPEND => "Spend",
        EFFECT_TYPE_RESTORE => "Restore",
        _ => "",
    }
}

/// Get readable defense/mitigation type.
fn readable_defense_type(id: i64) -> &'static str {
    match id {
        DEFENSE_SHIELD => "shield",
        DEFENSE_IMMUNE => "immune",
        DEFENSE_DEFLECT => "deflect",
        DEFENSE_PARRY => "parry",
        DEFENSE_DODGE => "dodge",
        DEFENSE_MISS => "miss",
        DEFENSE_RESIST => "resist",
        DEFENSE_COVER => "cover",
        DEFENSE_ABSORBED => "absorbed",
        DEFENSE_REFLECTED => "reflected",
        _ => "",
    }
}

/// Format damage type to a shorter display name.
fn format_damage_type(dmg_type: &str) -> &str {
    match dmg_type {
        "kinetic" => "kinetic",
        "energy" => "energy",
        "internal" => "internal",
        "elemental" => "elemental",
        _ => dmg_type,
    }
}

/// Get CSS class for row based on content.
fn row_class(row: &CombatLogRow, highlighted_row_idx: Option<u64>) -> String {
    let mut classes = vec!["log-row"];

    // Row background tint based on effect type
    if row.effect_id == EFFECT_DAMAGE {
        classes.push("log-damage-row");
    } else if row.effect_id == EFFECT_HEAL {
        classes.push("log-heal-row");
    }

    // Value color classes
    if row.value > 0 {
        if row.effect_id == EFFECT_DAMAGE {
            classes.push("log-damage");
        } else if row.effect_id == EFFECT_HEAL {
            classes.push("log-heal");
        }
    }

    // Critical hit
    if row.is_crit {
        classes.push("log-crit");
    }

    // Highlighted row (Find feature)
    if highlighted_row_idx == Some(row.row_idx) {
        classes.push("log-row-highlighted");
    }

    classes.join(" ")
}

/// Get CSS class for event type text.
fn event_type_class(row: &CombatLogRow) -> &'static str {
    match row.effect_type_id {
        EFFECT_TYPE_APPLYEFFECT => {
            if row.effect_id == EFFECT_DAMAGE {
                "log-type-damage"
            } else {
                "log-apply"
            }
        }
        EFFECT_TYPE_REMOVEEFFECT => "log-remove",
        EFFECT_TYPE_EVENT => "log-event",
        _ => "",
    }
}

#[component]
pub fn CombatLog(props: CombatLogProps) -> Element {
    // Mirror props into signals for reactivity
    let mut time_range_signal = use_signal(|| props.time_range.clone());
    let mut encounter_idx_signal = use_signal(|| props.encounter_idx);

    // Update signals when props change (runs on every render with new props)
    if *time_range_signal.read() != props.time_range {
        time_range_signal.set(props.time_range.clone());
    }
    if *encounter_idx_signal.read() != props.encounter_idx {
        encounter_idx_signal.set(props.encounter_idx);
    }

    // Determine whether to restore saved state:
    // - Only restore if no initial_search override (death tracker click)
    // - Only restore if encounter matches (filters are encounter-specific)
    let mut state = props.state;
    let should_restore = props.initial_search.is_none()
        && state.peek().encounter_idx == Some(props.encounter_idx);

    // Filter state - restore from saved state or use defaults
    let mut source_filter = use_signal(|| {
        if should_restore { state.peek().source_filter.clone() } else { None }
    });
    let mut target_filter = use_signal(|| {
        if should_restore { state.peek().target_filter.clone() } else { None }
    });
    let mut search_text = use_signal(|| {
        if let Some(ref search) = props.initial_search {
            search.clone()
        } else if should_restore {
            state.peek().search_text.clone()
        } else {
            String::new()
        }
    });

    // Event type filter checkboxes
    let mut filter_damage = use_signal(|| if should_restore { state.peek().filter_damage } else { true });
    let mut filter_healing = use_signal(|| if should_restore { state.peek().filter_healing } else { true });
    let mut filter_actions = use_signal(|| if should_restore { state.peek().filter_actions } else { true });
    let mut filter_effects = use_signal(|| if should_restore { state.peek().filter_effects } else { true });
    let mut filter_other = use_signal(|| if should_restore { state.peek().filter_other } else { true });

    // Show IDs toggle - NOW PERSISTED!
    let mut show_ids = use_signal(|| if should_restore { state.peek().show_ids } else { true });

    // Find feature - searches all data via backend query
    let mut find_text = use_signal(String::new);
    let mut find_debounce = use_signal(String::new);
    // Stores find match results with position and row_idx
    let mut find_matches = use_signal(Vec::<CombatLogFindMatch>::new);
    let mut find_current_idx = use_signal(|| 0usize);
    let mut highlighted_row = use_signal(|| None::<u64>);

    // Data state
    let mut rows = use_signal(Vec::<CombatLogRow>::new);
    let mut total_count = use_signal(|| 0u64);
    let mut source_names = use_signal(GroupedEntityNames::default);
    let mut target_names = use_signal(GroupedEntityNames::default);

    // Virtual scroll state - restore from saved state if same encounter
    let mut scroll_top = use_signal(|| {
        if should_restore { state.peek().scroll_offset } else { 0.0 }
    });

    // Track whether we need to restore DOM scroll position after first data load
    let mut needs_scroll_restore = use_signal(|| should_restore && state.peek().scroll_offset > 0.0);

    // Column widths for resizable columns (in pixels)
    let mut col_time = use_signal(|| 70.0f64);
    let mut col_source = use_signal(|| 110.0f64);
    let mut col_type = use_signal(|| 90.0f64);
    let mut col_target = use_signal(|| 110.0f64);
    let mut col_ability = use_signal(|| 280.0f64);
    let mut col_value = use_signal(|| 70.0f64);
    let mut col_abs = use_signal(|| 70.0f64);
    let mut col_over = use_signal(|| 70.0f64);
    let mut col_mit = use_signal(|| 60.0f64);
    let mut col_dmg_type = use_signal(|| 65.0f64);
    let mut col_threat = use_signal(|| 70.0f64);

    // Column resize dragging state
    let mut resizing_col = use_signal(|| None::<usize>);
    let mut resize_start_x = use_signal(|| 0.0f64);
    let mut resize_start_width = use_signal(|| 0.0f64);
    let mut container_height = use_signal(|| 500.0f64);
    let mut loaded_offset = use_signal(|| 0u64);

    // Debounced search
    let mut search_debounce = use_signal(String::new);

    // Track previous encounter to detect changes (for resetting state on encounter switch)
    let mut prev_encounter_idx = use_signal(|| props.encounter_idx);

    // Reset all UI state when encounter changes (not on initial mount)
    // This ensures nothing is remembered from the previous encounter
    use_effect(move || {
        let current = *encounter_idx_signal.read();
        let prev = *prev_encounter_idx.peek();

        if current != prev {
            prev_encounter_idx.set(current);

            // Reset scroll position
            scroll_top.set(0.0);
            loaded_offset.set(0);

            // Reset filters to defaults
            source_filter.set(None);
            target_filter.set(None);
            search_text.set(String::new());
            search_debounce.set(String::new());

            // Reset event type filters to defaults
            filter_damage.set(true);
            filter_healing.set(true);
            filter_actions.set(true);
            filter_effects.set(true);
            filter_other.set(true);

            // Clear data to trigger fresh load
            rows.set(vec![]);

            // Reset find feature state
            find_text.set(String::new());
            find_debounce.set(String::new());
            find_matches.set(vec![]);
            find_current_idx.set(0);
            highlighted_row.set(None);

            // Reset DOM scroll position
            if let Some(window) = web_sys::window()
                && let Some(doc) = window.document()
                && let Some(elem) = doc.get_element_by_id("combat-log-scroll")
            {
                elem.set_scroll_top(0);
            }
        }
    });

    // Load source/target names when encounter changes
    use_effect(move || {
        let idx = *encounter_idx_signal.read();
        spawn(async move {
            if let Some(sources) = api::query_source_names(Some(idx)).await {
                source_names.set(sources);
            }
            if let Some(targets) = api::query_target_names(Some(idx)).await {
                target_names.set(targets);
            }
        });
    });

    // Build event filters from checkboxes
    let build_event_filters = move || -> Option<CombatLogFilters> {
        let damage = *filter_damage.read();
        let healing = *filter_healing.read();
        let actions = *filter_actions.read();
        let effects = *filter_effects.read();
        let other = *filter_other.read();

        // If all are true, return None (no filtering needed)
        if damage && healing && actions && effects && other {
            return None;
        }

        Some(CombatLogFilters {
            damage,
            healing,
            actions,
            effects,
            other,
        })
    };

    // Load data when filters, time range, or encounter change
    use_effect(move || {
        let idx = *encounter_idx_signal.read();
        let tr = time_range_signal.read().clone();
        let source = source_filter.read().clone();
        let target = target_filter.read().clone();
        let search = search_debounce.read().clone();
        let search_opt = if search.is_empty() {
            None
        } else {
            Some(search)
        };
        let event_filters = build_event_filters();

        // Calculate load offset based on current scroll position
        let current_scroll = *scroll_top.peek();
        let load_offset = ((current_scroll / ROW_HEIGHT) as u64).saturating_sub(OVERSCAN as u64);
        loaded_offset.set(load_offset);

        spawn(async move {
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
                event_filters.as_ref(),
            )
            .await
            {
                total_count.set(count);
            }

            // Load page at computed offset
            if let Some(data) = api::query_combat_log(
                Some(idx),
                load_offset,
                PAGE_SIZE,
                source.as_deref(),
                target.as_deref(),
                search_opt.as_deref(),
                tr_opt,
                event_filters.as_ref(),
            )
            .await
            {
                rows.set(data);
            }
        });
    });

    // Restore DOM scroll position after initial data load (when navigating back to same encounter)
    use_effect(move || {
        let rows_loaded = !rows.read().is_empty();
        let needs_restore = *needs_scroll_restore.peek();
        
        if rows_loaded && needs_restore {
            needs_scroll_restore.set(false);
            let saved_scroll = *scroll_top.peek();
            
            if let Some(window) = web_sys::window()
                && let Some(doc) = window.document()
                && let Some(elem) = doc.get_element_by_id("combat-log-scroll")
            {
                elem.set_scroll_top(saved_scroll as i32);
            }
        }
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

    // Debounce find input
    use_effect({
        move || {
            let text = find_text.read().clone();
            spawn(async move {
                gloo_timers::future::TimeoutFuture::new(300).await;
                if *find_text.read() == text {
                    find_debounce.set(text);
                }
            });
        }
    });

    // Find feature: query backend for all matches when find text changes
    use_effect(move || {
        let find = find_debounce.read().clone();
        let idx = *encounter_idx_signal.read();
        let tr = time_range_signal.read().clone();
        let source = source_filter.read().clone();
        let target = target_filter.read().clone();
        let event_filters = build_event_filters();

        if find.is_empty() {
            find_matches.set(vec![]);
            find_current_idx.set(0);
            highlighted_row.set(None);
            return;
        }

        spawn(async move {
            let tr_opt = if tr.start == 0.0 && tr.end == 0.0 {
                None
            } else {
                Some(&tr)
            };

            if let Some(matches) = api::query_combat_log_find(
                Some(idx),
                &find,
                source.as_deref(),
                target.as_deref(),
                tr_opt,
                event_filters.as_ref(),
            )
            .await
            {
                find_current_idx.set(0);
                if let Some(first_match) = matches.first() {
                    highlighted_row.set(Some(first_match.row_idx));
                    // Scroll to center the first match in viewport
                    if let Some(window) = web_sys::window()
                        && let Some(doc) = window.document()
                        && let Some(elem) = doc.get_element_by_id("combat-log-scroll")
                        && let Some(html_elem) = elem.dyn_ref::<web_sys::HtmlElement>()
                    {
                        let container_h = html_elem.client_height() as f64;
                        let scroll_y = (first_match.pos as f64 * ROW_HEIGHT) - (container_h / 2.0) + (ROW_HEIGHT / 2.0);
                        elem.set_scroll_top(scroll_y.max(0.0) as i32);
                    }
                } else {
                    highlighted_row.set(None);
                }
                find_matches.set(matches);
            }
        });
    });


    // Continuously sync state back to parent - this ensures all state persists
    // Using .read() on all values creates subscriptions so this runs on ANY change
    use_effect(move || {
        let scroll = *scroll_top.read();
        let source = source_filter.read().clone();
        let target = target_filter.read().clone();
        let search = search_text.read().clone();
        let damage = *filter_damage.read();
        let healing = *filter_healing.read();
        let actions = *filter_actions.read();
        let effects = *filter_effects.read();
        let other = *filter_other.read();
        let ids = *show_ids.read();
        
        if let Ok(mut s) = state.try_write() {
            *s = CombatLogSessionState {
                encounter_idx: Some(*encounter_idx_signal.peek()),
                source_filter: source,
                target_filter: target,
                search_text: search,
                filter_damage: damage,
                filter_healing: healing,
                filter_actions: actions,
                filter_effects: effects,
                filter_other: other,
                show_ids: ids,
                scroll_offset: scroll,
            };
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
    use_effect(move || {
        let idx = *encounter_idx_signal.read();
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
            let event_filters = build_event_filters();

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
                    event_filters.as_ref(),
                )
                .await
                {
                    loaded_offset.set(new_offset);
                    rows.set(data);
                }
            });
        }
    });

    // Slice visible rows from loaded data (with bounds safety)
    let current_rows = rows.read();
    let offset = *loaded_offset.read() as usize;
    let visible_rows: Vec<CombatLogRow> = if !current_rows.is_empty() {
        let rel_start = start_idx.saturating_sub(offset).min(current_rows.len());
        let rel_end = end_idx.saturating_sub(offset).min(current_rows.len());
        if rel_start < rel_end {
            current_rows[rel_start..rel_end].to_vec()
        } else {
            vec![]
        }
    } else {
        vec![]
    };

    let sources_grouped = source_names.read().clone();
    let targets_grouped = target_names.read().clone();
    let show_ids_val = *show_ids.read();
    let highlighted_row_idx = *highlighted_row.read();
    let find_match_count = find_matches.read().len();
    let find_idx = *find_current_idx.read();

    rsx! {
        div { class: "combat-log-panel",
            // Filter bar - row 1
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
                    if !sources_grouped.friendly.is_empty() {
                        optgroup { label: "Friendly",
                            option { value: "__ALL_FRIENDLY__", "All Friendly" }
                            for name in sources_grouped.friendly.iter() {
                                option { value: "{name}", "{name}" }
                            }
                        }
                    }
                    if !sources_grouped.npcs.is_empty() {
                        optgroup { label: "NPCs",
                            option { value: "__ALL_NPCS__", "All NPCs" }
                            for name in sources_grouped.npcs.iter() {
                                option { value: "{name}", "{name}" }
                            }
                        }
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
                    if !targets_grouped.friendly.is_empty() {
                        optgroup { label: "Friendly",
                            option { value: "__ALL_FRIENDLY__", "All Friendly" }
                            for name in targets_grouped.friendly.iter() {
                                option { value: "{name}", "{name}" }
                            }
                        }
                    }
                    if !targets_grouped.npcs.is_empty() {
                        optgroup { label: "NPCs",
                            option { value: "__ALL_NPCS__", "All NPCs" }
                            for name in targets_grouped.npcs.iter() {
                                option { value: "{name}", "{name}" }
                            }
                        }
                    }
                }

                // Search input (filter)
                input {
                    class: "log-search",
                    r#type: "text",
                    placeholder: "Filter... (use OR)",
                    value: "{search_text}",
                    oninput: move |e| search_text.set(e.value()),
                }

                // Find group (searches all data via backend)
                div { class: "log-find-group",
                    input {
                        class: "log-find-input",
                        r#type: "text",
                        placeholder: "Find...",
                        value: "{find_text}",
                        oninput: move |e| find_text.set(e.value()),
                    }
                    button {
                        class: "find-nav-btn",
                        r#type: "button",
                        disabled: find_match_count == 0,
                        onclick: move |_| {
                            // Clone data out of signals to avoid borrow issues
                            let matches_vec = find_matches.read().clone();
                            if matches_vec.is_empty() {
                                return;
                            }
                            let current = *find_current_idx.read();
                            let prev = if current == 0 {
                                matches_vec.len().saturating_sub(1)
                            } else {
                                current - 1
                            };
                            let m = &matches_vec[prev];

                            // Now update state
                            find_current_idx.set(prev);
                            highlighted_row.set(Some(m.row_idx));

                            // Scroll to center match in viewport
                            if let Some(window) = web_sys::window()
                                && let Some(doc) = window.document()
                                && let Some(elem) = doc.get_element_by_id("combat-log-scroll")
                                && let Some(html_elem) = elem.dyn_ref::<web_sys::HtmlElement>()
                            {
                                let container_h = html_elem.client_height() as f64;
                                let scroll_y = (m.pos as f64 * ROW_HEIGHT) - (container_h / 2.0) + (ROW_HEIGHT / 2.0);
                                elem.set_scroll_top(scroll_y.max(0.0) as i32);
                            }
                        },
                        "▲"
                    }
                    button {
                        class: "find-nav-btn",
                        r#type: "button",
                        disabled: find_match_count == 0,
                        onclick: move |_| {
                            // Clone data out of signals to avoid borrow issues
                            let matches_vec = find_matches.read().clone();
                            if matches_vec.is_empty() {
                                return;
                            }
                            let current = *find_current_idx.read();
                            let next = if current + 1 >= matches_vec.len() {
                                0
                            } else {
                                current + 1
                            };
                            let m = &matches_vec[next];

                            // Now update state
                            find_current_idx.set(next);
                            highlighted_row.set(Some(m.row_idx));

                            // Scroll to center match in viewport
                            if let Some(window) = web_sys::window()
                                && let Some(doc) = window.document()
                                && let Some(elem) = doc.get_element_by_id("combat-log-scroll")
                                && let Some(html_elem) = elem.dyn_ref::<web_sys::HtmlElement>()
                            {
                                let container_h = html_elem.client_height() as f64;
                                let scroll_y = (m.pos as f64 * ROW_HEIGHT) - (container_h / 2.0) + (ROW_HEIGHT / 2.0);
                                elem.set_scroll_top(scroll_y.max(0.0) as i32);
                            }
                        },
                        "▼"
                    }
                    if find_match_count > 0 {
                        span { class: "find-count", "{find_idx + 1}/{find_match_count}" }
                    }
                }

                // Show IDs toggle
                label { class: "log-show-ids",
                    input {
                        r#type: "checkbox",
                        checked: show_ids_val,
                        onchange: move |e| {
                            let checked = e.checked();
                            show_ids.set(checked);
                            // Persist to config
                            spawn(async move {
                                if let Some(mut cfg) = api::get_config().await {
                                    cfg.show_log_ids = checked;
                                    let _ = api::update_config(&cfg).await;
                                }
                            });
                        },
                    }
                    "Show IDs"
                }

                // Row count
                span { class: "log-count", "{total} events" }
            }

            // Filter bar - row 2 (event type checkboxes)
            div { class: "log-event-filters",
                label { class: "log-filter-checkbox damage",
                    input {
                        r#type: "checkbox",
                        checked: *filter_damage.read(),
                        onchange: move |e| filter_damage.set(e.checked()),
                    }
                    "Damage"
                }
                label { class: "log-filter-checkbox healing",
                    input {
                        r#type: "checkbox",
                        checked: *filter_healing.read(),
                        onchange: move |e| filter_healing.set(e.checked()),
                    }
                    "Healing"
                }
                label { class: "log-filter-checkbox actions",
                    input {
                        r#type: "checkbox",
                        checked: *filter_actions.read(),
                        onchange: move |e| filter_actions.set(e.checked()),
                    }
                    "Actions"
                }
                label { class: "log-filter-checkbox effects",
                    input {
                        r#type: "checkbox",
                        checked: *filter_effects.read(),
                        onchange: move |e| filter_effects.set(e.checked()),
                    }
                    "Effects"
                }
                label { class: "log-filter-checkbox other",
                    input {
                        r#type: "checkbox",
                        checked: *filter_other.read(),
                        onchange: move |e| filter_other.set(e.checked()),
                    }
                    "Other"
                }
            }

            // Table container with virtual scrolling
            div {
                class: "log-table-container",
                id: "combat-log-scroll",
                onscroll: move |_| {
                    if let Some(window) = web_sys::window()
                        && let Some(doc) = window.document()
                        && let Some(elem) = doc.get_element_by_id("combat-log-scroll")
                        && let Some(html_elem) = elem.dyn_ref::<web_sys::HtmlElement>()
                    {
                        scroll_top.set(html_elem.scroll_top() as f64);
                        container_height.set(html_elem.client_height() as f64);
                    }
                },
                // Header row (sticky)
                div {
                    class: "log-header",
                    onmousemove: move |e| {
                        if let Some(col_idx) = *resizing_col.read() {
                            let delta = e.client_coordinates().x - *resize_start_x.read();
                            let new_width = (*resize_start_width.read() + delta).max(40.0);
                            match col_idx {
                                0 => col_time.set(new_width),
                                1 => col_source.set(new_width),
                                2 => col_type.set(new_width),
                                3 => col_target.set(new_width),
                                4 => col_ability.set(new_width),
                                5 => col_value.set(new_width),
                                6 => col_abs.set(new_width),
                                7 => col_over.set(new_width),
                                8 => col_mit.set(new_width),
                                9 => col_dmg_type.set(new_width),
                                10 => col_threat.set(new_width),
                                _ => {}
                            }
                        }
                    },
                    onmouseup: move |_| resizing_col.set(None),
                    onmouseleave: move |_| resizing_col.set(None),

                    div { class: "log-cell log-time", style: "width: {col_time}px; min-width: {col_time}px;", "Time" }
                    div {
                        class: "log-resize-handle",
                        onmousedown: move |e| {
                            e.prevent_default();
                            resizing_col.set(Some(0));
                            resize_start_x.set(e.client_coordinates().x);
                            resize_start_width.set(*col_time.read());
                        },
                    }
                    div { class: "log-cell log-source", style: "width: {col_source}px; min-width: {col_source}px;", "Source" }
                    div {
                        class: "log-resize-handle",
                        onmousedown: move |e| {
                            e.prevent_default();
                            resizing_col.set(Some(1));
                            resize_start_x.set(e.client_coordinates().x);
                            resize_start_width.set(*col_source.read());
                        },
                    }
                    div { class: "log-cell log-type", style: "width: {col_type}px; min-width: {col_type}px;", "Type" }
                    div {
                        class: "log-resize-handle",
                        onmousedown: move |e| {
                            e.prevent_default();
                            resizing_col.set(Some(2));
                            resize_start_x.set(e.client_coordinates().x);
                            resize_start_width.set(*col_type.read());
                        },
                    }
                    div { class: "log-cell log-target", style: "width: {col_target}px; min-width: {col_target}px;", "Target" }
                    div {
                        class: "log-resize-handle",
                        onmousedown: move |e| {
                            e.prevent_default();
                            resizing_col.set(Some(3));
                            resize_start_x.set(e.client_coordinates().x);
                            resize_start_width.set(*col_target.read());
                        },
                    }
                    div { class: "log-cell log-ability", style: "width: {col_ability}px; min-width: {col_ability}px;", "Ability" }
                    div {
                        class: "log-resize-handle",
                        onmousedown: move |e| {
                            e.prevent_default();
                            resizing_col.set(Some(4));
                            resize_start_x.set(e.client_coordinates().x);
                            resize_start_width.set(*col_ability.read());
                        },
                    }
                    div { class: "log-cell log-value", style: "width: {col_value}px; min-width: {col_value}px;", "Value" }
                    div {
                        class: "log-resize-handle",
                        onmousedown: move |e| {
                            e.prevent_default();
                            resizing_col.set(Some(5));
                            resize_start_x.set(e.client_coordinates().x);
                            resize_start_width.set(*col_value.read());
                        },
                    }
                    div { class: "log-cell log-absorbed", style: "width: {col_abs}px; min-width: {col_abs}px;", "Abs" }
                    div {
                        class: "log-resize-handle",
                        onmousedown: move |e| {
                            e.prevent_default();
                            resizing_col.set(Some(6));
                            resize_start_x.set(e.client_coordinates().x);
                            resize_start_width.set(*col_abs.read());
                        },
                    }
                    div { class: "log-cell log-overheal log-overheal-header", style: "width: {col_over}px; min-width: {col_over}px;", title: "Overheal", "Over" }
                    div {
                        class: "log-resize-handle",
                        onmousedown: move |e| {
                            e.prevent_default();
                            resizing_col.set(Some(7));
                            resize_start_x.set(e.client_coordinates().x);
                            resize_start_width.set(*col_over.read());
                        },
                    }
                    div { class: "log-cell log-mitigation", style: "width: {col_mit}px; min-width: {col_mit}px;", "Mit" }
                    div {
                        class: "log-resize-handle",
                        onmousedown: move |e| {
                            e.prevent_default();
                            resizing_col.set(Some(8));
                            resize_start_x.set(e.client_coordinates().x);
                            resize_start_width.set(*col_mit.read());
                        },
                    }
                    div { class: "log-cell log-dmg-type", style: "width: {col_dmg_type}px; min-width: {col_dmg_type}px;", "Type" }
                    div {
                        class: "log-resize-handle",
                        onmousedown: move |e| {
                            e.prevent_default();
                            resizing_col.set(Some(9));
                            resize_start_x.set(e.client_coordinates().x);
                            resize_start_width.set(*col_dmg_type.read());
                        },
                    }
                    div { class: "log-cell log-threat", style: "width: {col_threat}px; min-width: {col_threat}px;", "Threat" }
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
                                class: "{row_class(&row, highlighted_row_idx)}",
                                div { class: "log-cell log-time", style: "width: {col_time}px; min-width: {col_time}px;", "{format_time(row.time_secs)}" }
                                div { class: "log-cell log-source", style: "width: {col_source}px; min-width: {col_source}px;",
                                    "{row.source_name}"
                                    if show_ids_val && row.source_class_id != 0 {
                                        span { class: "log-id-suffix", " [{row.source_class_id}]" }
                                    }
                                }
                                div { class: "log-cell log-type {event_type_class(&row)}", style: "width: {col_type}px; min-width: {col_type}px;",
                                    "{readable_event_type(&row)}"
                                }
                                div { class: "log-cell log-target", style: "width: {col_target}px; min-width: {col_target}px;",
                                    "{row.target_name}"
                                    if show_ids_val && row.target_class_id != 0 {
                                        span { class: "log-id-suffix", " [{row.target_class_id}]" }
                                    }
                                }
                                div { class: "log-cell log-ability", style: "width: {col_ability}px; min-width: {col_ability}px;",
                                    if row.ability_id != 0 {
                                        AbilityIcon { key: "{row.ability_id}", ability_id: row.ability_id, size: 16 }
                                    }
                                    if !row.ability_name.is_empty() {
                                        "{row.ability_name}"
                                    } else {
                                        "{row.effect_name}"
                                    }
                                    if show_ids_val && row.ability_id != 0 {
                                        span { class: "log-id-suffix", " [{row.ability_id}]" }
                                    }
                                }
                                div { class: "log-cell log-value", style: "width: {col_value}px; min-width: {col_value}px;",
                                    if row.is_crit { "*" } else { "" }
                                    "{format_number(row.value)}"
                                }
                                div { class: "log-cell log-absorbed", style: "width: {col_abs}px; min-width: {col_abs}px;", "{format_number(row.absorbed)}" }
                                div { class: "log-cell log-overheal", style: "width: {col_over}px; min-width: {col_over}px;", "{format_number(row.overheal)}" }
                                div { class: "log-cell log-mitigation", style: "width: {col_mit}px; min-width: {col_mit}px;", "{readable_defense_type(row.defense_type_id)}" }
                                div { class: "log-cell log-dmg-type", style: "width: {col_dmg_type}px; min-width: {col_dmg_type}px;", "{format_damage_type(&row.damage_type)}" }
                                div { class: "log-cell log-threat", style: "width: {col_threat}px; min-width: {col_threat}px;",
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
