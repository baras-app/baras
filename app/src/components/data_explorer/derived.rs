//! Pure derivations behind the Data Explorer's memos.

use super::ctx::OverviewTableData;
use crate::api::{PvpFaction, RaidOverviewRow};
use crate::components::encounter_types::{EncounterSummary, group_by_area};
use crate::components::overview_table::{OverviewSort, OverviewTotals};

/// (area name, difficulty, encounters newest-first), newest area first
pub(super) type AreaSections = Vec<(String, Option<String>, Vec<EncounterSummary>)>;

/// Player/companion rows sorted by the selected column, with their totals
pub(super) fn overview_table(data: &[RaidOverviewRow], sort_col: OverviewSort, sort_asc: bool) -> OverviewTableData {
    let mut rows: Vec<RaidOverviewRow> = data
        .iter()
        .filter(|r| r.entity_type == "Player" || r.entity_type == "Companion")
        .cloned()
        .collect();

    // Sort rows by selected column. PvP rows (faction set) group
    // friendlies first, then enemies, sorting by column within each side.
    rows.sort_by(|a, b| {
        let rank = |r: &RaidOverviewRow| match r.faction {
            Some(PvpFaction::Friendly) => 0u8,
            Some(PvpFaction::Enemy) => 1,
            None => 2,
        };
        rank(a).cmp(&rank(b)).then_with(|| {
            let cmp = sort_col
                .extract(a)
                .partial_cmp(&sort_col.extract(b))
                .unwrap_or(std::cmp::Ordering::Equal);
            if sort_asc { cmp } else { cmp.reverse() }
        })
    });

    // Calculate totals; PvP encounters get per-side totals instead of a
    // combined group total (friend+enemy sums are meaningless)
    let totals = OverviewTotals::from_rows(rows.iter());
    let pvp_totals = if rows.iter().any(|r| r.faction.is_some()) {
        Some((
            OverviewTotals::from_rows(rows.iter().filter(|r| r.faction == Some(PvpFaction::Friendly))),
            OverviewTotals::from_rows(rows.iter().filter(|r| r.faction == Some(PvpFaction::Enemy))),
        ))
    } else {
        None
    };

    OverviewTableData {
        rows,
        totals,
        pvp_totals,
    }
}

// When filtering for boss-only, propagate is_phase_start to the next visible
// encounter so phase boundaries aren't lost when trash encounters are hidden
// PvP matches count as bosses (there's no "trash" in a warzone/arena)
pub(super) fn filter_history(history: Vec<EncounterSummary>, bosses_only: bool) -> Vec<EncounterSummary> {
    if bosses_only {
        let mut pending_phase_start = false;
        history
            .into_iter()
            .filter_map(|mut e| {
                if e.is_phase_start {
                    pending_phase_start = true;
                }
                if e.boss_name.is_some() || e.encounter_type == "PvP" {
                    if pending_phase_start {
                        e.is_phase_start = true;
                        pending_phase_start = false;
                    }
                    Some(e)
                } else {
                    None
                }
            })
            .collect()
    } else {
        history
    }
}

pub(super) fn area_sections(filtered: &[EncounterSummary]) -> AreaSections {
    group_by_area(filtered)
        .into_iter()
        .map(|(area, diff, encs)| {
            let mut reversed: Vec<_> = encs.into_iter().cloned().collect();
            reversed.reverse();
            (area, diff, reversed)
        })
        .rev()
        .collect::<Vec<_>>()
}
