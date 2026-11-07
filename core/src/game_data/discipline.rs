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

    pub const fn to_u8(self) -> u8 {
        match self {
            Role::Tank => 1,
            Role::Healer => 2,
            Role::Dps => 3,
        }
    }

    pub const fn from_u8(v: u8) -> Option<Self> {
        match v {
            1 => Some(Role::Tank),
            2 => Some(Role::Healer),
            3 => Some(Role::Dps),
            _ => None,
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

    pub const fn name(&self) -> &'static str {
        match self {
            Class::Sorcerer => "Sorcerer",
            Class::Assassin => "Assassin",
            Class::Juggernaut => "Juggernaut",
            Class::Marauder => "Marauder",
            Class::Mercenary => "Mercenary",
            Class::Powertech => "Powertech",
            Class::Operative => "Operative",
            Class::Sniper => "Sniper",
            Class::Sage => "Sage",
            Class::Shadow => "Shadow",
            Class::Guardian => "Guardian",
            Class::Sentinel => "Sentinel",
            Class::Commando => "Commando",
            Class::Vanguard => "Vanguard",
            Class::Scoundrel => "Scoundrel",
            Class::Gunslinger => "Gunslinger",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Discipline::all().iter().map(|d| d.class()).find(|c| c.name().eq_ignore_ascii_case(name))
    }

    /// Every role one of this class's disciplines fills, Tank → Healer → DPS
    pub fn roles(&self) -> impl Iterator<Item = Role> + '_ {
        [Role::Tank, Role::Healer, Role::Dps]
            .into_iter()
            .filter(move |r| Discipline::all().iter().any(|d| d.class() == *self && d.role() == *r))
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

    /// Look up discipline from its display name
    pub fn from_name(name: &str) -> Option<Self> {
        use Discipline::*;
        match name {
            "Lightning" => Some(Lightning),
            "Madness" => Some(Madness),
            "Corruption" => Some(Corruption),
            "Hatred" => Some(Hatred),
            "Darkness" => Some(Darkness),
            "Deception" => Some(Deception),
            "Vengeance" => Some(Vengeance),
            "Immortal" => Some(Immortal),
            "Rage" => Some(Rage),
            "Annihilation" => Some(Annihilation),
            "Carnage" => Some(Carnage),
            "Fury" => Some(Fury),
            "Arsenal" => Some(Arsenal),
            "Innovative Ordnance" => Some(InnovativeOrdnance),
            "Bodyguard" => Some(Bodyguard),
            "Shield Tech" => Some(ShieldTech),
            "Pyrotech" => Some(Pyrotech),
            "Advanced Prototype" => Some(AdvancedPrototype),
            "Concealment" => Some(Concealment),
            "Lethality" => Some(Lethality),
            "Medicine" => Some(Medicine),
            "Marksmanship" => Some(Marksmanship),
            "Engineering" => Some(Engineering),
            "Virulence" => Some(Virulence),
            "Telekinetics" => Some(Telekinetics),
            "Seer" => Some(Seer),
            "Balance" => Some(Balance),
            "Infiltration" => Some(Infiltration),
            "Kinetic Combat" => Some(KineticCombat),
            "Serenity" => Some(Serenity),
            "Focus" => Some(Focus),
            "Vigilance" => Some(Vigilance),
            "Defense" => Some(Defense),
            "Combat" => Some(Combat),
            "Watchman" => Some(Watchman),
            "Concentration" => Some(Concentration),
            "Gunnery" => Some(Gunnery),
            "Assault Specialist" => Some(AssaultSpecialist),
            "Combat Medic" => Some(CombatMedic),
            "Plasmatech" => Some(Plasmatech),
            "Shield Specialist" => Some(ShieldSpecialist),
            "Tactics" => Some(Tactics),
            "Scrapper" => Some(Scrapper),
            "Ruffian" => Some(Ruffian),
            "Sawbones" => Some(Sawbones),
            "Sharpshooter" => Some(Sharpshooter),
            "Saboteur" => Some(Saboteur),
            "Dirty Fighting" => Some(DirtyFighting),
            _ => None,
        }
    }

    /// Get the icon filename for this discipline (without path)
    pub const fn icon_name(&self) -> &'static str {
        use Discipline::*;
        match self {
            // Sorcerer
            Lightning => "lightning.png",
            Madness => "madness.png",
            Corruption => "corruption.png",
            // Assassin
            Hatred => "hatred.png",
            Darkness => "darkness.png",
            Deception => "deception.png",
            // Juggernaut
            Vengeance => "vengeance.png",
            Immortal => "immortal.png",
            Rage => "rage.png",
            // Marauder
            Annihilation => "annihilation.png",
            Carnage => "carnage.png",
            Fury => "fury.png",
            // Mercenary
            Arsenal => "arsenal.png",
            InnovativeOrdnance => "innovative-ordnance.png",
            Bodyguard => "bodyguard.png",
            // Powertech
            ShieldTech => "shield-tech.png",
            Pyrotech => "pyrotech.png",
            AdvancedPrototype => "advanced-prototype.png",
            // Operative
            Concealment => "concealment.png",
            Lethality => "lethality.png",
            Medicine => "medicine.png",
            // Sniper
            Marksmanship => "marksmanship.png",
            Engineering => "engineering.png",
            Virulence => "virulence.png",
            // Sage
            Telekinetics => "telekinetics.png",
            Seer => "seer.png",
            Balance => "balance.png",
            // Shadow
            Infiltration => "infiltration.png",
            KineticCombat => "kinetic-combat.png",
            Serenity => "serenity.png",
            // Guardian
            Focus => "focus.png",
            Vigilance => "vigilance.png",
            Defense => "defense.png",
            // Sentinel
            Combat => "combat.png",
            Watchman => "watchman.png",
            Concentration => "concentration.png",
            // Commando
            Gunnery => "gunnery.png",
            AssaultSpecialist => "assault-specialist.png",
            CombatMedic => "combat-medic.png",
            // Vanguard
            Plasmatech => "plasmatech.png",
            ShieldSpecialist => "shield-specialist.png",
            Tactics => "tactics.png",
            // Scoundrel
            Scrapper => "scrapper.png",
            Ruffian => "ruffian.png",
            Sawbones => "sawbones.png",
            // Gunslinger
            Sharpshooter => "sharpshooter.png",
            Saboteur => "saboteur.png",
            DirtyFighting => "dirty-fighting.png",
        }
    }

    /// Get all disciplines in display order (grouped by class)
    pub fn all() -> &'static [Discipline] {
        use Discipline::*;
        &[
            // Sorcerer / Sage
            Lightning,
            Madness,
            Corruption,
            Telekinetics,
            Balance,
            Seer,
            // Assassin / Shadow
            Hatred,
            Darkness,
            Deception,
            Infiltration,
            KineticCombat,
            Serenity,
            // Juggernaut / Guardian
            Vengeance,
            Immortal,
            Rage,
            Focus,
            Vigilance,
            Defense,
            // Marauder / Sentinel
            Annihilation,
            Carnage,
            Fury,
            Combat,
            Watchman,
            Concentration,
            // Mercenary / Commando
            Arsenal,
            InnovativeOrdnance,
            Bodyguard,
            Gunnery,
            AssaultSpecialist,
            CombatMedic,
            // Powertech / Vanguard
            ShieldTech,
            Pyrotech,
            AdvancedPrototype,
            Plasmatech,
            ShieldSpecialist,
            Tactics,
            // Operative / Scoundrel
            Concealment,
            Lethality,
            Medicine,
            Scrapper,
            Ruffian,
            Sawbones,
            // Sniper / Gunslinger
            Marksmanship,
            Engineering,
            Virulence,
            Sharpshooter,
            Saboteur,
            DirtyFighting,
        ]
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

/// One entry in a source/target discipline filter: either a whole role
/// (matches every discipline with that role) or a specific discipline.
///
/// Serializes untagged, so TOML accepts plain strings for both forms:
/// `source_disciplines = ["Tank", "Medicine"]`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DisciplineFilter {
    Role(Role),
    Discipline(Discipline),
}

