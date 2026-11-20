use tiny_skia::Color;

#[inline]
pub fn transparent() -> Color {
    Color::from_rgba8(0, 0, 0, 0)
}

#[inline]
pub fn black() -> Color {
    Color::from_rgba8(0, 0, 0, 255)
}

#[inline]
pub fn white() -> Color {
    Color::from_rgba8(255, 255, 255, 255)
}

#[inline]
pub fn red() -> Color {
    Color::from_rgba8(255, 0, 0, 255)
}

#[inline]
pub fn green() -> Color {
    Color::from_rgba8(0, 255, 0, 255)
}

#[inline]
pub fn blue() -> Color {
    Color::from_rgba8(0, 0, 255, 255)
}

#[inline]
pub fn yellow() -> Color {
    Color::from_rgba8(255, 255, 0, 255)
}

/// DPS metric background
#[inline]
pub fn dps_bar_bg() -> Color {
    Color::from_rgba8(60, 60, 60, 180)
}

/// DPS bar fill color
#[inline]
pub fn dps_bar_fill() -> Color {
    Color::from_rgba8(180, 50, 50, 255)
}

/// HPS bar fill color
#[inline]
pub fn hps_bar_fill() -> Color {
    Color::from_rgba8(50, 180, 50, 255)
}

/// Tank bar fill color
#[inline]
pub fn tank_bar_fill() -> Color {
    Color::from_rgba8(50, 100, 180, 255)
}

/// Enemy frame cell background (subtle dark panel)
#[inline]
pub fn enemy_cell_bg() -> Color {
    Color::from_rgba8(25, 25, 28, 190)
}

/// Veil over an enemy frame whose player has dropped out of the log
#[inline]
pub fn enemy_stale_dim() -> Color {
    Color::from_rgba8(0, 0, 0, 150)
}

/// HP bar fill when an enemy's class is not yet detected
#[inline]
pub fn enemy_unknown_class() -> Color {
    Color::from_rgba8(130, 130, 135, 255)
}

/// Guard highlight accent (blue border on guarded enemy frames)
#[inline]
pub fn guard_blue() -> Color {
    Color::from_rgba8(80, 150, 230, 255)
}

/// "Targeting you" alert text (enemy frames target line)
#[inline]
pub fn targeting_you_red() -> Color {
    Color::from_rgba8(235, 80, 80, 255)
}

/// Dimmed label color for secondary text
#[inline]
pub fn label_dim() -> Color {
    Color::from_rgba8(180, 180, 180, 255)
}

// ─────────────────────────────────────────────────────────────────────────
// Personal Stat Category Colors
// ─────────────────────────────────────────────────────────────────────────

/// Damage stat value color (red)
#[inline]
pub fn stat_damage() -> Color {
    Color::from_rgba8(220, 80, 80, 255)
}

/// Healing stat value color (green)
#[inline]
pub fn stat_healing() -> Color {
    Color::from_rgba8(80, 200, 80, 255)
}

/// Mitigation/DTPS stat value color (brownish orange)
#[inline]
pub fn stat_mitigation() -> Color {
    Color::from_rgba8(210, 150, 60, 255)
}

/// Threat stat value color (blue)
#[inline]
pub fn stat_threat() -> Color {
    Color::from_rgba8(80, 140, 220, 255)
}

/// Separator line color (subtle)
#[inline]
pub fn separator_line() -> Color {
    Color::from_rgba8(80, 80, 80, 180)
}

// ─────────────────────────────────────────────────────────────────────────
// Effect Type Colors
// ─────────────────────────────────────────────────────────────────────────

/// Debuff effect color - red
#[inline]
pub fn effect_debuff() -> Color {
    Color::from_rgba8(200, 60, 60, 255)
}

/// Cooldown ready state - bright light-blue
#[inline]
pub fn cooldown_ready() -> Color {
    Color::from_rgba8(100, 200, 255, 255)
}

// ─────────────────────────────────────────────────────────────────────────
// Frame/Window Colors
// ─────────────────────────────────────────────────────────────────────────

/// Frame border color
#[inline]
pub fn frame_border() -> Color {
    Color::from_rgba8(128, 128, 128, 200)
}

/// Resize indicator in corner
#[inline]
pub fn resize_indicator() -> Color {
    Color::from_rgba8(255, 255, 255, 150)
}

// ─────────────────────────────────────────────────────────────────────────
// Raid Frame Colors
// ─────────────────────────────────────────────────────────────────────────

/// Guide lines for rearrange mode
#[inline]
pub fn raid_guide() -> Color {
    Color::from_rgba8(180, 180, 180, 200)
}

/// Empty slot background
#[inline]
pub fn raid_empty_slot() -> Color {
    Color::from_rgba8(50, 50, 50, 98)
}

/// Empty slot text (rearrange mode)
#[inline]
pub fn raid_slot_text() -> Color {
    Color::from_rgba8(120, 180, 255, 255)
}

/// Slot number indicator
#[inline]
pub fn raid_slot_number() -> Color {
    Color::from_rgba8(120, 120, 120, 200)
}

/// Clear button in rearrange mode
#[inline]
pub fn raid_clear_button() -> Color {
    Color::from_rgba8(180, 60, 60, 220)
}

/// Overflow indicator text
#[inline]
pub fn raid_overflow() -> Color {
    Color::from_rgba8(255, 180, 100, 200)
}

/// OCR name not yet tied to a log player. Shares [`raid_overflow`]'s orange.
#[inline]
pub fn raid_name_provisional() -> Color {
    Color::from_rgba8(255, 180, 100, 255)
}

/// Name backed by a log player. Plain white — the default state carries no
/// colour signal; only provisional names are tinted.
#[inline]
pub fn raid_name_confirmed() -> Color {
    Color::from_rgba8(255, 255, 255, 255)
}

// ─────────────────────────────────────────────────────────────────────────
// Health Bar Colors (contextual)
// ─────────────────────────────────────────────────────────────────────────

// ─────────────────────────────────────────────────────────────────────────
// Effect Icon Colors
// ─────────────────────────────────────────────────────────────────────────

/// Effect icon background
#[inline]
pub fn effect_icon_bg() -> Color {
    Color::from_rgba8(80, 80, 80, 150)
}

/// Effect icon border
#[inline]
pub fn effect_icon_border() -> Color {
    Color::from_rgba8(150, 150, 150, 200)
}

/// Preview border for move mode placeholders
#[inline]
pub fn preview_border() -> Color {
    Color::from_rgba8(180, 180, 180, 180)
}

/// Effect bar background (inside icon)
#[inline]
pub fn effect_bar_bg() -> Color {
    Color::from_rgba8(20, 20, 20, 220)
}

/// Effect bar border
#[inline]
pub fn effect_bar_border() -> Color {
    Color::from_rgba8(60, 60, 60, 255)
}

/// Text shadow for icons
#[inline]
pub fn text_shadow() -> Color {
    Color::from_rgba8(0, 0, 0, 160)
}

/// Yellow countdown/charge text for icons (SWTOR style)
#[inline]
pub fn icon_countdown() -> Color {
    Color::from_rgba8(255, 210, 0, 255)
}

/// Muted/inactive text
#[inline]
pub fn text_muted() -> Color {
    Color::from_rgba8(150, 150, 150, 200)
}

// ─────────────────────────────────────────────────────────────────────────
// Role Icon Colors
// ─────────────────────────────────────────────────────────────────────────
