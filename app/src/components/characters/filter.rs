//! Lockout filter above the roster: a tile per operation with a count per
//! mode, a Locked / Free switch, and a boss strip per picked (operation,
//! mode) to narrow to one boss.

use std::collections::HashMap;

use dioxus::prelude::*;

use baras_types::characters::{CharacterRow, OperationRef, operation_key};

use super::lockouts::tier_short;

const TIERS: [&str; 3] = ["Story", "Veteran", "Master"];

/// Bosses the character is locked out of at this operation and mode. A kill
/// locks every boss before it, so this is also the index of the next boss.
fn progress(row: &CharacterRow, op_key: &str, tier: &str) -> usize {
    row.lockouts
        .iter()
        .find(|l| operation_key(l.area_id, &l.operation) == op_key && l.tier == tier)
        .map_or(0, |l| l.locked.len())
}

/// Does a character at `progress` match one pick?
/// Locked: holds a lockout (whose next boss is `boss`, when narrowed).
/// Free: holds none (or can still kill `boss`, when narrowed).
fn hit(free: bool, progress: usize, boss: Option<usize>) -> bool {
    match (free, boss) {
        (false, None) => progress > 0,
        (false, Some(i)) => progress > 0 && progress == i,
        (true, None) => progress == 0,
        (true, Some(i)) => progress <= i,
    }
}

/// Picked (operation key, mode) pairs, each optionally narrowed to a boss
/// index. Locked keeps characters matching any pick, Free those matching
/// every pick. No picks keeps everyone.
#[derive(Clone, PartialEq, Default)]
pub struct LockFilter {
    free: bool,
    picks: HashMap<(String, String), Option<usize>>,
}

impl LockFilter {
    pub fn keeps(&self, row: &CharacterRow) -> bool {
        let mut hits = self
            .picks
            .iter()
            .map(|((k, t), boss)| hit(self.free, progress(row, k, t), *boss));
        self.picks.is_empty() || if self.free { hits.all(|h| h) } else { hits.any(|h| h) }
    }

    fn toggle(&mut self, key: &str, tier: &str) {
        let pick = (key.to_string(), tier.to_string());
        if self.picks.remove(&pick).is_none() {
            self.picks.insert(pick, None);
        }
    }

    /// Boss indices mean different things per mode, so narrowing resets
    fn set_free(&mut self, free: bool) {
        if self.free != free {
            self.free = free;
            self.picks.values_mut().for_each(|b| *b = None);
        }
    }
}

/// The modes an operation has (every mode when the area has no definition)
fn modes_of(op: &OperationRef) -> impl Iterator<Item = &'static str> + '_ {
    TIERS
        .into_iter()
        .filter(|t| op.tiers.is_empty() || op.tiers.iter().any(|tb| tb.tier == *t))
}

/// Not a component: borrows the roster rather than cloning it into props.
/// Counts are over `rows`, the characters the toolbar filters keep.
pub fn lock_filter_block(
    operations: &[OperationRef],
    rows: &[&CharacterRow],
    mut filter: Signal<LockFilter>,
) -> Element {
    if operations.is_empty() {
        return rsx! {};
    }
    let f = filter.read().clone();
    let free = f.free;
    let count = |free: bool, key: &str, tier: &str, boss: Option<usize>| {
        rows.iter().filter(|r| hit(free, progress(r, key, tier), boss)).count()
    };
    // Picked (operation, mode) pairs that have a boss list, with their narrowing
    let strips = operations
        .iter()
        .flat_map(|op| op.tiers.iter().map(move |tb| (op, tb)))
        .filter_map(|(op, tb)| {
            let pick = (operation_key(op.area_id, &op.name), tb.tier.clone());
            let picked = *f.picks.get(&pick)?;
            Some((op, tb, pick, picked))
        });
    let noun = if free { "free" } else { "locked out" };

    rsx! {
        div { class: "op-filter",
            div { class: "op-filter-head",
                i { class: "fa-solid fa-lock" }
                span { class: "op-filter-label", "Lockouts" }
                div { class: "op-seg",
                    button { class: if !free { "on" }, title: "Count and filter characters holding a lockout", onclick: move |_| filter.write().set_free(false), "Locked" }
                    button { class: if free { "on" }, title: "Count and filter characters with no lockout", onclick: move |_| filter.write().set_free(true), "Free" }
                }
                span { class: "hint",
                    if free { "Click a count to show only the characters free for all picked" } else { "Click a count to show only the characters locked out there" }
                }
                if !f.picks.is_empty() {
                    button { class: "btn btn-xs btn-ghost", title: "Show every character again", onclick: move |_| filter.write().picks.clear(),
                        i { class: "fa-solid fa-xmark" } " Clear"
                    }
                }
            }
            div { class: "op-filter-grid",
                for op in operations.iter() {
                    {
                        let key = operation_key(op.area_id, &op.name);
                        let active = f.picks.keys().any(|(k, _)| *k == key);
                        let any_locked = modes_of(op).any(|t| count(false, &key, t, None) > 0);
                        rsx! {
                            div {
                                class: if active { "op-filter-badge active" } else if any_locked { "op-filter-badge locked" } else { "op-filter-badge free" },
                                title: "{op.name}",
                                span { class: "op-filter-name", "{op.name}" }
                                span { class: "op-filter-modes",
                                    for t in modes_of(op) {
                                        {
                                            let n = count(free, &key, t, None);
                                            let lit = f.picks.contains_key(&(key.clone(), t.to_string()));
                                            let key = key.clone();
                                            rsx! {
                                                button {
                                                    class: format!("op-mode tier-{}{}{}", t.to_lowercase(), if n == 0 { " none" } else { "" }, if lit { " lit" } else { "" }),
                                                    title: "{tier_short(t)}: {n} {noun} · click to show only them",
                                                    disabled: n == 0 && !lit,
                                                    onclick: move |_| filter.write().toggle(&key, t),
                                                    "{n}"
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
            // One boss strip per pick whose operation has a boss list
            for (op, tb, pick, picked) in strips {
                    {
                        // Locked adds a trailing "full clear" (no next boss)
                        let chips = tb.bosses.iter().map(String::as_str).chain((!free).then_some("Full clear"));
                        let any = pick.clone();
                        rsx! {
                            div { class: "op-boss-strip tier-{tb.tier.to_lowercase()}",
                                span { class: "op-boss-strip-label", "{op.name} · {tier_short(&tb.tier)}" }
                                span { class: "hint", if free { "can still kill" } else { "next boss is" } }
                                button {
                                    class: if picked.is_none() { "op-boss lit" } else { "op-boss" },
                                    onclick: move |_| { filter.write().picks.insert(any.clone(), None); },
                                    "Any"
                                }
                                for (i, boss) in chips.enumerate() {
                                    {
                                        let n = count(free, &pick.0, &pick.1, Some(i));
                                        let lit = picked == Some(i);
                                        let pick = pick.clone();
                                        rsx! {
                                            button {
                                                class: if lit { "op-boss lit" } else { "op-boss" },
                                                disabled: n == 0 && !lit,
                                                onclick: move |_| {
                                                    filter.write().picks.insert(pick.clone(), (!lit).then_some(i));
                                                },
                                                "{boss} "
                                                span { class: "op-boss-count", "{n}" }
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
