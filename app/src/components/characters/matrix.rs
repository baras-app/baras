//! Lockout matrix: characters down, operations across, one segment per
//! boss in each cell. Filled segments are locked until the next reset.

use dioxus::prelude::*;

use baras_types::characters::{CharacterRow, Lockout, OperationRef, operation_key};

use crate::components::class_icons::get_role_icon;

use super::lockouts::{tier_short, until};
use super::table::FactionIcon;

const TIERS: [&str; 3] = ["Story", "Veteran", "Master"];

/// The lockout for one operation at one mode, if any
fn lock_at<'a>(row: &'a CharacterRow, op_key: &str, tier: &str) -> Option<&'a Lockout> {
    row.lockouts
        .iter()
        .find(|l| operation_key(l.area_id, &l.operation) == op_key && l.tier == tier)
}

/// One mode's bar: a segment per boss, filled while locked
#[component]
fn LockBar(operation: String, tier: &'static str, bosses: usize, lock: Option<Lockout>, label: bool) -> Element {
    let total = lock.as_ref().filter(|l| l.total > 0).map_or(bosses, |l| l.total as usize);
    let locked = lock.as_ref().map_or(0, |l| l.locked.len());
    let title = match &lock {
        Some(l) => format!(
            "{operation} · {}{}\n{locked}/{} locked: {}\nResets in {}",
            tier_short(tier),
            if l.manual { " (marked by hand)" } else { "" },
            if total > 0 { total.to_string() } else { "?".into() },
            l.locked.join(", "),
            until(l.expires_ms)
        ),
        None => format!("{operation} · {} · free", tier_short(tier)),
    };
    rsx! {
        div { class: "lock-bar tier-{tier.to_lowercase()}", class: if locked > 0 { "locked" }, title: "{title}",
            if label { span { class: "lock-bar-tier", {tier_short(tier)} } }
            if total == 0 {
                span { class: "matrix-unknown", if locked > 0 { "{locked}" } else { "·" } }
            } else {
                for i in 0..total {
                    span { class: if i < locked { "seg on" } else { "seg" } }
                }
            }
        }
    }
}

/// SM / VM / NiM toggles for the matrix; the last one on stays on
#[component]
pub fn TierPicks(tiers: Signal<[bool; 3]>) -> Element {
    let on = *tiers.read();
    rsx! {
        div { class: "matrix-tier-picks", title: "Modes shown in the lockout matrix",
            for (i, t) in TIERS.iter().enumerate() {
                button {
                    class: format!("matrix-tier-pick tier-{}{}", t.to_lowercase(), if on[i] { " on" } else { "" }),
                    onclick: move |_| {
                        let mut v = *tiers.read();
                        v[i] = !v[i];
                        if v.iter().any(|b| *b) {
                            tiers.set(v);
                        }
                    },
                    {tier_short(t)}
                }
            }
        }
    }
}

#[component]
pub fn LockoutMatrix(
    rows: Vec<CharacterRow>,
    operations: Vec<OperationRef>,
    /// Which of Story / Veteran / Master to draw
    tiers: [bool; 3],
    on_edit: EventHandler<CharacterRow>,
) -> Element {
    let picked: Vec<&'static str> = TIERS.iter().zip(tiers).filter(|(_, on)| *on).map(|(t, _)| *t).collect();
    // Columns: operations with a known boss list first, then the rest
    let mut sorted: Vec<&OperationRef> = operations.iter().collect();
    sorted.sort_by_key(|o| (o.tiers.is_empty(), o.name.clone()));
    // Modes drawn per column: the picked ones the operation has (every picked
    // one with a lockout when the area is unknown). A known operation with
    // none of the picked modes drops out.
    let (ops, (keys, shown)): (Vec<&OperationRef>, (Vec<String>, Vec<Vec<(&'static str, usize)>>)) = sorted
        .into_iter()
        .filter_map(|o| {
            let k = operation_key(o.area_id, &o.name);
            let modes: Vec<(&'static str, usize)> = if o.tiers.is_empty() {
                picked.iter().copied().filter(|t| rows.iter().any(|r| lock_at(r, &k, t).is_some())).map(|t| (t, 0)).collect()
            } else {
                picked.iter().copied().filter_map(|t| o.tiers.iter().find(|tb| tb.tier == t).map(|tb| (t, tb.bosses.len()))).collect()
            };
            (o.tiers.is_empty() || !modes.is_empty()).then_some((o, (k, modes)))
        })
        .unzip();
    // How many characters each column locks out at a shown mode, for the header
    let locked_counts: Vec<usize> = keys
        .iter()
        .zip(shown.iter())
        .map(|(k, modes)| rows.iter().filter(|r| modes.iter().any(|(t, _)| lock_at(r, k, t).is_some())).count())
        .collect();

    rsx! {
        div { class: "pull-history-table-wrap lockout-matrix-wrap",
            table { class: "pull-history-table lockout-matrix",
                thead {
                    tr {
                        th { class: "matrix-char", "Character" }
                        for ((o, n), modes) in ops.iter().zip(locked_counts.iter()).zip(shown.iter()) {
                            th { class: "matrix-op",
                                title: if o.tiers.is_empty() {
                                    format!("{}: no boss list for this area", o.name)
                                } else {
                                    format!("{} · {} · {} locked", o.name, modes.iter().map(|(t, b)| format!("{} {b} bosses", tier_short(t))).collect::<Vec<_>>().join(", "), n)
                                },
                                span { class: "matrix-op-name", "{o.name}" }
                                if *n > 0 { span { class: "matrix-op-count", "{n}" } }
                            }
                        }
                    }
                }
                tbody {
                    if rows.is_empty() {
                        tr { td { colspan: "{ops.len() + 1}", class: "library-empty", "No characters match." } }
                    }
                    for row in rows.iter() {
                        {
                            let r = row.clone();
                            rsx! {
                                tr { key: "{row.key}",
                                    td { class: "matrix-char",
                                        button { class: "matrix-char-btn", title: "Edit {row.name}", onclick: move |_| on_edit.call(r.clone()),
                                            FactionIcon { faction: row.faction }
                                            span { class: "matrix-char-text",
                                                span { class: "matrix-char-name", "{row.name}" }
                                                if !row.roles.is_empty() {
                                                    span { class: "matrix-char-roles",
                                                        for r in row.roles.iter() {
                                                            if let Some(icon) = get_role_icon(&r.icon) { img { src: *icon, alt: "{r.name}", title: "{r.name}" } }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    for ((o, key), modes) in ops.iter().zip(keys.iter()).zip(shown.iter()) {
                                        td { class: "matrix-cell",
                                            if modes.is_empty() {
                                                span { class: "matrix-unknown", title: "No lockouts recorded here", "—" }
                                            } else {
                                                div { class: "lock-bars",
                                                    for (t, bosses) in modes.iter() {
                                                        LockBar {
                                                            operation: o.name.clone(),
                                                            tier: *t,
                                                            bosses: *bosses,
                                                            lock: lock_at(row, key, t).cloned(),
                                                            label: true,
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
    }
}
