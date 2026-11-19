//! Ability grouping and sorting for the detailed breakdown table.

use crate::api::{AbilityBreakdown, BreakdownMode};
use crate::components::mirror_names::display_name;
use crate::types::{SortColumn, SortDirection};

// Group stats for hierarchical display
#[derive(Clone, Default, PartialEq)]
pub(super) struct GroupStats {
    pub target: Option<String>,
    pub first_hit: Option<f32>,
    pub total: f64,
    pub percent: f64,
    pub rate: f64,
    pub hits: i64,
    pub avg: f64,
    pub crit_pct: f64,
    pub miss_count: i64,
    pub activation_count: i64,
    pub crit_total: f64,
    pub effective_total: f64,
    pub shield_total: f64,
    pub shield_rate: f64,
    // DamageTaken-specific
    pub shield_count: i64,
    pub absorbed_total: f64,
    // Single-value metrics
    pub max_hit: f64,
}

/// Target groups in display order, each with its sorted abilities
pub(super) type AbilityGroups = Vec<(GroupStats, Vec<AbilityBreakdown>)>;

/// Groups abilities by target when a breakdown mode is enabled, else one flat group
pub(super) fn group_abilities(
    abilities: &[AbilityBreakdown],
    col: SortColumn,
    dir: SortDirection,
    mode: BreakdownMode,
) -> AbilityGroups {
    let list: Vec<AbilityBreakdown> = abilities
        .iter()
        .cloned()
        .map(|mut a| {
            a.ability_name = display_name(a.ability_id, &a.ability_name).into_owned();
            a
        })
        .collect();

    // Sort function for abilities within groups
    let sort_abilities = |mut items: Vec<AbilityBreakdown>| -> Vec<AbilityBreakdown> {
        items.sort_by(|a, b| {
            let cmp = match col {
                SortColumn::Target | SortColumn::Ability => a.ability_name.cmp(&b.ability_name),
                SortColumn::Total => a
                    .total_value
                    .partial_cmp(&b.total_value)
                    .unwrap_or(std::cmp::Ordering::Equal),
                SortColumn::Percent => a
                    .percent_of_total
                    .partial_cmp(&b.percent_of_total)
                    .unwrap_or(std::cmp::Ordering::Equal),
                SortColumn::Rate => a.dps.partial_cmp(&b.dps).unwrap_or(std::cmp::Ordering::Equal),
                SortColumn::Hits => a.hit_count.cmp(&b.hit_count),
                SortColumn::Avg => a.avg_hit.partial_cmp(&b.avg_hit).unwrap_or(std::cmp::Ordering::Equal),
                SortColumn::CritPct => a
                    .crit_rate
                    .partial_cmp(&b.crit_rate)
                    .unwrap_or(std::cmp::Ordering::Equal),
                SortColumn::MissPct => {
                    let a_total = a.hit_count + a.miss_count;
                    let b_total = b.hit_count + b.miss_count;
                    let a_pct = if a_total > 0 {
                        a.miss_count as f64 / a_total as f64
                    } else {
                        0.0
                    };
                    let b_pct = if b_total > 0 {
                        b.miss_count as f64 / b_total as f64
                    } else {
                        0.0
                    };
                    a_pct.partial_cmp(&b_pct).unwrap_or(std::cmp::Ordering::Equal)
                }
                SortColumn::AvgHit => a.avg_hit.partial_cmp(&b.avg_hit).unwrap_or(std::cmp::Ordering::Equal),
                SortColumn::AvgCrit => {
                    let a_avg = if a.crit_count > 0 {
                        a.crit_total / a.crit_count as f64
                    } else {
                        0.0
                    };
                    let b_avg = if b.crit_count > 0 {
                        b.crit_total / b.crit_count as f64
                    } else {
                        0.0
                    };
                    a_avg.partial_cmp(&b_avg).unwrap_or(std::cmp::Ordering::Equal)
                }
                SortColumn::Activations => a.activation_count.cmp(&b.activation_count),
                SortColumn::Effective => a
                    .effective_total
                    .partial_cmp(&b.effective_total)
                    .unwrap_or(std::cmp::Ordering::Equal),
                SortColumn::EffectivePct => {
                    let a_pct = if a.total_value > 0.0 {
                        a.effective_total / a.total_value
                    } else {
                        0.0
                    };
                    let b_pct = if b.total_value > 0.0 {
                        b.effective_total / b.total_value
                    } else {
                        0.0
                    };
                    a_pct.partial_cmp(&b_pct).unwrap_or(std::cmp::Ordering::Equal)
                }
                SortColumn::ShieldTotal => {
                    let a_val = if a.is_shield { a.total_value } else { 0.0 };
                    let b_val = if b.is_shield { b.total_value } else { 0.0 };
                    a_val.partial_cmp(&b_val).unwrap_or(std::cmp::Ordering::Equal)
                }
                SortColumn::Sps => {
                    let a_val = if a.is_shield { a.dps } else { 0.0 };
                    let b_val = if b.is_shield { b.dps } else { 0.0 };
                    a_val.partial_cmp(&b_val).unwrap_or(std::cmp::Ordering::Equal)
                }
                SortColumn::AttackType => a.attack_type.cmp(&b.attack_type),
                SortColumn::DamageType => a.damage_type.cmp(&b.damage_type),
                SortColumn::ShldPct => {
                    let a_pct = if a.hit_count > 0 {
                        a.shield_count as f64 / a.hit_count as f64
                    } else {
                        0.0
                    };
                    let b_pct = if b.hit_count > 0 {
                        b.shield_count as f64 / b.hit_count as f64
                    } else {
                        0.0
                    };
                    a_pct.partial_cmp(&b_pct).unwrap_or(std::cmp::Ordering::Equal)
                }
                SortColumn::Absorbed => a
                    .absorbed_total
                    .partial_cmp(&b.absorbed_total)
                    .unwrap_or(std::cmp::Ordering::Equal),
                SortColumn::MaxHit => a.max_hit.partial_cmp(&b.max_hit).unwrap_or(std::cmp::Ordering::Equal),
                SortColumn::AvgPerActivation => {
                    let val = |x: &AbilityBreakdown| {
                        if x.activation_count > 0 {
                            x.total_value / x.activation_count as f64
                        } else {
                            0.0
                        }
                    };
                    val(a).partial_cmp(&val(b)).unwrap_or(std::cmp::Ordering::Equal)
                }
                SortColumn::AvgPerActivationEff => {
                    let val = |x: &AbilityBreakdown| {
                        if x.activation_count > 0 {
                            x.effective_total / x.activation_count as f64
                        } else {
                            0.0
                        }
                    };
                    val(a).partial_cmp(&val(b)).unwrap_or(std::cmp::Ordering::Equal)
                }
            };
            match dir {
                SortDirection::Asc => cmp,
                SortDirection::Desc => cmp.reverse(),
            }
        });
        items
    };

    // If not grouping by target, return flat list with empty stats
    if !mode.by_target_type && !mode.by_target_instance {
        return vec![(GroupStats::default(), sort_abilities(list))];
    }

    // Group by target (using target_name + target_log_id for instance mode)
    use std::collections::BTreeMap;
    let mut groups: BTreeMap<(String, Option<i64>), Vec<AbilityBreakdown>> = BTreeMap::new();

    for ability in list {
        let target = ability.target_name.clone().unwrap_or_default();
        // Use target_log_id for instance grouping (unique per NPC spawn)
        let instance_key = if mode.by_target_instance {
            ability.target_log_id
        } else {
            None
        };
        groups.entry((target, instance_key)).or_default().push(ability);
    }

    // Convert to vec with aggregate group stats
    let mut result: Vec<(GroupStats, Vec<AbilityBreakdown>)> = groups
        .into_iter()
        .map(|((target, _instance_key), abilities)| {
            let total: f64 = abilities.iter().map(|a| a.total_value).sum();
            let percent: f64 = abilities.iter().map(|a| a.percent_of_total).sum();
            let rate: f64 = abilities.iter().map(|a| a.dps).sum();
            let hits: i64 = abilities.iter().map(|a| a.hit_count).sum();
            let crits: i64 = abilities.iter().map(|a| a.crit_count).sum();
            let first_hit = abilities.first().and_then(|a| a.target_first_hit_secs);
            let avg = if hits > 0 { total / hits as f64 } else { 0.0 };
            let crit_pct = if hits > 0 {
                crits as f64 / hits as f64 * 100.0
            } else {
                0.0
            };
            let miss_count: i64 = abilities.iter().map(|a| a.miss_count).sum();
            // Activations are per-ability (not per-target), so sum unique abilities' counts
            let activation_count: i64 = abilities.iter().map(|a| a.activation_count).sum();
            let crit_total: f64 = abilities.iter().map(|a| a.crit_total).sum();
            let effective_total: f64 = abilities.iter().map(|a| a.effective_total).sum();
            let shield_total: f64 = abilities.iter().filter(|a| a.is_shield).map(|a| a.total_value).sum();
            let shield_rate: f64 = abilities.iter().filter(|a| a.is_shield).map(|a| a.dps).sum();
            let shield_count: i64 = abilities.iter().map(|a| a.shield_count).sum();
            let absorbed_total: f64 = abilities.iter().map(|a| a.absorbed_total).sum();
            let max_hit: f64 = abilities.iter().map(|a| a.max_hit).fold(0.0_f64, f64::max);

            let stats = GroupStats {
                target: Some(target),
                first_hit,
                total,
                percent,
                rate,
                hits,
                avg,
                crit_pct,
                miss_count,
                activation_count,
                crit_total,
                effective_total,
                shield_total,
                shield_rate,
                shield_count,
                absorbed_total,
                max_hit,
            };
            (stats, sort_abilities(abilities))
        })
        .collect();

    // Sort groups by total (descending by default)
    result.sort_by(|a, b| {
        let cmp = match col {
            SortColumn::Target => a.0.target.cmp(&b.0.target),
            _ => a.0.total.partial_cmp(&b.0.total).unwrap_or(std::cmp::Ordering::Equal),
        };
        match col {
            SortColumn::Target => {
                if dir == SortDirection::Asc {
                    cmp
                } else {
                    cmp.reverse()
                }
            }
            _ => cmp.reverse(),
        }
    });

    result
}
