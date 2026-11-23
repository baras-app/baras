//! The overview table's columns: what each one shows, and the picker that
//! chooses which are displayed and in what order (one layout for PvE, one for PvP).

use dioxus::prelude::*;

use baras_types::{OverviewColumn, UiSessionState, formatting};

use crate::api::{self, RaidOverviewRow};
use crate::components::{ToastSeverity, use_toast};

#[derive(Clone, Copy, PartialEq)]
pub enum Group {
    Damage,
    Threat,
    Taken,
    HealingTaken,
    Healing,
    Shielding,
    Activity,
}

impl Group {
    /// Cell colour class
    pub fn class(self) -> &'static str {
        match self {
            Self::Damage => "dmg",
            Self::Threat => "threat",
            Self::Taken => "taken",
            Self::HealingTaken | Self::Healing => "heal",
            Self::Shielding => "shield",
            Self::Activity => "apm",
        }
    }
}

#[derive(Clone, Copy)]
pub enum Format {
    Compact,
    Pct,
    Decimal(usize),
}

impl Format {
    pub fn apply(self, n: f64, european: bool) -> String {
        match self {
            Self::Compact => formatting::format_compact_f64(n, european),
            Self::Pct => formatting::format_pct(n, european),
            Self::Decimal(precision) => formatting::format_decimal_f64(n, precision, european),
        }
    }
}

pub struct ColumnDef {
    pub id: OverviewColumn,
    pub group: Group,
    pub label: &'static str,
    pub title: &'static str,
    pub value: fn(&RaidOverviewRow) -> f64,
    pub format: Format,
    /// The footer shows the column's sum (rates and totals; not percentages)
    pub sums: bool,
}

impl ColumnDef {
    /// Per-second rates carry the comparison and keep their full colour;
    /// totals, percentages and counts sit back as supporting figures
    pub fn is_rate(&self) -> bool {
        matches!(
            self.id,
            C::Dps
                | C::BossDps
                | C::Tps
                | C::Dtps
                | C::Edtps
                | C::Aps
                | C::Htps
                | C::Ehtps
                | C::Hps
                | C::Ehps
                | C::Sps
                | C::Apm
        )
    }

    /// Cell classes: group colour, toned down for supporting columns and zeros
    pub fn cell_class(&self, value: f64) -> String {
        format!(
            "{}{}{}",
            self.group.class(),
            if self.is_rate() { "" } else { " sub" },
            if value == 0.0 { " zero" } else { "" }
        )
    }
}

const fn col(
    id: OverviewColumn,
    group: Group,
    label: &'static str,
    title: &'static str,
    value: fn(&RaidOverviewRow) -> f64,
    format: Format,
    sums: bool,
) -> ColumnDef {
    ColumnDef {
        id,
        group,
        label,
        title,
        value,
        format,
        sums,
    }
}

use Format::{Compact, Decimal, Pct};
use OverviewColumn as C;

