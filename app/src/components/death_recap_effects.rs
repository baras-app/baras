//! Death recap: the effects on the player over the recap window, one row per
//! (effect, source) with a bar on the same time axis as the HP sparkline.

use baras_types::{DeathRecapEffect, formatting};
use dioxus::prelude::*;

use super::death_recap_drawer::LedgerIcon;

/// The effect's windows cut at each stack change: (start, end, stacks), the
/// count only for effects that stack
fn stack_spans(e: &DeathRecapEffect) -> Vec<(f32, f32, Option<i32>)> {
    if e.stack_changes.is_empty() {
        return e.windows.iter().map(|&(a, b)| (a, b, None)).collect();
    }
    let mut out = Vec::new();
    for &(a, b) in &e.windows {
        let mut from = a;
        let mut stacks = e.stack_changes.iter().rfind(|&&(t, _)| t <= a).map_or(1, |&(_, n)| n);
        for &(t, n) in e.stack_changes.iter().filter(|&&(t, _)| t > a && t < b) {
            out.push((from, t, Some(stacks)));
            (from, stacks) = (t, n);
        }
        out.push((from, b, Some(stacks)));
    }
    out
}

#[component]
pub fn RecapEffects(effects: Vec<DeathRecapEffect>, window_start: f32, death_time: f32, european: bool) -> Element {
    // None = all, Some(true) = enemy-applied, Some(false) = friendly
    let mut side = use_signal(|| None::<bool>);
    if effects.is_empty() {
        return rsx! {};
    }
    let span = (death_time - window_start).max(0.001);
    let pct = move |t: f32| ((t - window_start) / span * 100.0).clamp(0.0, 100.0);
    let secs = move |s: f32| formatting::format_decimal(s, 1, european);
    let both_sides = effects.iter().any(|e| e.from_npc) && effects.iter().any(|e| !e.from_npc);
    let shown = if both_sides { side() } else { None };

    rsx! {
        // A short list keeps its natural height: the minimum would pad it
        div { class: if effects.len() < 4 { "recap-effects-wrap few" } else { "recap-effects-wrap" },
            div { class: "recap-ledger-head",
                h5 { "Effects on Player" }
                if both_sides {
                    div { class: "filter-pills",
                        for (label, pick) in [("All", None), ("Enemy", Some(true)), ("Friendly", Some(false))] {
                            button {
                                class: if shown == pick { "seg active" } else { "seg" },
                                onclick: move |_| side.set(pick),
                                "{label}"
                            }
                        }
                    }
                }
            }
            div { class: "recap-effects",
                for e in effects.iter().filter(|e| shown.is_none_or(|npc| e.from_npc == npc)) {
                    {
                        let (first, last) = (e.windows[0], e.windows[e.windows.len() - 1]);
                        let applied = format!("Applied {}s before the death", secs(death_time - first.0));
                        let (when, title) = if e.active_at_death {
                            (format!("for {}s", secs(death_time - last.0)), format!("{applied} · still up when it happened"))
                        } else {
                            let ended = secs(death_time - last.1);
                            (format!("ended -{ended}s"), format!("{applied} · ended {ended}s before it"))
                        };
                        let (icon_class, row_class) = if e.from_npc {
                            ("ledger-icon stat-damage", "ledger-row effect-row enemy")
                        } else {
                            ("ledger-icon stat-heal", "ledger-row effect-row")
                        };
                        rsx! {
                            div { key: "{e.effect_id}:{e.source_name}", class: "{row_class}", title: "{title}",
                                LedgerIcon { ability_id: e.ability_id, icon: "fa-solid fa-circle", icon_class }
                                span { class: "ledger-ability",
                                    "{e.effect_name}"
                                    if e.stacks > 1 {
                                        span { class: "effect-stacks", " ×{e.stacks}" }
                                    }
                                    if !e.source_name.is_empty() {
                                        span { class: "recap-source", " — {e.source_name}" }
                                    }
                                }
                                span { class: if e.stack_changes.is_empty() { "effect-track" } else { "effect-track stacked" },
                                    for (a, b, stacks) in stack_spans(e) {
                                        span {
                                            // Squared off on the left when it began before the window
                                            class: if a < window_start { "effect-seg from-before" } else { "effect-seg" },
                                            style: "left: {pct(a):.2}%; width: {pct(b) - pct(a):.2}%",
                                            if let Some(n) = stacks {
                                                "{n}"
                                            }
                                        }
                                    }
                                }
                                span { class: "effect-when", "{when}" }
                            }
                        }
                    }
                }
            }
        }
    }
}
