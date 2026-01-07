//! Phase Timeline Filter Component
//!
//! A timeline bar showing encounter duration with phase segments.
//! Allows selecting a phase or dragging an arbitrary time range.

use dioxus::prelude::*;

use crate::api::{EncounterTimeline, PhaseSegment, TimeRange};

fn format_time(secs: f32) -> String {
    let mins = (secs / 60.0) as i32;
    let secs = (secs % 60.0) as i32;
    format!("{}:{:02}", mins, secs)
}

/// Generate a consistent HSL color based on phase_id string.
/// All instances of the same phase type will get the same color.
/// Uses muted colors that blend with the dark UI theme.
fn phase_color(phase_id: &str) -> String {
    // Simple hash function to get a consistent hue
    let hash: u32 = phase_id
        .bytes()
        .fold(0u32, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u32));
    let hue = hash % 360;
    // Muted saturation and moderate lightness for subtle distinction
    let sat = 25 + (hash % 15); // 25-40% (muted)
    let light = 30 + (hash % 10); // 30-40% (darker, subtle)
    format!("hsl({}, {}%, {}%)", hue, sat, light)
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

    // Calculate percentage position for a time value
    let time_to_pct = |t: f32| -> f32 {
        if duration > 0.0 {
            (t / duration) * 100.0
        } else {
            0.0
        }
    };

    // Handle clicking on a phase segment
    let select_phase = move |phase: &PhaseSegment| {
        props
            .on_range_change
            .call(TimeRange::new(phase.start_secs, phase.end_secs));
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

    // Use effect to handle global mouse events during drag
    use_effect(move || {
        let is_dragging = drag_start.read().is_some();
        if !is_dragging {
            return;
        }

        // Add document-level listeners for mousemove and mouseup
        let window = match web_sys::window() {
            Some(w) => w,
            None => return,
        };
        let document = match window.document() {
            Some(d) => d,
            None => return,
        };

        use wasm_bindgen::JsCast;
        use wasm_bindgen::prelude::*;

        // Mousemove handler
        let drag_start_clone = drag_start.clone();
        let mut committed_range_clone = committed_range.clone();
        let on_mousemove =
            Closure::<dyn FnMut(web_sys::MouseEvent)>::new(move |e: web_sys::MouseEvent| {
                // Use try_read to handle signal being dropped when component unmounts
                let Ok(drag_guard) = drag_start_clone.try_read() else {
                    return;
                };
                let Some(start_time) = *drag_guard else {
                    return;
                };
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
                    let _ = committed_range_clone
                        .try_write()
                        .map(|mut w| *w = Some(TimeRange::new(start, end)));
                }
            });

        // Mouseup handler
        let mut drag_start_clone2 = drag_start.clone();
        let mut committed_range_clone2 = committed_range.clone();
        let on_range_change = props.on_range_change.clone();
        let on_mouseup =
            Closure::<dyn FnMut(web_sys::MouseEvent)>::new(move |e: web_sys::MouseEvent| {
                // Use try_read to handle signal being dropped when component unmounts
                let Ok(drag_guard) = drag_start_clone2.try_read() else {
                    return;
                };
                let Some(start_time) = *drag_guard else {
                    // Not dragging, just return
                    return;
                };
                drop(drag_guard); // Release read lock before writing

                let Some(el) = web_sys::window()
                    .and_then(|w| w.document())
                    .and_then(|d| d.get_element_by_id("phase-timeline-track"))
                else {
                    let _ = drag_start_clone2.try_write().map(|mut w| {
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

                    // If just a click (no drag), reset to full
                    let final_range = if (end - start).abs() < 1.0 {
                        TimeRange::full(duration)
                    } else {
                        TimeRange::new(start, end)
                    };

                    let _ = committed_range_clone2
                        .try_write()
                        .map(|mut w| *w = Some(final_range));
                    on_range_change.call(final_range);
                }
                let _ = drag_start_clone2.try_write().map(|mut w| {
                    *w = None;
                });
            });

        // Add listeners
        let _ = document
            .add_event_listener_with_callback("mousemove", on_mousemove.as_ref().unchecked_ref());
        let _ = document
            .add_event_listener_with_callback("mouseup", on_mouseup.as_ref().unchecked_ref());

        // Store closures to prevent dropping
        on_mousemove.forget();
        on_mouseup.forget();
    });

    // During drag use committed_range, otherwise use props range
    let is_dragging = drag_start.read().is_some();
    let display_range = if is_dragging {
        committed_range.read().unwrap_or(range)
    } else {
        range // Always use props range when not dragging
    };

    rsx! {
        div { class: "phase-timeline",
            // Compact row: track + range display
            div { class: "phase-timeline-row",
                // Timeline track with phases (interactive)
                div {
                    id: "phase-timeline-track",
                    class: "phase-timeline-track",
                    onmousedown: on_track_mousedown,

                    // Time markers inside the track
                    span { class: "track-marker start", "0:00" }
                    span { class: "track-marker mid", "{format_time(duration / 2.0)}" }
                    span { class: "track-marker end", "{format_time(duration)}" }

                    // Render phase segments
                    for phase in phases.iter() {
                        {
                            let left = time_to_pct(phase.start_secs);
                            let width = time_to_pct(phase.end_secs - phase.start_secs);
                            let is_selected = (range.start - phase.start_secs).abs() < 0.1
                                && (range.end - phase.end_secs).abs() < 0.1;
                            let phase_clone = phase.clone();
                            let bg_color = phase_color(&phase.phase_id);

                            rsx! {
                                div {
                                    class: if is_selected { "phase-segment selected" } else { "phase-segment" },
                                    style: "left: {left}%; width: {width}%; background: {bg_color};",
                                    title: "{phase.phase_name} ({format_time(phase.start_secs)} - {format_time(phase.end_secs)})",
                                    onclick: move |e| {
                                        e.stop_propagation();
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

                // Range display + reset (inline)
                div { class: "phase-timeline-range",
                    span { class: "phase-timeline-range-value", "{format_time(display_range.start)}" }
                    span { class: "phase-timeline-range-separator", "—" }
                    span { class: "phase-timeline-range-value", "{format_time(display_range.end)}" }

                    if !range.is_full(duration) {
                        button {
                            class: "phase-timeline-reset",
                            onclick: reset_range,
                            "✕"
                        }
                    }
                }
            }

            // Phase legend chips (compact)
            if !phases.is_empty() {
                div { class: "phase-chips",
                    for phase in phases.iter() {
                        {
                            let is_active = (range.start - phase.start_secs).abs() < 0.1
                                && (range.end - phase.end_secs).abs() < 0.1;
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
