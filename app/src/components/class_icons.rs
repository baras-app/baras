//! Discipline and role icon assets for player display

use dioxus::prelude::*;
use manganis::Asset;

// Pre-declare all discipline icon assets
static ICON_LIGHTNING: Asset = asset!("/assets/icons/discipline/lightning.png");
static ICON_MADNESS: Asset = asset!("/assets/icons/discipline/madness.png");
static ICON_CORRUPTION: Asset = asset!("/assets/icons/discipline/corruption.png");
static ICON_HATRED: Asset = asset!("/assets/icons/discipline/hatred.png");
static ICON_DARKNESS: Asset = asset!("/assets/icons/discipline/darkness.png");
static ICON_DECEPTION: Asset = asset!("/assets/icons/discipline/deception.png");
static ICON_VENGEANCE: Asset = asset!("/assets/icons/discipline/vengeance.png");
static ICON_IMMORTAL: Asset = asset!("/assets/icons/discipline/immortal.png");
static ICON_RAGE: Asset = asset!("/assets/icons/discipline/rage.png");
static ICON_ANNIHILATION: Asset = asset!("/assets/icons/discipline/annihilation.png");
static ICON_CARNAGE: Asset = asset!("/assets/icons/discipline/carnage.png");
static ICON_FURY: Asset = asset!("/assets/icons/discipline/fury.png");
static ICON_ARSENAL: Asset = asset!("/assets/icons/discipline/arsenal.png");
static ICON_INNOVATIVE_ORDNANCE: Asset = asset!("/assets/icons/discipline/innovative-ordnance.png");
static ICON_BODYGUARD: Asset = asset!("/assets/icons/discipline/bodyguard.png");
static ICON_SHIELD_TECH: Asset = asset!("/assets/icons/discipline/shield-tech.png");
static ICON_PYROTECH: Asset = asset!("/assets/icons/discipline/pyrotech.png");
static ICON_ADVANCED_PROTOTYPE: Asset = asset!("/assets/icons/discipline/advanced-prototype.png");
static ICON_CONCEALMENT: Asset = asset!("/assets/icons/discipline/concealment.png");
static ICON_LETHALITY: Asset = asset!("/assets/icons/discipline/lethality.png");
static ICON_MEDICINE: Asset = asset!("/assets/icons/discipline/medicine.png");
static ICON_MARKSMANSHIP: Asset = asset!("/assets/icons/discipline/marksmanship.png");
static ICON_ENGINEERING: Asset = asset!("/assets/icons/discipline/engineering.png");
static ICON_VIRULENCE: Asset = asset!("/assets/icons/discipline/virulence.png");
static ICON_TELEKINETICS: Asset = asset!("/assets/icons/discipline/telekinetics.png");
static ICON_SEER: Asset = asset!("/assets/icons/discipline/seer.png");
static ICON_BALANCE: Asset = asset!("/assets/icons/discipline/balance.png");
static ICON_INFILTRATION: Asset = asset!("/assets/icons/discipline/infiltration.png");
static ICON_KINETIC_COMBAT: Asset = asset!("/assets/icons/discipline/kinetic-combat.png");
static ICON_SERENITY: Asset = asset!("/assets/icons/discipline/serenity.png");
static ICON_FOCUS: Asset = asset!("/assets/icons/discipline/focus.png");
static ICON_VIGILANCE: Asset = asset!("/assets/icons/discipline/vigilance.png");
static ICON_DEFENSE: Asset = asset!("/assets/icons/discipline/defense.png");
static ICON_COMBAT: Asset = asset!("/assets/icons/discipline/combat.png");
static ICON_WATCHMAN: Asset = asset!("/assets/icons/discipline/watchman.png");
static ICON_CONCENTRATION: Asset = asset!("/assets/icons/discipline/concentration.png");
static ICON_GUNNERY: Asset = asset!("/assets/icons/discipline/gunnery.png");
static ICON_ASSAULT_SPECIALIST: Asset = asset!("/assets/icons/discipline/assault-specialist.png");
static ICON_COMBAT_MEDIC: Asset = asset!("/assets/icons/discipline/combat-medic.png");
static ICON_PLASMATECH: Asset = asset!("/assets/icons/discipline/plasmatech.png");
static ICON_SHIELD_SPECIALIST: Asset = asset!("/assets/icons/discipline/shield-specialist.png");
static ICON_TACTICS: Asset = asset!("/assets/icons/discipline/tactics.png");
static ICON_SCRAPPER: Asset = asset!("/assets/icons/discipline/scrapper.png");
static ICON_RUFFIAN: Asset = asset!("/assets/icons/discipline/ruffian.png");
static ICON_SAWBONES: Asset = asset!("/assets/icons/discipline/sawbones.png");
static ICON_SHARPSHOOTER: Asset = asset!("/assets/icons/discipline/sharpshooter.png");
static ICON_SABOTEUR: Asset = asset!("/assets/icons/discipline/saboteur.png");
static ICON_DIRTY_FIGHTING: Asset = asset!("/assets/icons/discipline/dirty-fighting.png");
static ICON_COMPANION: Asset = asset!("/assets/icons/discipline/companion.png");

