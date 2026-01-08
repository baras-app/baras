//! SWTOR Character Disciplines and Role Mapping
//!
//! Maps discipline GUIDs from combat logs to character roles (Tank, Healer, DPS).
//! Data sourced from StarParse.

use serde::{Deserialize, Serialize};

/// Character role in group content
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Role {
    Tank,
    Healer,
    Dps,
}

impl Role {
    /// Get the icon filename for this role (without path)
    pub const fn icon_name(&self) -> &'static str {
        match self {
            Role::Tank => "icon_tank.png",
            Role::Healer => "icon_heal.png",
            Role::Dps => "icon_dps.png",
        }
    }
}

/// SWTOR base classes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Class {
    // Imperial
    Sorcerer,
    Assassin,
    Juggernaut,
    Marauder,
    Mercenary,
    Powertech,
    Operative,
    Sniper,
    // Republic
    Sage,
    Shadow,
    Guardian,
    Sentinel,
    Commando,
    Vanguard,
    Scoundrel,
    Gunslinger,
}

impl Class {
    /// Get the icon filename for this class (without path)
    pub const fn icon_name(&self) -> &'static str {
        match self {
            Class::Sorcerer => "sorcerer.png",
            Class::Assassin => "assassin.png",
            Class::Juggernaut => "juggernaut.png",
            Class::Marauder => "marauder.png",
            Class::Mercenary => "mercenary.png",
            Class::Powertech => "powertech.png",
            Class::Operative => "operative.png",
            Class::Sniper => "sniper.png",
            Class::Sage => "sage.png",
            Class::Shadow => "shadow.png",
            Class::Guardian => "guardian.png",
            Class::Sentinel => "sentinel.png",
            Class::Commando => "commando.png",
            Class::Vanguard => "vanguard.png",
            Class::Scoundrel => "scoundrel.png",
            Class::Gunslinger => "gunslinger.png",
        }
    }
}

/// Character discipline with associated role and class
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Discipline {
    // Sorcerer
    Lightning,
    Madness,
    Corruption,
    // Assassin
    Hatred,
    Darkness,
    Deception,
    // Juggernaut
    Vengeance,
    Immortal,
    Rage,
    // Marauder
    Annihilation,
    Carnage,
    Fury,
    // Mercenary
    Arsenal,
    InnovativeOrdnance,
    Bodyguard,
    // Powertech
    ShieldTech,
    Pyrotech,
    AdvancedPrototype,
    // Operative
    Concealment,
    Lethality,
    Medicine,
    // Sniper
    Marksmanship,
    Engineering,
    Virulence,
    // Sage
    Telekinetics,
    Seer,
    Balance,
    // Shadow
    Infiltration,
    KineticCombat,
    Serenity,
    // Guardian
    Focus,
    Vigilance,
    Defense,
    // Sentinel
    Combat,
    Watchman,
    Concentration,
    // Commando
    Gunnery,
    AssaultSpecialist,
    CombatMedic,
    // Vanguard
    Plasmatech,
    ShieldSpecialist,
    Tactics,
    // Scoundrel
    Scrapper,
    Ruffian,
    Sawbones,
    // Gunslinger
    Sharpshooter,
    Saboteur,
    DirtyFighting,
}

impl Discipline {
    /// Get the role for this discipline
    pub const fn role(&self) -> Role {
        use Discipline::*;
        match self {
            // Tanks
            Immortal | Darkness | ShieldTech | Defense | KineticCombat | ShieldSpecialist => {
                Role::Tank
            }
            // Healers
            Corruption | Bodyguard | Medicine | Seer | CombatMedic | Sawbones => Role::Healer,
            // DPS (everything else)
            _ => Role::Dps,
        }
    }

    /// Get the base class for this discipline
    pub const fn class(&self) -> Class {
        use Discipline::*;
        match self {
            Lightning | Madness | Corruption => Class::Sorcerer,
            Hatred | Darkness | Deception => Class::Assassin,
            Vengeance | Immortal | Rage => Class::Juggernaut,
            Annihilation | Carnage | Fury => Class::Marauder,
            Arsenal | InnovativeOrdnance | Bodyguard => Class::Mercenary,
            ShieldTech | Pyrotech | AdvancedPrototype => Class::Powertech,
            Concealment | Lethality | Medicine => Class::Operative,
            Marksmanship | Engineering | Virulence => Class::Sniper,
            Telekinetics | Seer | Balance => Class::Sage,
            Infiltration | KineticCombat | Serenity => Class::Shadow,
            Focus | Vigilance | Defense => Class::Guardian,
            Combat | Watchman | Concentration => Class::Sentinel,
            Gunnery | AssaultSpecialist | CombatMedic => Class::Commando,
            Plasmatech | ShieldSpecialist | Tactics => Class::Vanguard,
            Scrapper | Ruffian | Sawbones => Class::Scoundrel,
            Sharpshooter | Saboteur | DirtyFighting => Class::Gunslinger,
        }
    }

