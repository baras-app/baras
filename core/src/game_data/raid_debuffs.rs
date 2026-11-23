//! Effect ID → raid debuff, generated from data/effect_data.csv.

use baras_types::RaidDebuff;

include!(concat!(env!("OUT_DIR"), "/raid_debuffs.rs"));