// Pre-declare role icon assets
static ICON_ROLE_TANK: Asset = asset!("/assets/icons/role/icon_tank.png");
static ICON_ROLE_HEALER: Asset = asset!("/assets/icons/role/icon_heal.png");
static ICON_ROLE_DPS: Asset = asset!("/assets/icons/role/icon_dps.png");

/// Get the asset for a class/discipline icon by filename
pub fn get_class_icon(icon_name: &str) -> Option<&'static Asset> {
    match icon_name {
        // Discipline icons
        "lightning.png" => Some(&ICON_LIGHTNING),
        "madness.png" => Some(&ICON_MADNESS),
        "corruption.png" => Some(&ICON_CORRUPTION),
        "hatred.png" => Some(&ICON_HATRED),
        "darkness.png" => Some(&ICON_DARKNESS),
        "deception.png" => Some(&ICON_DECEPTION),
        "vengeance.png" => Some(&ICON_VENGEANCE),
        "immortal.png" => Some(&ICON_IMMORTAL),
        "rage.png" => Some(&ICON_RAGE),
        "annihilation.png" => Some(&ICON_ANNIHILATION),
        "carnage.png" => Some(&ICON_CARNAGE),
        "fury.png" => Some(&ICON_FURY),
        "arsenal.png" => Some(&ICON_ARSENAL),
        "innovative-ordnance.png" => Some(&ICON_INNOVATIVE_ORDNANCE),
        "bodyguard.png" => Some(&ICON_BODYGUARD),
        "shield-tech.png" => Some(&ICON_SHIELD_TECH),
        "pyrotech.png" => Some(&ICON_PYROTECH),
        "advanced-prototype.png" => Some(&ICON_ADVANCED_PROTOTYPE),
        "concealment.png" => Some(&ICON_CONCEALMENT),
        "lethality.png" => Some(&ICON_LETHALITY),
        "medicine.png" => Some(&ICON_MEDICINE),
        "marksmanship.png" => Some(&ICON_MARKSMANSHIP),
        "engineering.png" => Some(&ICON_ENGINEERING),
        "virulence.png" => Some(&ICON_VIRULENCE),
        "telekinetics.png" => Some(&ICON_TELEKINETICS),
        "seer.png" => Some(&ICON_SEER),
        "balance.png" => Some(&ICON_BALANCE),
        "infiltration.png" => Some(&ICON_INFILTRATION),
        "kinetic-combat.png" => Some(&ICON_KINETIC_COMBAT),
        "serenity.png" => Some(&ICON_SERENITY),
        "focus.png" => Some(&ICON_FOCUS),
        "vigilance.png" => Some(&ICON_VIGILANCE),
        "defense.png" => Some(&ICON_DEFENSE),
        "combat.png" => Some(&ICON_COMBAT),
        "watchman.png" => Some(&ICON_WATCHMAN),
        "concentration.png" => Some(&ICON_CONCENTRATION),
        "gunnery.png" => Some(&ICON_GUNNERY),
        "assault-specialist.png" => Some(&ICON_ASSAULT_SPECIALIST),
        "combat-medic.png" => Some(&ICON_COMBAT_MEDIC),
        "plasmatech.png" => Some(&ICON_PLASMATECH),
        "shield-specialist.png" => Some(&ICON_SHIELD_SPECIALIST),
        "tactics.png" => Some(&ICON_TACTICS),
        "scrapper.png" => Some(&ICON_SCRAPPER),
        "ruffian.png" => Some(&ICON_RUFFIAN),
        "sawbones.png" => Some(&ICON_SAWBONES),
        "sharpshooter.png" => Some(&ICON_SHARPSHOOTER),
        "saboteur.png" => Some(&ICON_SABOTEUR),
        "dirty-fighting.png" => Some(&ICON_DIRTY_FIGHTING),
        "companion.png" => Some(&ICON_COMPANION),
        _ => None,
    }
}

/// Get the asset for a role icon by filename
pub fn get_role_icon(icon_name: &str) -> Option<&'static Asset> {
    match icon_name {
        "icon_tank.png" => Some(&ICON_ROLE_TANK),
        "icon_heal.png" => Some(&ICON_ROLE_HEALER),
        "icon_dps.png" => Some(&ICON_ROLE_DPS),
        _ => None,
    }
}
