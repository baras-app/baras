//! Effect ID → raid debuff (from data/effect_data.csv), and the disciplines
//! that apply each debuff (from data/raid_debuff_disciplines.csv).

use baras_types::RaidDebuff;

include!(concat!(env!("OUT_DIR"), "/raid_debuffs.rs"));
