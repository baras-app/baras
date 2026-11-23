//! Interrupt ability IDs, generated from `data/ability_data.csv` (`is_interrupt` and off the GCD),
//! and the cleanses, a list in `build.rs`.

include!(concat!(env!("OUT_DIR"), "/interrupt_abilities.rs"));
include!(concat!(env!("OUT_DIR"), "/cleanse_abilities.rs"));