impl DisciplineFilter {
    /// Check whether a discipline satisfies this filter entry
    pub fn matches(&self, discipline: Discipline) -> bool {
        match self {
            Self::Role(role) => discipline.role() == *role,
            Self::Discipline(d) => *d == discipline,
        }
    }

    /// Get the display name for this filter entry
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Role(Role::Tank) => "Tank",
            Self::Role(Role::Healer) => "Healer",
            Self::Role(Role::Dps) => "DPS",
            Self::Discipline(d) => d.name(),
        }
    }

    /// Look up a filter entry from its display name
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "Tank" => Some(Self::Role(Role::Tank)),
            "Healer" => Some(Self::Role(Role::Healer)),
            "DPS" | "Dps" => Some(Self::Role(Role::Dps)),
            _ => Discipline::from_name(name).map(Self::Discipline),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discipline_filter_toml_round_trip() {
        #[derive(Serialize, Deserialize)]
        struct Wrapper {
            filters: Vec<DisciplineFilter>,
        }

        let original = Wrapper {
            filters: vec![
                DisciplineFilter::Role(Role::Tank),
                DisciplineFilter::Role(Role::Dps),
                DisciplineFilter::Discipline(Discipline::Medicine),
                DisciplineFilter::Discipline(Discipline::InnovativeOrdnance),
            ],
        };

        let toml = toml::to_string(&original).unwrap();
        let parsed: Wrapper = toml::from_str(&toml).unwrap();
        assert_eq!(parsed.filters, original.filters);

        // Roles must win over disciplines for their own names, and untagged
        // strings must resolve to the right variant kind
        let parsed: Wrapper =
            toml::from_str(r#"filters = ["Healer", "Deception"]"#).unwrap();
        assert_eq!(
            parsed.filters,
            vec![
                DisciplineFilter::Role(Role::Healer),
                DisciplineFilter::Discipline(Discipline::Deception),
            ]
        );
    }
}
