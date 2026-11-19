//! Clears summary: per operation and group size, the full clears, fastest
//! clear and timed runs achieved at each tier. Folded from the run list the
//! Operations tab already holds.

use dioxus::prelude::*;

use baras_types::history::RunSummary;

use super::operations::{RunKey, key_of, tier_class};
use super::run_detail::fmt_secs;

const TIERS: [&str; 3] = ["Story", "Veteran", "Master"];

#[derive(Clone, PartialEq)]
struct Best {
    secs: i64,
    /// A timed-run tier clocked from the first pull: no start region was logged
    approx: bool,
    key: RunKey,
    character: String,
    date: String,
}

#[derive(Clone, PartialEq, Default)]
struct TierClears {
    clears: u32,
    best: Option<Best>,
    timed: u32,
    /// The timed-run achievement exists at this tier
    has_timer: bool,
}

/// One operation at one group size, tiers in `TIERS` order
#[derive(Clone, PartialEq)]
pub(super) struct OpClears {
    operation: String,
    size: u8,
    is_flashpoint: bool,
    tiers: [TierClears; 3],
}

/// Operations first, then flashpoints, each by name. Runs in an unrecognised
/// tier are left out.
pub(super) fn summarize(runs: &[&RunSummary]) -> Vec<OpClears> {
    let mut out: Vec<OpClears> = Vec::new();
    for r in runs {
        let Some(t) = TIERS.iter().position(|t| *t == r.tier) else {
            continue;
        };
        let idx = out
            .iter()
            .position(|o| o.operation == r.operation && o.size == r.group_size)
            .unwrap_or_else(|| {
                out.push(OpClears {
                    operation: r.operation.clone(),
                    size: r.group_size,
                    is_flashpoint: r.is_flashpoint,
                    tiers: Default::default(),
                });
                out.len() - 1
            });
        let tier = &mut out[idx].tiers[t];
        tier.has_timer |= r.timer.limit_secs.is_some();
        if !r.cleared {
            continue;
        }
        tier.clears += 1;
        tier.timed += u32::from(r.timer.achieved == Some(true));
        // A clear clocked from the start region beats any clocked from the first pull
        let approx = r.timer.from_first_pull && r.timer.limit_secs.is_some();
        if tier
            .best
            .as_ref()
            .is_none_or(|b| (approx, r.timer.elapsed_secs) < (b.approx, b.secs))
        {
            tier.best = Some(Best {
                secs: r.timer.elapsed_secs,
                approx,
                key: key_of(r),
                character: r.character.clone(),
                date: r.started.get(..10).unwrap_or(&r.started).to_string(),
            });
        }
    }
    out.sort_by(|a, b| (a.is_flashpoint, &a.operation).cmp(&(b.is_flashpoint, &b.operation)));
    out
}

/// A row opens the operation's run table; a best time opens that run
#[component]
pub(super) fn ClearsTable(
    rows: Vec<OpClears>,
    on_area: EventHandler<String>,
    on_pick: EventHandler<RunKey>,
) -> Element {
    let mut size = use_signal(|| 8u8);
    let mut sizes: Vec<u8> = rows.iter().map(|o| o.size).collect();
    sizes.sort_unstable();
    sizes.dedup();
    let picked = *size.read();
    let shown = if sizes.contains(&picked) {
        picked
    } else {
        sizes.first().copied().unwrap_or(8)
    };
    rsx! {
        div { class: "pull-history-header",
            div { class: "pull-history-title",
                h3 { "Clears" }
                span { class: "pull-history-operation", "Full clears and fastest time · Click a row for its runs" }
            }
            if sizes.len() > 1 {
                div { class: "filter-pills", title: "Group size",
                    for s in sizes {
                        button {
                            class: if s == shown { "seg active size" } else { "seg size" },
                            onclick: move |_| size.set(s),
                            if s == 0 { "Other" } else { "{s}-man" }
                        }
                    }
                }
            }
        }
        div { class: "pull-history-table-wrap clears-wrap",
            table { class: "pull-history-table clears-table",
                thead {
                    tr {
                        th { rowspan: "2", class: "clears-op", "Operation" }
                        for t in TIERS {
                            th { colspan: "2", class: "clears-group", span { class: "{tier_class(t)}", "{t}" } }
                        }
                        th { rowspan: "2", class: "clears-timed", title: "Timed-run achievements earned", "Timed" }
                    }
                    tr {
                        for _ in TIERS {
                            th { class: "clears-count", "Clears" }
                            th { class: "clears-time", "Best" }
                        }
                    }
                }
                tbody {
                    for o in rows.iter().filter(|o| o.size == shown) {
                        {
                            let op = o.operation.clone();
                            let timed: u32 = o.tiers.iter().map(|t| t.timed).sum();
                            rsx! {
                                tr { key: "{o.operation}", class: "run-row", onclick: move |_| on_area.call(op.clone()),
                                    td { class: "clears-op", "{o.operation}" }
                                    for t in o.tiers.iter() {
                                        td { class: "clears-count",
                                            if t.clears > 0 { "{t.clears}" } else { span { class: "clears-none", "—" } }
                                        }
                                        td { class: "clears-time",
                                            if let Some(b) = &t.best {
                                                {
                                                    let key = b.key.clone();
                                                    let note = if b.approx { " · clocked from the first pull" } else { "" };
                                                    rsx! {
                                                        span {
                                                            class: "clears-best",
                                                            title: "{b.character} · {b.date}{note}",
                                                            onclick: move |e| {
                                                                e.stop_propagation();
                                                                on_pick.call(key.clone());
                                                            },
                                                            if b.approx { "~" }
                                                            {fmt_secs(b.secs)}
                                                        }
                                                    }
                                                }
                                            } else {
                                                span { class: "clears-none", "—" }
                                            }
                                        }
                                    }
                                    td { class: "clears-timed",
                                        if !o.tiers.iter().any(|t| t.has_timer) {
                                            span { class: "clears-none", "—" }
                                        } else if timed > 0 {
                                            span { class: "run-badge timer-ok", "{timed}" }
                                        } else {
                                            "0"
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
