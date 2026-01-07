// this is exhaustive
pub mod effect_type_id {
    pub const APPLYEFFECT: i64 = 836045448945477;
    pub const AREAENTERED: i64 = 836045448953664;
    pub const DISCIPLINECHANGED: i64 = 836045448953665;
    pub const EVENT: i64 = 836045448945472;
    pub const MODIFYCHARGES: i64 = 836045448953666;
    pub const REMOVEEFFECT: i64 = 836045448945478;
    pub const RESTORE: i64 = 836045448945476;
    pub const SPEND: i64 = 836045448945473;
}

// common ones only, not exhaustive
pub mod effect_id {
    pub const ABILITYACTIVATE: i64 = 836045448945479;
    pub const ABILITYCANCEL: i64 = 836045448945481;
    pub const ABILITYDEACTIVATE: i64 = 836045448945480;
    pub const ABILITYINTERRUPT: i64 = 836045448945482;
    pub const DEATH: i64 = 836045448945493;
    pub const DAMAGE: i64 = 836045448945501;
    pub const ENTERCOMBAT: i64 = 836045448945489;
    pub const EXITCOMBAT: i64 = 836045448945490;
    pub const FAILEDEFFECT: i64 = 836045448945499;
    pub const HEAL: i64 = 836045448945500;
    pub const REVIVED: i64 = 836045448945494;
    pub const TARGETCLEARED: i64 = 836045448953669;
    pub const TARGETSET: i64 = 836045448953668;
    pub const TAUNT: i64 = 836045448945488;
}

// SWTOR bug: These abilities report 6 charges on ApplyEffect instead of 7
const CHARGE_BUG_ABILITIES: [i64; 2] = [
    999516199190528, // Trauma Probe
    985226842996736, // Kolto Shell
];

/// Correct charge counts for abilities with known SWTOR logging bugs.
/// Only apply to ApplyEffect events, not ModifyCharges.
pub fn correct_apply_charges(effect_id: i64, charges: u8) -> u8 {
    if CHARGE_BUG_ABILITIES.contains(&effect_id) {
        charges.saturating_add(1)
    } else {
        charges
    }
}

pub mod defense_type {
    pub const REFLECTED: i64 = 836045448953649;
    pub const ABSORBED: i64 = 836045448945511;
    pub const COVER: i64 = 836045448945510;
    pub const DEFLECT: i64 = 836045448945508;
    pub const DODGE: i64 = 836045448945505;
    pub const IMMUNE: i64 = 836045448945506;
    pub const MISS: i64 = 836045448945502;
    pub const PARRY: i64 = 836045448945503;
    pub const RESIST: i64 = 836045448945507;
    pub const SHIELD: i64 = 836045448945509;
}
