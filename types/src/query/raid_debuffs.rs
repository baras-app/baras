//! Raid debuff matrix: the standard debuffs a group keeps on enemies, per target

use serde::{Deserialize, Serialize};

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
}
