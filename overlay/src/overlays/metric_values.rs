//! Values behind the placeholders of a metric overlay's template.

use baras_core::PlayerMetrics;
use baras_types::formatting;
use baras_types::metric_template::MetricVar;

use super::MetricEntry;

/// Shown where a placeholder has no value (a row that isn't a player, or a
/// footer placeholder that can't be summed)
const MISSING: &str = "-";

enum Stat {
    Amount(i64),
    Percent(f32),
    Decimal(f32),
}

fn player_stat(var: MetricVar, m: &PlayerMetrics) -> Option<Stat> {
    use MetricVar::*;
    use Stat::*;
    Some(match var {
        Rate | Total | Share => return None,
        Dps => Amount(m.dps),
        Edps => Amount(m.edps),
        BossDps => Amount(m.bossdps),
        TotalDamage => Amount(m.total_damage),
        TotalDamageEffective => Amount(m.total_damage_effective),
        TotalDamageBoss => Amount(m.total_damage_boss),
        DamageCritPct => Percent(m.damage_crit_pct),
        Hps => Amount(m.hps),
        Ehps => Amount(m.ehps),
        TotalHealing => Amount(m.total_healing),
        TotalHealingEffective => Amount(m.total_healing_effective),
        HealCritPct => Percent(m.heal_crit_pct),
        EffectiveHealPct => Percent(m.effective_heal_pct),
        Tps => Amount(m.tps),
        TotalThreat => Amount(m.total_threat),
        Dtps => Amount(m.dtps),
        Edtps => Amount(m.edtps),
        TotalDamageTaken => Amount(m.total_damage_taken),
        TotalDamageTakenEffective => Amount(m.total_damage_taken_effective),
        Abs => Amount(m.abs),
        TotalShielding => Amount(m.total_shielding),
        Htps => Amount(m.htps),
        Ehtps => Amount(m.ehtps),
        TotalHealingReceived => Amount(m.total_healing_received),
        TotalHealingReceivedEffective => Amount(m.total_healing_received_effective),
        DefensePct => Percent(m.defense_pct),
        ShieldPct => Percent(m.shield_pct),
        TotalShieldAbsorbed => Amount(m.total_shield_absorbed),
        Apm => Decimal(m.apm),
        Interrupts => Amount(m.interrupt_casts as i64),
    })
}

/// One row's value for a placeholder; `rate_sum` is the group's rate
pub(super) fn entry_value(var: MetricVar, entry: &MetricEntry, rate_sum: i64, eu: bool) -> String {
    match var {
        MetricVar::Rate => entry
            .display_value
            .clone()
            .unwrap_or_else(|| formatting::format_compact(entry.value, eu)),
        MetricVar::Total => entry
            .display_total
            .clone()
            .unwrap_or_else(|| formatting::format_compact(entry.total_value, eu)),
        MetricVar::Share => formatting::format_pct_ratio(entry.value, rate_sum, eu),
        _ => match entry.player.as_deref().and_then(|m| player_stat(var, m)) {
            Some(Stat::Amount(n)) => formatting::format_compact(n, eu),
            Some(Stat::Percent(p)) => formatting::format_pct_f32(p, eu),
            Some(Stat::Decimal(d)) => formatting::format_f32_1(d, eu),
            None => MISSING.to_string(),
        },
    }
}

/// The footer's value for a placeholder: the sum over `entries` where the
/// value is an amount; rates of change and percentages don't add up
pub(super) fn footer_value<'a>(var: MetricVar, entries: impl Iterator<Item = &'a MetricEntry>, eu: bool) -> String {
    let amount = |e: &MetricEntry| match var {
        MetricVar::Rate => Some(e.value),
        MetricVar::Total => Some(e.total_value),
        _ => match player_stat(var, e.player.as_deref()?)? {
            Stat::Amount(n) => Some(n),
            _ => None,
        },
    };
    match var {
        MetricVar::Share => formatting::format_pct(100.0, eu),
        _ => entries
            .map(amount)
            .sum::<Option<i64>>()
            .map_or_else(|| MISSING.to_string(), |sum| formatting::format_compact(sum, eu)),
    }
}