/// Every column, in the order the picker lists the hidden ones
pub static COLUMNS: [ColumnDef; 28] = [
    col(
        C::DamageTotal,
        Group::Damage,
        "Damage",
        "Damage dealt",
        |r| r.damage_total,
        Compact,
        true,
    ),
    col(
        C::Dps,
        Group::Damage,
        "DPS",
        "Damage per second",
        |r| r.dps,
        Compact,
        true,
    ),
    col(
        C::DamageCritPct,
        Group::Damage,
        "Dmg Crit",
        "Share of damaging hits that crit",
        |r| r.damage_crit_pct,
        Pct,
        false,
    ),
    col(
        C::BossDamage,
        Group::Damage,
        "Boss Dmg",
        "Damage dealt to bosses",
        |r| r.boss_damage_total,
        Compact,
        true,
    ),
    col(
        C::BossDps,
        Group::Damage,
        "Boss DPS",
        "Damage per second dealt to bosses",
        |r| r.boss_dps,
        Compact,
        true,
    ),
    col(
        C::ThreatTotal,
        Group::Threat,
        "Threat",
        "Threat generated",
        |r| r.threat_total,
        Compact,
        true,
    ),
    col(
        C::Tps,
        Group::Threat,
        "TPS",
        "Threat per second",
        |r| r.tps,
        Compact,
        true,
    ),
    col(
        C::DamageTakenTotal,
        Group::Taken,
        "Dmg Taken",
        "Damage taken",
        |r| r.damage_taken_total,
        Compact,
        true,
    ),
    col(
        C::Dtps,
        Group::Taken,
        "DTPS",
        "Damage taken per second",
        |r| r.dtps,
        Compact,
        true,
    ),
    col(
        C::Edtps,
        Group::Taken,
        "EDTPS",
        "Damage taken per second after mitigation",
        |r| r.edtps,
        Compact,
        true,
    ),
    col(
        C::AbsorbedTotal,
        Group::Taken,
        "Absorbed",
        "Incoming damage absorbed",
        |r| r.absorbed_total,
        Compact,
        true,
    ),
    col(
        C::Aps,
        Group::Taken,
        "APS",
        "Incoming damage absorbed per second",
        |r| r.aps,
        Compact,
        true,
    ),
    col(
        C::DefensePct,
        Group::Taken,
        "Def %",
        "Share of incoming attacks missed, dodged, parried, deflected or resisted",
        |r| r.defense_pct,
        Pct,
        false,
    ),
    col(
        C::ShieldPct,
        Group::Taken,
        "Shield %",
        "Share of landed hits that were shielded",
        |r| r.shield_pct,
        Pct,
        false,
    ),
    col(
        C::HealingTakenTotal,
        Group::HealingTaken,
        "Heal Taken",
        "Healing received, overheal included",
        |r| r.healing_taken_total,
        Compact,
        true,
    ),
    col(
        C::Htps,
        Group::HealingTaken,
        "HTPS",
        "Healing received per second",
        |r| r.htps,
        Compact,
        true,
    ),
    col(
        C::Ehtps,
        Group::HealingTaken,
        "EHTPS",
        "Effective healing received per second",
        |r| r.ehtps,
        Compact,
        true,
    ),
    col(
        C::HealingTotal,
        Group::Healing,
        "Healing",
        "Healing done, shielding and overheal included",
        |r| r.healing_total,
        Compact,
        true,
    ),
    col(
        C::HealingEffective,
        Group::Healing,
        "Eff Heal",
        "Effective healing done, shielding included",
        |r| r.healing_effective,
        Compact,
        true,
    ),
    col(
        C::Hps,
        Group::Healing,
        "HPS",
        "Healing per second",
        |r| r.hps,
        Compact,
        true,
    ),
    col(
        C::HealingPct,
        Group::Healing,
        "Eff %",
        "Share of healing that was effective",
        |r| r.healing_pct,
        Pct,
        false,
    ),
    col(
        C::Ehps,
        Group::Healing,
        "EHPS",
        "Effective healing per second",
        |r| r.ehps,
        Compact,
        true,
    ),
    col(
        C::HealingCritPct,
        Group::Healing,
        "Heal Crit",
        "Share of heals that crit",
        |r| r.healing_crit_pct,
        Pct,
        false,
    ),
    col(
        C::ShieldingTotal,
        Group::Shielding,
        "Shielding",
        "Shielding given",
        |r| r.shielding_given_total,
        Compact,
        true,
    ),
    col(
        C::Sps,
        Group::Shielding,
        "SPS",
        "Shielding per second",
        |r| r.sps,
        Compact,
        true,
    ),
    col(
        C::Apm,
        Group::Activity,
        "APM",
        "Actions per minute",
        |r| r.apm,
        Decimal(1),
        false,
    ),
    col(
        C::Interrupts,
        Group::Activity,
        "Int",
        "Interrupt casts",
        |r| r.interrupts,
        Decimal(0),
        true,
    ),
    col(
        C::Cleanses,
        Group::Activity,
        "Cln",
        "Cleanse casts",
        |r| r.cleanses,
        Decimal(0),
        true,
    ),
];

