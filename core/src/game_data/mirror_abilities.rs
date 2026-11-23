//! Republic ⇄ Imperial mirror-class ability pairs, keyed by ability ID, and
//! the pairs of their named effects, keyed by effect ID.
//!
//! Generated at build time from data/ability_data.csv (`mirror_fqn` column)
//! and data/effect_data.csv. The map is symmetric: both halves of a pair are
//! present as keys. An ID missing here may still be a variant of a mirrored
//! ability: look up its `base_ability_id` next.

/// The mirror counterpart of a class ability.
#[derive(Debug)]
pub struct MirrorAbility {
    pub mirror_id: i64,
    /// English name of the mirror ability.
    pub mirror_name: &'static str,
    /// Faction of the *keyed* ability (true = Imperial), so callers can decide
    /// whether a requested faction needs a swap at all.
    pub is_imperial: bool,
}

include!(concat!(env!("OUT_DIR"), "/mirror_abilities.rs"));