    /// Get the display name for this discipline
    pub const fn name(&self) -> &'static str {
        use Discipline::*;
        match self {
            Lightning => "Lightning",
            Madness => "Madness",
            Corruption => "Corruption",
            Hatred => "Hatred",
            Darkness => "Darkness",
            Deception => "Deception",
            Vengeance => "Vengeance",
            Immortal => "Immortal",
            Rage => "Rage",
            Annihilation => "Annihilation",
            Carnage => "Carnage",
            Fury => "Fury",
            Arsenal => "Arsenal",
            InnovativeOrdnance => "Innovative Ordnance",
            Bodyguard => "Bodyguard",
            ShieldTech => "Shield Tech",
            Pyrotech => "Pyrotech",
            AdvancedPrototype => "Advanced Prototype",
            Concealment => "Concealment",
            Lethality => "Lethality",
            Medicine => "Medicine",
            Marksmanship => "Marksmanship",
            Engineering => "Engineering",
            Virulence => "Virulence",
            Telekinetics => "Telekinetics",
            Seer => "Seer",
            Balance => "Balance",
            Infiltration => "Infiltration",
            KineticCombat => "Kinetic Combat",
            Serenity => "Serenity",
            Focus => "Focus",
            Vigilance => "Vigilance",
            Defense => "Defense",
            Combat => "Combat",
            Watchman => "Watchman",
            Concentration => "Concentration",
            Gunnery => "Gunnery",
            AssaultSpecialist => "Assault Specialist",
            CombatMedic => "Combat Medic",
            Plasmatech => "Plasmatech",
            ShieldSpecialist => "Shield Specialist",
            Tactics => "Tactics",
            Scrapper => "Scrapper",
            Ruffian => "Ruffian",
            Sawbones => "Sawbones",
            Sharpshooter => "Sharpshooter",
            Saboteur => "Saboteur",
            DirtyFighting => "Dirty Fighting",
        }
    }

    /// Look up discipline from its GUID (from combat log)
    pub fn from_guid(guid: i64) -> Option<Self> {
        use Discipline::*;
        match guid {
            // Sorcerer
            2031339142381586 => Some(Lightning),
            2031339142381584 => Some(Madness),
            2031339142381587 => Some(Corruption),
            // Assassin
            2031339142381580 => Some(Hatred),
            2031339142381582 => Some(Darkness),
            2031339142381583 => Some(Deception),
            // Juggernaut
            2031339142381576 => Some(Vengeance),
            2031339142381577 => Some(Immortal),
            2031339142381578 => Some(Rage),
            // Marauder
            2031339142381572 => Some(Annihilation),
            2031339142381573 => Some(Carnage),
            2031339142381574 => Some(Fury),
            // Mercenary
            2031339142381601 => Some(Arsenal),
            2031339142381598 => Some(InnovativeOrdnance),
            2031339142381600 => Some(Bodyguard),
            // Powertech
            2031339142381604 => Some(ShieldTech),
            2031339142381602 => Some(Pyrotech),
            2031339142381605 => Some(AdvancedPrototype),
            // Operative
            2031339142381595 => Some(Concealment),
            2031339142381593 => Some(Lethality),
            2031339142381596 => Some(Medicine),
            // Sniper
            2031339142381591 => Some(Marksmanship),
            2031339142381592 => Some(Engineering),
            2031339142381589 => Some(Virulence),
            // Sage
            2031339142381618 => Some(Telekinetics),
            2031339142381619 => Some(Seer),
            2031339142381616 => Some(Balance),
            // Shadow
            2031339142381620 => Some(Infiltration),
            2031339142381622 => Some(KineticCombat),
            2031339142381623 => Some(Serenity),
            // Guardian
            2031339142381607 => Some(Focus),
            2031339142381610 => Some(Vigilance),
            2031339142381609 => Some(Defense),
            // Sentinel
            2031339142381613 => Some(Combat),
            2031339142381614 => Some(Watchman),
            2031339142381611 => Some(Concentration),
            // Commando
            2031339142381636 => Some(Gunnery),
            2031339142381634 => Some(AssaultSpecialist),
            2031339142381637 => Some(CombatMedic),
            // Vanguard
            2031339142381638 => Some(Plasmatech),
            2031339142381641 => Some(ShieldSpecialist),
            2031339142381640 => Some(Tactics),
            // Scoundrel
            2031339142381632 => Some(Scrapper),
            2031339142381629 => Some(Ruffian),
            2031339142381631 => Some(Sawbones),
            // Gunslinger
            2031339142381627 => Some(Sharpshooter),
            2031339142381628 => Some(Saboteur),
            2031339142381625 => Some(DirtyFighting),
            _ => None,
        }
    }
}