pub fn column(id: OverviewColumn) -> &'static ColumnDef {
    COLUMNS.iter().find(|c| c.id == id).unwrap_or(&COLUMNS[0])
}

/// Checklist behind the table's columns button: shown columns first, in
/// table order, each movable up or down; the hidden ones below
#[component]
pub fn OverviewColumnMenu(
    mut state: Signal<UiSessionState>,
    shown: Vec<OverviewColumn>,
    pvp: bool,
    mut open: Signal<bool>,
) -> Element {
    let mut toast = use_toast();
    let save = use_callback(move |columns: Vec<OverviewColumn>| {
        state.write().data_explorer.overview_columns.set(pvp, columns.clone());
        spawn(async move {
            if let Err(err) = api::patch_config(move |cfg| cfg.overview_columns.set(pvp, columns)).await {
                toast.show(format!("Failed to save settings: {}", err), ToastSeverity::Normal);
            }
        });
    });

    rsx! {
           if open() {
                div { class: "nav-menu-backdrop", onclick: move |_| open.set(false) }
                   div { class: "nav-menu-list overview-columns-list",
                       div { class: "multi-select-group", if pvp { "Columns — PvP" } else { "Columns — PvE" } }
                       for (i, id) in shown.iter().copied().enumerate() {
                           {
                               let def = column(id);
                               let (hide, up, down) = (shown.clone(), shown.clone(), shown.clone());
                               let last = shown.len() - 1;
                               rsx! {
                                   div { key: "{id:?}", class: "overview-columns-row",
                                       button {
                                           class: "nav-menu-item active",
                                           title: "{def.title}",
                                           // The last column stays: an empty table shows nothing
                                           disabled: last == 0,
                                           onclick: move |_| {
                                               let mut next = hide.clone();
                                               next.remove(i);
                                               save.call(next);
                                           },
                                           i { class: "fa-solid fa-square-check" }
                                           " {def.label}"
    span { class: "overview-columns-desc", "{def.title}" }
                                       }
                                       button {
                                           class: "overview-columns-move",
                                           title: "Move earlier",
                                           disabled: i == 0,
                                           onclick: move |_| {
                                               let mut next = up.clone();
                                               next.swap(i, i - 1);
                                               save.call(next);
                                           },
                                           i { class: "fa-solid fa-chevron-up" }
                                       }
                                       button {
                                           class: "overview-columns-move",
                                           title: "Move later",
                                           disabled: i == last,
                                           onclick: move |_| {
                                               let mut next = down.clone();
                                               next.swap(i, i + 1);
                                               save.call(next);
                                           },
                                           i { class: "fa-solid fa-chevron-down" }
                                       }
                                   }
                               }
                           }
                       }
                       div { class: "nav-menu-divider" }
                       for def in COLUMNS.iter().filter(|c| !shown.contains(&c.id)) {
                           {
                               let add = shown.clone();
                               rsx! {
                                   button {
                                       key: "{def.id:?}",
                                       class: "nav-menu-item",
                                       title: "{def.title}",
                                       onclick: move |_| {
                                           let mut next = add.clone();
                                           next.push(def.id);
                                           save.call(next);
                                       },
                                       i { class: "fa-regular fa-square" }
                                       " {def.label}"
    span { class: "overview-columns-desc", "{def.title}" }
                                   }
                               }
                           }
                       }
                       div { class: "nav-menu-divider" }
                       button {
                           class: "nav-menu-item",
                           onclick: move |_| save.call(Vec::new()),
                           i { class: "fa-solid fa-rotate-left" }
                           " Reset to default"
                       }
                   }
               }

       }
}
