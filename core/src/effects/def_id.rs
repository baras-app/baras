//! Compact handle to a definition within one `DefinitionSet`.

/// Index of a definition in its `DefinitionSet`. Only valid for the set that
/// issued it: a reload issues new ids (`EffectTracker::set_definitions` remaps).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DefId(pub(super) u32);
