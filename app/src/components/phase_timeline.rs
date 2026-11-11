//! Phase Timeline Filter Component
//!
//! A timeline bar showing encounter duration with phase segments.
//! Allows selecting a phase or dragging an arbitrary time range.

use dioxus::prelude::*;
use wasm_bindgen::{JsCast, prelude::*};

use crate::api::{EncounterTimeline, PhaseSegment, TimeRange};

fn format_time(secs: f32) -> String {
    let mins = (secs / 60.0) as i32;
    let secs = (secs % 60.0) as i32;
    format!("{}:{:02}", mins, secs)
}

/// Parse a time string in M:SS, M:SS.d, or bare seconds format.
fn parse_time(s: &str) -> Option<f32> {
    let s = s.trim();
    if let Some((min_str, sec_str)) = s.split_once(':') {
        let mins: f32 = min_str.parse().ok()?;
        let secs: f32 = sec_str.parse().ok()?;
        Some(mins * 60.0 + secs)
    } else {
        s.parse().ok()
    }
}

/// Generate a consistent HSL color based on phase_id string.
/// All instances of the same phase type will get the same color.
/// Uses muted colors; lightness follows the active theme via `--phase-l`.
fn phase_color(phase_id: &str) -> String {
    // Simple hash function to get a consistent hue
    let hash: u32 = phase_id
        .bytes()
        .fold(0u32, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u32));
    let hue = hash % 360;
    // Muted saturation and moderate lightness for subtle distinction
    let sat = 25 + (hash % 15); // 25-40% (muted)
    // Lightness offset on top of the theme's --phase-l (30% dark, 78% light)
    let light = hash % 10;
    format!("hsl({}, {}%, calc(var(--phase-l) + {}%))", hue, sat, light)
}

/// A phase is selected when the range matches it exactly. A full range never
/// counts, even if a single phase happens to span the entire fight.
fn is_phase_selected(range: TimeRange, duration: f32, phase: &PhaseSegment) -> bool {
    !range.is_full(duration)
        && (range.start - phase.start_secs).abs() < 0.1
        && (range.end - phase.end_secs).abs() < 0.1
}

#[derive(Props, Clone, PartialEq)]
pub struct PhaseTimelineProps {
    /// Timeline data (duration + phases)
    pub timeline: EncounterTimeline,
    /// Current selected time range
    pub range: TimeRange,
    /// Callback when range changes
    pub on_range_change: EventHandler<TimeRange>,
}

