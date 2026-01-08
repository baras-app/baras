//! Class and role icon assets for player display

use dioxus::prelude::*;
use manganis::Asset;

// Pre-declare all class icon assets
static ICON_ASSASSIN: Asset = asset!("/assets/icons/class/assassin.png");
static ICON_COMMANDO: Asset = asset!("/assets/icons/class/commando.png");
static ICON_GUARDIAN: Asset = asset!("/assets/icons/class/guardian.png");
static ICON_GUNSLINGER: Asset = asset!("/assets/icons/class/gunslinger.png");
static ICON_JUGGERNAUT: Asset = asset!("/assets/icons/class/juggernaut.png");
static ICON_MARAUDER: Asset = asset!("/assets/icons/class/marauder.png");
static ICON_MERCENARY: Asset = asset!("/assets/icons/class/mercenary.png");
static ICON_OPERATIVE: Asset = asset!("/assets/icons/class/operative.png");
static ICON_POWERTECH: Asset = asset!("/assets/icons/class/powertech.png");
static ICON_SAGE: Asset = asset!("/assets/icons/class/sage.png");
static ICON_SCOUNDREL: Asset = asset!("/assets/icons/class/scoundrel.png");
static ICON_SENTINEL: Asset = asset!("/assets/icons/class/sentinel.png");
static ICON_SHADOW: Asset = asset!("/assets/icons/class/shadow.png");
static ICON_SNIPER: Asset = asset!("/assets/icons/class/sniper.png");
static ICON_SORCERER: Asset = asset!("/assets/icons/class/sorcerer.png");
static ICON_VANGUARD: Asset = asset!("/assets/icons/class/vanguard.png");

// Pre-declare role icon assets
static ICON_ROLE_TANK: Asset = asset!("/assets/icons/role/icon_tank.png");
static ICON_ROLE_HEALER: Asset = asset!("/assets/icons/role/icon_heal.png");
static ICON_ROLE_DPS: Asset = asset!("/assets/icons/role/icon_dps.png");

/// Get the asset for a class icon by filename
pub fn get_class_icon(icon_name: &str) -> Option<&'static Asset> {
    match icon_name {
        "assassin.png" => Some(&ICON_ASSASSIN),
        "commando.png" => Some(&ICON_COMMANDO),
        "guardian.png" => Some(&ICON_GUARDIAN),
        "gunslinger.png" => Some(&ICON_GUNSLINGER),
        "juggernaut.png" => Some(&ICON_JUGGERNAUT),
        "marauder.png" => Some(&ICON_MARAUDER),
        "mercenary.png" => Some(&ICON_MERCENARY),
        "operative.png" => Some(&ICON_OPERATIVE),
        "powertech.png" => Some(&ICON_POWERTECH),
        "sage.png" => Some(&ICON_SAGE),
        "scoundrel.png" => Some(&ICON_SCOUNDREL),
        "sentinel.png" => Some(&ICON_SENTINEL),
        "shadow.png" => Some(&ICON_SHADOW),
        "sniper.png" => Some(&ICON_SNIPER),
        "sorcerer.png" => Some(&ICON_SORCERER),
        "vanguard.png" => Some(&ICON_VANGUARD),
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
