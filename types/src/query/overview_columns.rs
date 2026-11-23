//! Columns of the Data Explorer's overview table and the user's saved layouts.

use serde::{Deserialize, Serialize};

/// A number column of the overview table (also what it sorts by)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverviewColumn {
    DamageTotal,
    #[default]
    Dps,
    DamageCritPct,
    BossDamage,
    BossDps,
    ThreatTotal,
    Tps,
    DamageTakenTotal,
    Dtps,
    Edtps,
    AbsorbedTotal,
    Aps,
    DefensePct,
    ShieldPct,
    HealingTakenTotal,
    Htps,
    Ehtps,
    HealingTotal,
    HealingEffective,
    Hps,
    HealingPct,
    Ehps,
    HealingCritPct,
    ShieldingTotal,
    Sps,
    Apm,
    Interrupts,
    Cleanses,
}

impl OverviewColumn {
    /// Shown, in this order, until the user picks their own
    pub const DEFAULT: [Self; 15] = [
        Self::DamageTotal,
        Self::Dps,
        Self::ThreatTotal,
        Self::Tps,
        Self::DamageTakenTotal,
        Self::Dtps,
        Self::Aps,
        Self::HealingTotal,
        Self::Hps,
        Self::HealingPct,
        Self::Ehps,
        Self::ShieldingTotal,
        Self::Sps,
        Self::Apm,
        Self::Interrupts,
    ];
}

/// The user's column picks per encounter kind; empty = the default layout
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct OverviewLayouts {
    #[serde(default)]
    pub pve: Vec<OverviewColumn>,
    #[serde(default)]
    pub pvp: Vec<OverviewColumn>,
}

impl OverviewLayouts {
    pub fn columns(&self, pvp: bool) -> Vec<OverviewColumn> {
        let saved = if pvp { &self.pvp } else { &self.pve };
        if saved.is_empty() {
            OverviewColumn::DEFAULT.to_vec()
        } else {
            saved.clone()
        }
    }

    pub fn set(&mut self, pvp: bool, columns: Vec<OverviewColumn>) {
        *(if pvp { &mut self.pvp } else { &mut self.pve }) = columns;
    }
}
