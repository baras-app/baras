//! Taunts and threat drops in time order, with what each taunt did.

use dioxus::prelude::*;

use baras_types::{TauntOutcome, ThreatEvent, ThreatEventKind, ThreatEventTarget, formatting};

use super::{Roster, mmss};
use crate::components::ability_icon::AbilityIcon;
use crate::types::EnemyViewsState;

/// What `source`'s taunt did to one enemy (the text follows the enemy's
/// name), or how much threat a drop removed
fn result(t: &ThreatEventTarget, source: &str, european: bool) -> String {
    let mut text = match t.outcome {
        Some(TauntOutcome::TookAggro) if t.other.is_empty() => format!("turned to {source}"),
        Some(TauntOutcome::TookAggro) => format!("pulled off {}", t.other),
        Some(TauntOutcome::AlreadyHad) => format!("was already on {source}"),
        Some(TauntOutcome::NoSwitch) => format!("turned to {} instead", t.other),
        None if t.amount < 0.0 => formatting::format_compact(t.amount as i64, european),
        None => String::new(),
    };
    if !t.overlapped.is_empty() {
        text.push_str(&format!(", while {}'s taunt was still on it", t.overlapped));
    }
    text
}

#[component]
pub fn ThreatEvents(
    events: Vec<ThreatEvent>,
    /// The enemy the list is narrowed to
    on: Option<String>,
    state: Signal<EnemyViewsState>,
    roster: Roster,
    european: bool,
) -> Element {
    let shown = state.read().aggro_event_kind;
    let rows: Vec<&ThreatEvent> = events.iter().filter(|e| shown.is_none_or(|k| k == e.kind)).collect();
    let pills = [
        ("All", None),
        ("Taunts", Some(ThreatEventKind::Taunt)),
        ("AoE taunts", Some(ThreatEventKind::AoeTaunt)),
        ("Drops", Some(ThreatEventKind::Drop)),
    ];

    rsx! {
        div { class: "threat-panel",
            div { class: "uptime-toolbar",
                span { class: "uptime-title",
                    "Taunts and threat drops"
                    if let Some(on) = on {
                        " on {on}"
                    }
                }
                div { class: "filter-pills",
                    for (label, kind) in pills {
                        button {
                            class: if shown == kind { "seg active" } else { "seg" },
                            onclick: move |_| state.with_mut(|s| s.aggro_event_kind = kind),
                            "{label}"
                        }
                    }
                }
            }
            if rows.is_empty() {
                div { class: "uptime-empty", "None in this range" }
            } else {
                div { class: "threat-table-box",
                    table { class: "threat-table",
                        thead {
                            tr {
                                th { class: "num threat-time", "Time" }
                                th { class: "threat-name", "Player" }
                                th { class: "threat-name", "Ability" }
                                th { class: "threat-result", "Enemy and result" }
                            }
                        }
                        tbody {
                            for e in rows {
                                tr { class: if e.kind == ThreatEventKind::Drop { "threat-drop" },
                                    td { class: "num threat-time", "{mmss(e.time_secs)}" }
                                    td { title: "{e.source}",
                                        span { class: "name-with-icon",
                                            {roster.role(&e.source)}
                                            {roster.discipline(&e.source)}
                                            span { class: "name-text", "{e.source}" }
                                        }
                                    }
                                    td { title: "{e.ability_name}",
                                        span { class: "name-with-icon",
                                            if e.ability_id != 0 {
                                                AbilityIcon { ability_id: e.ability_id, size: 16 }
                                            }
                                            span { class: "name-text", "{e.ability_name}" }
                                        }
                                    }
                                    td { class: "threat-result",
                                        for t in e.targets.iter() {
                                            div {
                                                strong { "{t.name}" }
                                                span { class: match t.outcome {
                                                        Some(TauntOutcome::TookAggro) => "threat-outcome took",
                                                        _ => "threat-outcome",
                                                    },
                                                    " {result(t, &e.source, european)}"
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
