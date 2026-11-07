//! Character roster served to the Characters tab.
//!
//! A character is a (name, server) pair. What the logs know (classes played,
//! kills, last session) is derived on every query; what they cannot know
//! (crew skills, guild, note, overrides) lives in `CharacterRecord`, stored
//! in `~/.config/baras/characters.json`.

use serde::{Deserialize, Serialize};

/// Base class display names, Imperial first, in mirror order
pub const CLASSES: &[&str] = &[
    "Sorcerer", "Assassin", "Juggernaut", "Marauder", "Mercenary", "Powertech", "Operative", "Sniper",
    "Sage", "Shadow", "Guardian", "Sentinel", "Commando", "Vanguard", "Scoundrel", "Gunslinger",
];

/// Crafting, gathering and mission skills; a character holds up to three
pub const CREW_SKILLS: &[&str] = &[
    "Armormech", "Armstech", "Artifice", "Biochem", "Cybertech", "Synthweaving",
    "Archaeology", "Bioanalysis", "Scavenging", "Slicing",
    "Diplomacy", "Investigation", "Treasure Hunting", "Underworld Trading",
];

pub const MAX_CLASSES: usize = 2;
pub const MAX_CREW_SKILLS: usize = 3;
pub const CHARACTER_NOTE_MAX: usize = 500;

/// Weekly reset weekday and default hour (Tuesday 12:00 UTC, 6 AM Central
/// standard time); the hour is user-adjustable in the store
pub const DEFAULT_RESET_HOUR_UTC: u8 = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Faction {
    #[default]
    Unknown,
    Republic,
    Empire,
}

impl Faction {
    pub fn label(self) -> &'static str {
        match self {
            Faction::Unknown => "—",
            Faction::Republic => "Republic",
            Faction::Empire => "Empire",
        }
    }
}

/// `name@server`; server is the display name ("Star Forge"), empty when unknown
pub fn character_key(name: &str, server: &str) -> String {
    format!("{name}@{server}")
}

/// An operation the user marked as locked by hand (a kill made off-log).
/// Treated as a kill of the final boss at `marked_ms`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManualLock {
    pub operation: String,
    #[serde(default)]
    pub area_id: i64,
    /// "Story" | "Veteran" | "Master"
    pub tier: String,
    /// True UTC epoch milliseconds
    pub marked_ms: i64,
}

/// The user-owned half of a character.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct CharacterRecord {
    pub name: String,
    #[serde(default)]
    pub server: String,
    /// Assigned combat styles (display names); empty = whatever the logs show
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub classes: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub crew_skills: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub guild: String,
    /// Set by the user: combat styles cross factions, so logs cannot tell
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub faction: Option<Faction>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub manual_locks: Vec<ManualLock>,
}

/// A class or role with its icon filename (resolved by the frontend registry)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IconLabel {
    pub name: String,
    pub icon: String,
}

/// Locked bosses of one operation at one mode. 8 and 16 player share a
/// lockout, so group size plays no part.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Lockout {
    pub operation: String,
    pub area_id: i64,
    /// "Story" | "Veteran" | "Master" | ""
    pub tier: String,
    /// Locked bosses in operation order (a kill locks every boss before it)
    pub locked: Vec<String>,
    /// Bosses in the operation at this difficulty; 0 when the area is unknown
    pub total: u8,
    /// True UTC epoch milliseconds of the reset that clears it
    pub expires_ms: i64,
    pub manual: bool,
}

impl Lockout {
    pub fn is_full(&self) -> bool {
        self.total > 0 && self.locked.len() >= self.total as usize
    }
}

/// One character as the table shows it: record plus everything derived.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct CharacterRow {
    pub key: String,
    pub name: String,
    pub server: String,
    pub faction: Faction,
    pub classes: Vec<IconLabel>,
    /// Classes came from the logs rather than the record
    pub classes_auto: bool,
    /// Union of what the classes can fill: Tank, Healer, Dps order
    pub roles: Vec<IconLabel>,
    pub crew_skills: Vec<String>,
    pub guild: String,
    pub note: String,
    /// ISO 8601 local time of the newest log event; empty when never logged
    pub last_seen: String,
    pub lockouts: Vec<Lockout>,
    /// Has a record in the store (not just discovered from logs)
    pub saved: bool,
    pub hidden: bool,
}

/// An operation the logs have seen, for the badges and the manual lockout
/// picker. One entry per area; the name is the one most of its pulls used
/// (clients log localized area names).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OperationRef {
    pub name: String,
    pub area_id: i64,
}

/// What identifies an operation across localized names: the area id when
/// known, else the name
pub fn operation_key(area_id: i64, name: &str) -> String {
    if area_id != 0 { area_id.to_string() } else { name.to_string() }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct CharacterRoster {
    pub rows: Vec<CharacterRow>,
    /// Servers seen on any row, sorted
    pub servers: Vec<String>,
    /// Operations seen in any log, sorted by name
    pub operations: Vec<OperationRef>,
    pub reset_hour_utc: u8,
    pub next_weekly_reset_ms: i64,
    pub next_daily_reset_ms: i64,
}
