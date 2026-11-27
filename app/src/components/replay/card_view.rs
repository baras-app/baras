//! What one entity's card shows at a playback time

/// A buff or debuff on a card
#[derive(Clone, PartialEq)]
pub struct IconView {
    pub effect_id: i64,
    pub ability_id: i64,
    pub name: String,
    /// Applied by the highlighted entity
    pub from_highlight: bool,
}

#[derive(Clone, PartialEq)]
pub struct CastView {
    pub name: String,
    pub ability_id: i64,
    /// Progress, 0–100
    pub pct: u8,
    pub is_channel: bool,
    pub interrupted: bool,
}

#[derive(Clone, PartialEq, Default)]
pub struct CardView {
    pub hp: i32,
    pub max_hp: i32,
    pub dead: bool,
    /// Index of the targeted entity in the replay data
    pub target: Option<usize>,
    pub cast: Option<CastView>,
    pub buffs: Vec<IconView>,
    pub debuffs: Vec<IconView>,
    /// Buffs and debuffs past the icon limit
    pub more_buffs: usize,
    pub more_debuffs: usize,
    /// Damage and healing per second over the last seconds, when asked for
    pub rates: Option<(f32, f32)>,
    /// With maps: the entity is not on the one shown, so not on the canvas
    pub off_map: bool,
    /// First map entry covering the entity, to say where it is instead
    pub map: Option<usize>,
}
