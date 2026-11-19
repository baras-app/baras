//! Template for the text a metric overlay shows beside each player's name.
//!
//! `{name}` placeholders are replaced with values; anything else is literal.
//! A `|` splits the template into a centre and a right column, so
//! `{total} | {rate}` lines totals and rates up down the overlay. Without a
//! `|` the whole template is right-aligned.

/// A value a template can show. `Rate`, `Total` and `Share` are the overlay's
/// own metric; the rest are the player's stats whatever the overlay shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetricVar {
    Rate,
    Total,
    /// The player's share of the group's rate
    Share,
    Dps,
    Edps,
    BossDps,
    TotalDamage,
    TotalDamageEffective,
    TotalDamageBoss,
    DamageCritPct,
    Hps,
    Ehps,
    TotalHealing,
    TotalHealingEffective,
    HealCritPct,
    EffectiveHealPct,
    Tps,
    TotalThreat,
    Dtps,
    Edtps,
    TotalDamageTaken,
    TotalDamageTakenEffective,
    Abs,
    TotalShielding,
    Htps,
    Ehtps,
    TotalHealingReceived,
    TotalHealingReceivedEffective,
    DefensePct,
    ShieldPct,
    TotalShieldAbsorbed,
    Apm,
    Interrupts,
}

/// A placeholder's name, value and description
pub type Placeholder = (&'static str, MetricVar, &'static str);

impl MetricVar {
    /// Every placeholder, grouped as settings lists them
    pub const GROUPS: &[(&str, &[Placeholder])] = &[
        ("This overlay", &[
            ("rate", Self::Rate, "The overlay's own per-second value"),
            ("total", Self::Total, "The overlay's own cumulative total"),
            ("pct", Self::Share, "Share of the group's rate"),
        ]),
        ("Damage", &[
            ("dps", Self::Dps, "Damage per second"),
            ("edps", Self::Edps, "Effective damage per second"),
            ("bossdps", Self::BossDps, "Damage per second on bosses"),
            ("damage_crit_pct", Self::DamageCritPct, "Damage critical chance"),
            ("total_damage", Self::TotalDamage, "Total damage"),
            ("total_damage_effective", Self::TotalDamageEffective, "Total effective damage"),
            ("total_damage_boss", Self::TotalDamageBoss, "Total damage on bosses"),
        ]),
        ("Healing", &[
            ("hps", Self::Hps, "Healing per second"),
            ("ehps", Self::Ehps, "Effective healing per second"),
            ("ehps_pct", Self::EffectiveHealPct, "Share of healing that was effective"),
            ("heal_crit_pct", Self::HealCritPct, "Healing critical chance"),
            ("abs", Self::Abs, "Shielding (absorbs) per second"),
            ("total_healing", Self::TotalHealing, "Total healing"),
            ("total_healing_effective", Self::TotalHealingEffective, "Total effective healing"),
            ("total_shielding", Self::TotalShielding, "Total shielding (absorbs)"),
        ]),
        ("Damage taken", &[
            ("dtps", Self::Dtps, "Damage taken per second"),
            ("edtps", Self::Edtps, "Effective damage taken per second"),
            ("defense_pct", Self::DefensePct, "Attacks dodged, parried or deflected"),
            ("shield_pct", Self::ShieldPct, "Hits shielded"),
            ("total_damage_taken", Self::TotalDamageTaken, "Total damage taken"),
            ("total_damage_taken_effective", Self::TotalDamageTakenEffective, "Total effective damage taken"),
            ("total_shield_absorbed", Self::TotalShieldAbsorbed, "Total damage absorbed by shield"),
        ]),
        ("Healing received", &[
            ("htps", Self::Htps, "Healing received per second"),
            ("ehtps", Self::Ehtps, "Effective healing received per second"),
            ("total_healing_received", Self::TotalHealingReceived, "Total healing received"),
            ("total_healing_received_effective", Self::TotalHealingReceivedEffective, "Total effective healing received"),
        ]),
        ("Other", &[
            ("tps", Self::Tps, "Threat per second"),
            ("total_threat", Self::TotalThreat, "Total threat"),
            ("apm", Self::Apm, "Actions per minute"),
            ("interrupts", Self::Interrupts, "Interrupts cast"),
        ]),
    ];

    fn all() -> impl Iterator<Item = &'static Placeholder> {
        Self::GROUPS.iter().flat_map(|(_, vars)| vars.iter())
    }

    fn from_name(name: &str) -> Option<Self> {
        Self::all().find(|(n, _, _)| *n == name).map(|(_, v, _)| *v)
    }

    /// Cumulative values, hidden where totals are suppressed (warzones)
    pub fn is_total(self) -> bool {
        Self::all().any(|(n, v, _)| *v == self && n.starts_with("total"))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Segment {
    Text(String),
    Var(MetricVar),
}

/// Ready-made templates offered in settings: (template, label)
pub const PRESETS: &[(&str, &str)] = &[
    ("{rate}", "Rate"),
    ("{total}", "Total"),
    ("{total} | {rate}", "Total and Rate"),
    ("{rate} ({pct})", "Rate (share)"),
    ("{total} | {rate} ({pct})", "Total and Rate (share)"),
    ("", "Name only"),
];

/// A parsed template; an empty column draws nothing
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MetricTemplate {
    pub center: Vec<Segment>,
    pub right: Vec<Segment>,
}

impl MetricTemplate {
    pub fn parse(template: &str) -> Self {
        match template.split_once('|') {
            Some((center, right)) => Self {
                center: parse_column(center),
                right: parse_column(right),
            },
            None => Self {
                center: Vec::new(),
                right: parse_column(template),
            },
        }
    }

    /// Drop every column that shows a cumulative total
    pub fn without_totals(mut self) -> Self {
        let has_total =
            |col: &[Segment]| col.iter().any(|s| matches!(s, Segment::Var(v) if v.is_total()));
        if has_total(&self.center) {
            self.center.clear();
        }
        if has_total(&self.right) {
            self.right.clear();
        }
        self
    }
}

/// Unknown or unclosed placeholders stay as literal text so a typo is visible
fn parse_column(column: &str) -> Vec<Segment> {
    let mut segments = Vec::new();
    let mut text = String::new();
    let mut rest = column.trim();
    while let Some(open) = rest.find('{') {
        let var = rest[open + 1..]
            .find('}')
            .and_then(|len| Some((len, MetricVar::from_name(&rest[open + 1..open + 1 + len])?)));
        match var {
            Some((len, var)) => {
                text.push_str(&rest[..open]);
                if !text.is_empty() {
                    segments.push(Segment::Text(std::mem::take(&mut text)));
                }
                segments.push(Segment::Var(var));
                rest = &rest[open + len + 2..];
            }
            None => {
                text.push_str(&rest[..=open]);
                rest = &rest[open + 1..];
            }
        }
    }
    text.push_str(rest);
    if !text.is_empty() {
        segments.push(Segment::Text(text));
    }
    segments
}

/// Fill a column in; `value` formats one placeholder
pub fn render(column: &[Segment], value: impl Fn(MetricVar) -> String) -> String {
    column
        .iter()
        .map(|s| match s {
            Segment::Text(t) => t.clone(),
            Segment::Var(v) => value(*v),
        })
        .collect()
}
