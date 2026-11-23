//! Raid debuff matrix: the standard debuffs a group keeps on enemies, per target

use serde::{Deserialize, Serialize};

/// Damage type ids as logged
const KINETIC: i64 = 836045448940873;
const ENERGY: i64 = 836045448940874;
const ELEMENTAL: i64 = 836045448940875;
const INTERNAL: i64 = 836045448940876;

/// A hit's attack type (as in `attack_types.csv`) and damage type id → whether
/// a debuff raises it
type RaisesHit = fn(&str, i64) -> bool;

/// The standardized enemy debuffs; several abilities apply each one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RaidDebuff {
    Sundered,
    Marked,
    BeatDown,
    Susceptible,
    Vulnerable,
    Assailable,
    Overwhelmed,
    Trauma,
    Impaired,
    Weakened,
    Unsteady,
}

impl RaidDebuff {
    /// Display order: damage taken, healing, then the tank debuffs
    pub const ALL: [Self; 11] = [
        Self::Sundered,
        Self::Marked,
        Self::BeatDown,
        Self::Susceptible,
        Self::Vulnerable,
        Self::Assailable,
        Self::Overwhelmed,
        Self::Trauma,
        Self::Impaired,
        Self::Weakened,
        Self::Unsteady,
    ];

    /// Name as logged, without the "(Physical)" style suffix
    pub fn name(self) -> &'static str {
        match self {
            Self::BeatDown => "Beat Down",
            Self::Sundered => "Sundered",
            Self::Marked => "Marked",
            Self::Susceptible => "Susceptible",
            Self::Vulnerable => "Vulnerable",
            Self::Assailable => "Assailable",
            Self::Overwhelmed => "Overwhelmed",
            Self::Trauma => "Trauma",
            Self::Impaired => "Impaired",
            Self::Weakened => "Weakened",
            Self::Unsteady => "Unsteady",
        }
    }

    /// One effect ID that applies it; every effect of a debuff shares its icon
    pub fn icon_effect_id(self) -> i64 {
        match self {
            Self::Sundered => 3394699201085734,
            Self::Marked => 3465720780292447,
            Self::BeatDown => 3478012976693516,
            Self::Susceptible => 814884145070400,
            Self::Vulnerable => 3347334301745440,
            Self::Assailable => 3403448049467654,
            Self::Overwhelmed => 993073748247163,
            Self::Trauma => 987812413309363,
            Self::Impaired => 3178507727274302,
            Self::Weakened => 991390121066781,
            Self::Unsteady => 4056656740614482,
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Sundered => "-20% armor rating",
            Self::Marked => "+5% ranged damage taken",
            Self::BeatDown => "+5% melee damage taken",
            Self::Susceptible => "+5% tech damage taken",
            Self::Vulnerable => "+5% force damage taken",
            Self::Assailable => "+7% internal/elemental damage taken",
            Self::Overwhelmed => "+10% area damage taken",
            Self::Trauma => "-20% healing received",
            Self::Impaired => "-5% force and tech damage dealt",
            Self::Weakened => "-5% melee and ranged damage dealt",
            Self::Unsteady => "-5% melee and ranged accuracy",
        }
    }

    /// The damage it raises on its target, where the log tells that kind of
    /// hit apart: its label and the test on a hit. None for the rest (area
    /// damage, healing and the offense debuffs).
    fn boost(self) -> Option<(&'static str, RaisesHit)> {
        let boost: (&'static str, RaisesHit) = match self {
            Self::Sundered => ("kinetic and energy", |_, d| matches!(d, KINETIC | ENERGY)),
            Self::Marked => ("ranged", |a, _| a == "Ranged"),
            Self::BeatDown => ("melee", |a, _| a == "Melee"),
            Self::Susceptible => ("tech", |a, _| a == "Tech"),
            Self::Vulnerable => ("force", |a, _| a == "Force"),
            Self::Assailable => ("internal and elemental", |_, d| matches!(d, INTERNAL | ELEMENTAL)),
            _ => return None,
        };
        Some(boost)
    }

    /// The kind of damage it raises, as a label; None where the log cannot tell
    pub fn boosts(self) -> Option<&'static str> {
        self.boost().map(|b| b.0)
    }

    /// Whether it raises a hit of this attack type and damage type id
    pub fn raises(self, attack_type: &str, dmg_type: i64) -> bool {
        self.boost().is_some_and(|b| (b.1)(attack_type, dmg_type))
    }

    pub fn group(self) -> &'static str {
        match self {
            Self::Trauma => "Healing",
            Self::Impaired | Self::Weakened | Self::Unsteady => "Offense",
            _ => "Damage Taken",
        }
    }
}

/// One player's applications of a debuff through one ability.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RaidDebuffSource {
    pub name: String,
    pub ability_id: i64,
    pub ability_name: String,
    pub count: u32,
}

/// A debuff that was on a target at some point.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RaidDebuffCell {
    pub debuff: RaidDebuff,
    /// Share of the target's time in the fight, 0-100
    pub uptime_pct: f32,
    /// When it was up: merged (start, end) combat seconds
    pub windows: Vec<(f32, f32)>,
    /// Most applications first
    pub sources: Vec<RaidDebuffSource>,
}

/// An enemy and the raid debuffs it carried. Same-named enemies share a row:
/// a debuff counts as up while it is on at least one of them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RaidDebuffTarget {
    pub name: String,
    pub is_boss: bool,
    /// Time from first seen to last seen (or death), within the queried range
    pub present_secs: f32,
    /// Only the debuffs that were applied
    pub cells: Vec<RaidDebuffCell>,
    /// Only the debuffs with a `boosts` kind the group dealt to this enemy
    pub worth: Vec<RaidDebuffWorth>,
}

/// The group's damage on one enemy of the kind a debuff raises.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RaidDebuffWorth {
    pub debuff: RaidDebuff,
    /// All of it, within the queried range
    pub affected_dmg: f64,
    /// The part that landed while the debuff was not on the enemy
    pub unbuffed_dmg: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RaidDebuffReport {
    pub targets: Vec<RaidDebuffTarget>,
    /// Per debuff, the discipline GUIDs that apply it: allowed by the game
    /// data and seen doing so in real logs
    pub able: Vec<(RaidDebuff, Vec<i64>)>,
}