#[component]
pub fn PhaseTimelineFilter(props: PhaseTimelineProps) -> Element {
    let duration = props.timeline.duration_secs;
    let phases = &props.timeline.phases;
    let range = props.range;

    // Drag state: start_time when dragging
    let mut drag_start = use_signal(|| None::<f32>);
    let mut committed_range = use_signal(|| None::<TimeRange>); // Persists after drag until acknowledged
    let mut was_dragged = use_signal(|| false);

    // Editing state for inline time inputs
    let mut editing_start = use_signal(|| None::<String>);
    let mut editing_end = use_signal(|| None::<String>);

    // Track current duration from props so persistent closures see latest value
    let mut current_duration = use_signal(|| duration);
    if *current_duration.peek() != duration {
        current_duration.set(duration);
    }

    // JS function refs for cleanup on unmount
    let mut move_fn: Signal<Option<js_sys::Function>> = use_signal(|| None);
    let mut up_fn: Signal<Option<js_sys::Function>> = use_signal(|| None);

    // Calculate percentage position for a time value
    let time_to_pct = |t: f32| -> f32 {
        if duration > 0.0 {
            (t / duration) * 100.0
        } else {
            0.0
        }
    };

    // Handle clicking on a phase segment or chip; clicking the selected phase clears it
    let select_phase = move |phase: &PhaseSegment| {
        let next = if is_phase_selected(range, duration, phase) {
            TimeRange::full(duration)
        } else {
            TimeRange::new(phase.start_secs, phase.end_secs)
        };
        props.on_range_change.call(next);
    };

    // Handle reset to full range
    let reset_range = move |_| {
        committed_range.set(None);
        props.on_range_change.call(TimeRange::full(duration));
    };

    // Helper: convert client X to time value using track bounds
    let client_x_to_time = move |client_x: f64| -> Option<f32> {
        if let Some(window) = web_sys::window()
            && let Some(document) = window.document()
            && let Some(el) = document.get_element_by_id("phase-timeline-track")
        {
            let rect = el.get_bounding_client_rect();
            let x = client_x - rect.left();
            let width = rect.width();
            if width > 0.0 && duration > 0.0 {
                let pct = (x / width).clamp(0.0, 1.0);
                return Some((pct as f32) * duration);
            }
        }
        None
    };

    // Mouse down on track - start drag
    let on_track_mousedown = {
        move |e: MouseEvent| {
            if let Some(time) = client_x_to_time(e.client_coordinates().x) {
                drag_start.set(Some(time));
                committed_range.set(Some(TimeRange::new(time, time)));
            }
        }
    };

    // Persistent document listeners registered once on mount, removed on unmount.
    // Closures check drag_start at runtime and early-return when not dragging,
    // so only 2 closures exist total (not per-drag).
    use_effect(move || {
        let window = match web_sys::window() {
            Some(w) => w,
            None => return,
        };
        let document = match window.document() {
            Some(d) => d,
            None => return,
        };

        // Mousemove handler
        let drag_start_mv = drag_start;
        let mut committed_range_mv = committed_range;
        let dur_mv = current_duration;
        let on_mousemove =
            Closure::<dyn FnMut(web_sys::MouseEvent)>::new(move |e: web_sys::MouseEvent| {
                let Ok(drag_guard) = drag_start_mv.try_read() else {
                    return;
                };
                let Some(start_time) = *drag_guard else {
                    return;
                };
                let duration = *dur_mv.peek();
                let Some(el) = web_sys::window()
                    .and_then(|w| w.document())
                    .and_then(|d| d.get_element_by_id("phase-timeline-track"))
                else {
                    return;
                };

                let rect = el.get_bounding_client_rect();
                let x = e.client_x() as f64 - rect.left();
                let width = rect.width();
                if width > 0.0 && duration > 0.0 {
                    let pct = (x / width).clamp(0.0, 1.0);
                    let current_time = (pct as f32) * duration;
                    let (start, end) = if current_time < start_time {
                        (current_time, start_time)
                    } else {
                        (start_time, current_time)
                    };
                    let _ = committed_range_mv
                        .try_write()
                        .map(|mut w| *w = Some(TimeRange::new(start, end)));
                }
            });

        // Mouseup handler
        let mut drag_start_up = drag_start;
        let mut committed_range_up = committed_range;
        let mut was_dragged_up = was_dragged;
        let dur_up = current_duration;
        let on_range_change = props.on_range_change.clone();
        let on_mouseup =
            Closure::<dyn FnMut(web_sys::MouseEvent)>::new(move |e: web_sys::MouseEvent| {
                let Ok(drag_guard) = drag_start_up.try_read() else {
                    return;
                };
                let Some(start_time) = *drag_guard else {
                    return;
                };
                drop(drag_guard); // Release read lock before writing

                let duration = *dur_up.peek();
                let Some(el) = web_sys::window()
                    .and_then(|w| w.document())
                    .and_then(|d| d.get_element_by_id("phase-timeline-track"))
                else {
                    let _ = drag_start_up.try_write().map(|mut w| {
                        *w = None;
                    });
                    return;
                };

                let rect = el.get_bounding_client_rect();
                let x = e.client_x() as f64 - rect.left();
                let width = rect.width();
                if width > 0.0 && duration > 0.0 {
                    let pct = (x / width).clamp(0.0, 1.0);
                    let end_time = (pct as f32) * duration;

                    let (start, end) = if end_time < start_time {
                        (end_time, start_time)
                    } else {
                        (start_time, end_time)
                    };

                    // Only commit real drags here. Plain clicks are handled by the
                    // click handlers (phase segment selects itself, bare track resets),
                    // avoiding a redundant range change that races the segment onclick.
                    if (end - start).abs() >= 1.0 {
                        // Mark as dragged so phase/track onclick doesn't clobber
                        let _ = was_dragged_up.try_write().map(|mut w| *w = true);
                        let final_range = TimeRange::new(start, end);
                        let _ = committed_range_up
                            .try_write()
                            .map(|mut w| *w = Some(final_range));
                        on_range_change.call(final_range);
                    } else {
                        let _ = committed_range_up.try_write().map(|mut w| *w = None);
                    }
                }
                let _ = drag_start_up.try_write().map(|mut w| {
                    *w = None;
                });
            });

        // Register listeners and store function refs for cleanup
        let move_func = on_mousemove.as_ref().unchecked_ref::<js_sys::Function>().clone();
        let up_func = on_mouseup.as_ref().unchecked_ref::<js_sys::Function>().clone();
        let _ = document.add_event_listener_with_callback("mousemove", &move_func);
        let _ = document.add_event_listener_with_callback("mouseup", &up_func);
        move_fn.set(Some(move_func));
        up_fn.set(Some(up_func));

        // Only 2 closures total, not per-drag
        on_mousemove.forget();
        on_mouseup.forget();
    });

    // Remove document listeners on unmount
    use_drop(move || {
        if let Some(window) = web_sys::window()
            && let Some(document) = window.document()
        {
            if let Some(f) = move_fn.peek().as_ref() {
                let _ = document.remove_event_listener_with_callback("mousemove", f);
            }
            if let Some(f) = up_fn.peek().as_ref() {
                let _ = document.remove_event_listener_with_callback("mouseup", f);
            }
        }
    });

    // During drag use committed_range, otherwise use props range
    let is_dragging = drag_start.read().is_some();
    let display_range = if is_dragging {
        committed_range.read().unwrap_or(range)
    } else {
        range // Always use props range when not dragging
    };

    // Display values for editable time inputs
    let start_display = editing_start.read().clone().unwrap_or_else(|| format_time(display_range.start));
    let end_display = editing_end.read().clone().unwrap_or_else(|| format_time(display_range.end));

    rsx! {
        div { class: "phase-timeline",
            // Compact row: track + range display
            div { class: "phase-timeline-row",
                // Timeline track with phases (interactive)
                div {
                    id: "phase-timeline-track",
                    class: "phase-timeline-track",
                    onmousedown: on_track_mousedown,
                    // Click on bare track (phase segments stop propagation) resets to full
                    onclick: move |_| {
                        if *was_dragged.peek() {
                            was_dragged.set(false);
                            return;
                        }
                        committed_range.set(None);
                        props.on_range_change.call(TimeRange::full(duration));
                    },

                    // Render phase segments
                    for phase in phases.iter() {
                        {
                            let left = time_to_pct(phase.start_secs);
                            let width = time_to_pct(phase.end_secs - phase.start_secs);
                            let is_selected = is_phase_selected(range, duration, phase);
                            let phase_clone = phase.clone();
                            let bg_color = phase_color(&phase.phase_id);

                            rsx! {
                                div {
                                    class: if is_selected { "phase-segment selected" } else { "phase-segment" },
                                    // Color goes through a custom property: Dioxus re-applies stale longhands
                                    // when a var()-based `background` shorthand reads back empty in WebKit
                                    style: "left: {left}%; width: {width}%; --seg-color: {bg_color};",
                                    title: "{phase.phase_name} ({format_time(phase.start_secs)} - {format_time(phase.end_secs)})",
                                    onclick: move |e| {
                                        e.stop_propagation();
                                        if *was_dragged.peek() {
                                            was_dragged.set(false);
                                            return;
                                        }
                                        select_phase(&phase_clone);
                                    },

                                    // Show time marker + abbreviated name if wide enough
                                    if width > 10.0 {
                                        span { class: "phase-time", "{format_time(phase.start_secs)}" }
                                        span { class: "phase-label", "{phase.phase_name}" }
                                    } else if width > 5.0 {
                                        span { class: "phase-time", "{format_time(phase.start_secs)}" }
                                    }
                                }
                            }
                        }
                    }

                    // Selection overlay
                    {
                        let left = time_to_pct(display_range.start);
                        let raw_width = time_to_pct(display_range.end - display_range.start);
                        let width = if raw_width < 1.0 { 1.0 } else { raw_width };
                        let is_visible = !display_range.is_full(duration) || is_dragging;
                        let class_name = if is_dragging { "phase-timeline-selection preview" } else { "phase-timeline-selection" };

                        rsx! {
                            if is_visible {
                                div {
                                    class: "{class_name}",
                                    style: "left: {left}%; width: {width}%;",
                                }
                            }
                        }
                    }
                }
            }

            // Time axis below the track (kept out of the track so labels never collide)
            div { class: "phase-timeline-axis",
                span { "0:00" }
                span { "{format_time(duration / 2.0)}" }
                span { "{format_time(duration)}" }
            }

            // Controls row: range inputs + phase chips inline
            div { class: "phase-timeline-controls",
                // Range inputs
                div { class: "phase-timeline-range",
                    input {
                        class: "phase-timeline-range-value",
                        r#type: "text",
                        size: 6,
                        value: "{start_display}",
                        onfocus: move |_| {
                            editing_start.set(Some(format_time(range.start)));
                        },
                        oninput: move |e: Event<FormData>| {
                            editing_start.set(Some(e.value()));
                        },
                        onblur: move |_| {
                            if let Some(ref text) = *editing_start.read() {
                                if let Some(t) = parse_time(text) {
                                    let t = t.clamp(0.0, duration);
                                    if t < range.end {
                                        props.on_range_change.call(TimeRange::new(t, range.end));
                                    }
                                }
                            }
                            editing_start.set(None);
                        },
                        onkeydown: move |e: Event<KeyboardData>| {
                            match e.key() {
                                Key::Enter => {
                                    if let Some(ref text) = *editing_start.read() {
                                        if let Some(t) = parse_time(text) {
                                            let t = t.clamp(0.0, duration);
                                            if t < range.end {
                                                props.on_range_change.call(TimeRange::new(t, range.end));
                                            }
                                        }
                                    }
                                    editing_start.set(None);
                                }
                                Key::Escape => editing_start.set(None),
                                Key::ArrowUp => {
                                    e.prevent_default();
                                    let cur = editing_start.read().as_ref()
                                        .and_then(|t| parse_time(t))
                                        .unwrap_or(range.start);
                                    let v = (cur + 1.0).min(range.end - 1.0).max(0.0);
                                    props.on_range_change.call(TimeRange::new(v, range.end));
                                    editing_start.set(Some(format_time(v)));
                                }
                                Key::ArrowDown => {
                                    e.prevent_default();
                                    let cur = editing_start.read().as_ref()
                                        .and_then(|t| parse_time(t))
                                        .unwrap_or(range.start);
                                    let v = (cur - 1.0).max(0.0);
                                    props.on_range_change.call(TimeRange::new(v, range.end));
                                    editing_start.set(Some(format_time(v)));
                                }
                                _ => {}
                            }
                        },
                    }
                    span { class: "phase-timeline-range-separator", "\u{2014}" }
                    input {
                        class: "phase-timeline-range-value",
                        r#type: "text",
                        size: 6,
                        value: "{end_display}",
                        onfocus: move |_| {
                            editing_end.set(Some(format_time(range.end)));
                        },
                        oninput: move |e: Event<FormData>| {
                            editing_end.set(Some(e.value()));
                        },
                        onblur: move |_| {
                            if let Some(ref text) = *editing_end.read() {
                                if let Some(t) = parse_time(text) {
                                    let t = t.clamp(0.0, duration);
                                    if t > range.start {
                                        props.on_range_change.call(TimeRange::new(range.start, t));
                                    }
                                }
                            }
                            editing_end.set(None);
                        },
                        onkeydown: move |e: Event<KeyboardData>| {
                            match e.key() {
                                Key::Enter => {
                                    if let Some(ref text) = *editing_end.read() {
                                        if let Some(t) = parse_time(text) {
                                            let t = t.clamp(0.0, duration);
                                            if t > range.start {
                                                props.on_range_change.call(TimeRange::new(range.start, t));
                                            }
                                        }
                                    }
                                    editing_end.set(None);
                                }
                                Key::Escape => editing_end.set(None),
                                Key::ArrowUp => {
                                    e.prevent_default();
                                    let cur = editing_end.read().as_ref()
                                        .and_then(|t| parse_time(t))
                                        .unwrap_or(range.end);
                                    let v = (cur + 1.0).min(duration);
                                    props.on_range_change.call(TimeRange::new(range.start, v));
                                    editing_end.set(Some(format_time(v)));
                                }
                                Key::ArrowDown => {
                                    e.prevent_default();
                                    let cur = editing_end.read().as_ref()
                                        .and_then(|t| parse_time(t))
                                        .unwrap_or(range.end);
                                    let v = (cur - 1.0).max(range.start + 1.0);
                                    props.on_range_change.call(TimeRange::new(range.start, v));
                                    editing_end.set(Some(format_time(v)));
                                }
                                _ => {}
                            }
                        },
                    }

                    if !range.is_full(duration) {
                        button {
                            class: "phase-timeline-reset",
                            onclick: reset_range,
                            "\u{2715}"
                        }
                    }
                }

                // Phase legend chips (inline with range)
                if !phases.is_empty() {
                    div { class: "phase-chips",
                        for phase in phases.iter() {
                            {
                                let is_active = is_phase_selected(range, duration, phase);
                                let phase_clone = phase.clone();
                                let bg_color = phase_color(&phase.phase_id);

                                rsx! {
                                    button {
                                        class: if is_active { "phase-chip active" } else { "phase-chip" },
                                        style: "--chip-color: {bg_color};",
                                        onclick: move |_| select_phase(&phase_clone),

                                        "{phase.phase_name}"
                                        if phase.instance > 1 {
                                            span { class: "chip-instance", " ({phase.instance})" }
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
