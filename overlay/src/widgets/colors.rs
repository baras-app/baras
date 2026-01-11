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

/// Semi-transparent background for overlays
#[inline]
pub fn overlay_bg() -> Color {
    Color::from_rgba8(30, 30, 30, 10)
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

/// Dimmed label color for secondary text
#[inline]
pub fn label_dim() -> Color {
    Color::from_rgba8(180, 180, 180, 255)
}

// ─────────────────────────────────────────────────────────────────────────
// Effect Type Colors
// ─────────────────────────────────────────────────────────────────────────

/// HoT (Heal over Time) effect color - green
#[inline]
pub fn effect_hot() -> Color {
    Color::from_rgba8(80, 200, 80, 255)
}

/// Debuff effect color - red
#[inline]
pub fn effect_debuff() -> Color {
    Color::from_rgba8(200, 60, 60, 255)
}

/// Buff effect color - blue
#[inline]
pub fn effect_buff() -> Color {
    Color::from_rgba8(80, 140, 220, 255)
}

/// Shield/absorb effect color - yellow/gold
#[inline]
pub fn effect_shield() -> Color {
    Color::from_rgba8(220, 180, 50, 255)
}

/// Cleanse/dispellable effect color - purple
#[inline]
pub fn effect_cleansable() -> Color {
    Color::from_rgba8(180, 80, 200, 255)
}

/// Proc/temporary buff color - cyan
#[inline]
pub fn effect_proc() -> Color {
    Color::from_rgba8(80, 200, 220, 255)
}

/// Cooldown ready state - bright light-blue
#[inline]
pub fn cooldown_ready() -> Color {
    Color::from_rgba8(100, 200, 255, 255)
}

// ─────────────────────────────────────────────────────────────────────────
// Frame/Window Colors
// ─────────────────────────────────────────────────────────────────────────

/// Default frame background
#[inline]
pub fn frame_bg() -> Color {
    Color::from_rgba8(30, 30, 30, 255)
}

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

/// Raid frame player background
#[inline]
pub fn raid_frame_bg() -> Color {
    Color::from_rgba8(40, 40, 40, 200)
}

/// Selection highlight for raid frames
#[inline]
pub fn raid_selection() -> Color {
    Color::from_rgba8(80, 120, 180, 220)
}

/// Guide lines for rearrange mode
#[inline]
pub fn raid_guide() -> Color {
    Color::from_rgba8(180, 180, 180, 200)
}

/// Empty slot background
#[inline]
pub fn raid_empty_slot() -> Color {
    Color::from_rgba8(50, 50, 50, 140)
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

// ─────────────────────────────────────────────────────────────────────────
// Health Bar Colors (contextual)
// ─────────────────────────────────────────────────────────────────────────

/// Health bar - healthy (>60%)
#[inline]
pub fn health_high() -> Color {
    Color::from_rgba8(80, 200, 80, 255)
}

/// Health bar - medium (30-60%)
#[inline]
pub fn health_medium() -> Color {
    Color::from_rgba8(220, 180, 50, 255)
}

/// Health bar - low (<30%)
#[inline]
pub fn health_low() -> Color {
    Color::from_rgba8(220, 60, 60, 255)
}

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

/// Muted/inactive text
#[inline]
pub fn text_muted() -> Color {
    Color::from_rgba8(150, 150, 150, 200)
}

// ─────────────────────────────────────────────────────────────────────────
// Role Icon Colors
// ─────────────────────────────────────────────────────────────────────────

/// Tank role icon (blue shield)
#[inline]
pub fn role_tank() -> Color {
    Color::from_rgba8(100, 150, 220, 255)
}

/// Healer role icon (green cross)
#[inline]
pub fn role_healer() -> Color {
    Color::from_rgba8(100, 220, 100, 255)
}
