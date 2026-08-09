//! Raid Frame Overlay
//!
//! Displays a grid of player frames showing health, effects, and role icons.
//! Supports click-to-swap rearrangement of frames.

use std::time::Instant;
use tiny_skia::Color;

use super::{Overlay, OverlayConfigUpdate, OverlayData, RaidRegistryAction};
use crate::frame::OverlayFrame;
use crate::platform::{OverlayConfig, PlatformError};
use crate::utils::truncate_name;
use crate::widgets::colors;

// ─────────────────────────────────────────────────────────────────────────────
// Player Role
// ─────────────────────────────────────────────────────────────────────────────

/// Player role for icon display
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlayerRole {
    #[default]
    Dps,
    Tank,
    Healer,
}

impl PlayerRole {
    /// Determine role from SWTOR discipline name
    pub fn from_discipline(discipline: &str) -> Self {
        let lower = discipline.to_lowercase();

        // Tank disciplines
        if lower.contains("tank")
            || matches!(
                lower.as_str(),
                "immortal"
                    | "darkness"
                    | "shield specialist"
                    | "shield tech"
                    | "defense"
                    | "kinetic combat"
            )
        {
            PlayerRole::Tank
        }
        // Healer disciplines
        else if lower.contains("heal")
            || matches!(
                lower.as_str(),
                "corruption" | "medicine" | "bodyguard" | "combat medic" | "seer" | "sawbones"
            )
        {
            PlayerRole::Healer
        } else {
            PlayerRole::Dps
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Raid Effect
// ─────────────────────────────────────────────────────────────────────────────

/// A tracked effect on a player (buff, debuff, HoT, etc.)
#[derive(Debug, Clone)]
pub struct RaidEffect {
    /// Unique ID for this effect instance
    pub effect_id: u64,
    /// Display name of the effect
    pub name: String,
    /// Number of stacks/charges (0 = no stacking display)
    pub charges: u8,
    /// When this effect expires (None = permanent until removed)
    pub expires_at: Option<Instant>,
    /// Total duration of the effect (for fill percentage calculation)
    pub duration: Option<std::time::Duration>,
    /// Color for the effect indicator
    pub color: Color,
    /// Is this a beneficial effect?
    pub is_buff: bool,
    /// Pre-loaded icon RGBA data (width, height, rgba_bytes) - Arc for cheap cloning
    pub icon: Option<std::sync::Arc<(u32, u32, Vec<u8>)>>,
    /// Per-effect icon toggle from the effect definition; combined with the
    /// overlay-level `show_effect_icons` config (both must be true to render an icon)
    pub show_icon: bool,
}

impl RaidEffect {
    pub fn new(effect_id: u64, name: impl Into<String>) -> Self {
        Self {
            effect_id,
            name: name.into(),
            charges: 0,
            expires_at: None,
            duration: None,
            color: Color::from_rgba8(100, 180, 255, 255),
            is_buff: true,
            icon: None,
            show_icon: true,
        }
    }

    pub fn with_charges(mut self, charges: u8) -> Self {
        self.charges = charges;
        self
    }

    pub fn with_expiry(mut self, expires_at: Instant) -> Self {
        self.expires_at = Some(expires_at);
        self
    }

    /// Set the effect duration (used for fill percentage calculation)
    pub fn with_duration(mut self, duration: std::time::Duration) -> Self {
        self.duration = Some(duration);
        self
    }

    /// Convenience: set both expiry and duration from a duration value
    pub fn with_duration_from_now(mut self, duration: std::time::Duration) -> Self {
        self.duration = Some(duration);
        self.expires_at = Some(Instant::now() + duration);
        self
    }

    pub fn with_color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    /// Set color from RGBA u8 array (convenience for external code)
    pub fn with_color_rgba(mut self, rgba: [u8; 4]) -> Self {
        self.color = Color::from_rgba8(rgba[0], rgba[1], rgba[2], rgba[3]);
        self
    }

    pub fn with_is_buff(mut self, is_buff: bool) -> Self {
        self.is_buff = is_buff;
        self
    }

    /// Set the icon data (RGBA pixels)
    pub fn with_icon(mut self, icon: std::sync::Arc<(u32, u32, Vec<u8>)>) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Set the per-effect icon toggle
    pub fn with_show_icon(mut self, show_icon: bool) -> Self {
        self.show_icon = show_icon;
        self
    }

    /// Check if the effect has expired
    pub fn is_expired(&self) -> bool {
        self.expires_at.is_some_and(|exp| exp <= Instant::now())
    }

    /// Calculate the fill percentage (0.0 = expired, 1.0 = full duration remaining)
    /// Returns 1.0 if no duration/expiry is set (permanent effect)
    pub fn fill_percent(&self) -> f32 {
        match (self.expires_at, self.duration) {
            (Some(expires), Some(duration)) => {
                let now = Instant::now();
                if now >= expires {
                    0.0
                } else {
                    let remaining = expires.duration_since(now);
                    (remaining.as_secs_f32() / duration.as_secs_f32()).clamp(0.0, 1.0)
                }
            }
            _ => 1.0, // Permanent effect or no duration info
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Raid Frame
// ─────────────────────────────────────────────────────────────────────────────

/// A single player frame in the raid display
#[derive(Debug, Clone)]
pub struct RaidFrame {
    /// Visual slot position (0-15 for a 16-player raid)
    pub slot: u8,
    /// Player entity ID (None if slot is empty)
    pub player_id: Option<i64>,
    /// Player display name
    pub name: String,
    /// Current HP percentage (0.0 - 1.0)
    pub hp_percent: f32,
    /// Player's role
    pub role: PlayerRole,
    /// Class icon filename (e.g., "assassin.png") for class icon display
    pub class_icon: Option<String>,
    /// Active effects on this player
    pub effects: Vec<RaidEffect>,
    /// Is this the local player?
    pub is_self: bool,
}

impl RaidFrame {
    /// Create an empty frame at the given slot
    pub fn empty(slot: u8) -> Self {
        Self {
            slot,
            player_id: None,
            name: String::new(),
            hp_percent: 0.0,
            role: PlayerRole::Dps,
            class_icon: None,
            effects: Vec::new(),
            is_self: false,
        }
    }

    /// A provisional OCR label still counts as an occupied frame.
    pub fn is_empty(&self) -> bool {
        self.name.is_empty()
    }

    /// Clear the frame (remove player)
    pub fn clear(&mut self) {
        self.player_id = None;
        self.name.clear();
        self.hp_percent = 0.0;
        self.role = PlayerRole::Dps;
        self.class_icon = None;
        self.effects.clear();
        self.is_self = false;
    }

    /// Apply or refresh an effect
    pub fn apply_effect(&mut self, effect: RaidEffect, max_effects: usize) {
        // Check if effect already exists
        if let Some(existing) = self
            .effects
            .iter_mut()
            .find(|e| e.effect_id == effect.effect_id)
        {
            // Refresh: update expiry, duration, and take higher stack count
            existing.expires_at = effect.expires_at;
            existing.duration = effect.duration;
            existing.charges = existing.charges.max(effect.charges);
        } else if self.effects.len() < max_effects {
            // New effect, have room
            self.effects.push(effect);
        }
        // At max effects: ignore new effect (oldest stay)
    }

    /// Remove an effect by ID
    pub fn remove_effect(&mut self, effect_id: u64) {
        self.effects.retain(|e| e.effect_id != effect_id);
    }

    /// Remove all expired effects
    pub fn prune_expired_effects(&mut self) {
        self.effects.retain(|e| !e.is_expired());
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Swap State
// ─────────────────────────────────────────────────────────────────────────────

/// State for the click-to-swap interaction
#[derive(Debug, Clone, Copy, Default)]
pub struct SwapState {
    /// Currently selected slot for swapping (first click)
    pub selected_slot: Option<u8>,
}

impl SwapState {
    /// Handle a click on a slot
    /// Returns Some((a, b)) if a swap should occur between slots a and b
    pub fn on_click(&mut self, slot: u8) -> Option<(u8, u8)> {
        match self.selected_slot {
            None => {
                // First click: select this slot
                self.selected_slot = Some(slot);
                None
            }
            Some(first) if first == slot => {
                // Clicked same slot: deselect
                self.selected_slot = None;
                None
            }
            Some(first) => {
                // Second click on different slot: perform swap
                self.selected_slot = None;
                Some((first, slot))
            }
        }
    }

    /// Cancel selection (e.g., on Escape or background click)
    pub fn cancel(&mut self) {
        self.selected_slot = None;
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Interaction Mode
// ─────────────────────────────────────────────────────────────────────────────

/// Interaction mode for the overlay
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InteractionMode {
    #[default]
    Normal, // click_through = true, clicks pass through
    Move,      // click_through = false, drag = move window
    Rearrange, // click_through = false, click = swap slots
}

impl InteractionMode {
    fn shows_detect_button(self) -> bool {
        matches!(self, Self::Move | Self::Rearrange)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Grid Layout
// ─────────────────────────────────────────────────────────────────────────────

/// Layout configuration for the raid grid
#[derive(Debug, Clone, Copy)]
pub struct RaidGridLayout {
    /// Number of columns
    pub columns: u8,
    /// Number of rows
    pub rows: u8,
}

impl RaidGridLayout {
    /// Create a layout from config-defined columns/rows
    pub fn from_config(settings: &baras_core::context::RaidOverlaySettings) -> Self {
        Self {
            columns: settings.grid_columns.clamp(1, 6),
            rows: settings.grid_rows.clamp(1, 24),
        }
    }

    /// Create a layout for the given player count
    pub fn for_player_count(count: u8) -> Self {
        match count {
            0..=4 => Self {
                columns: 1,
                rows: 4,
            },
            5..=8 => Self {
                columns: 2,
                rows: 4,
            },
            _ => Self {
                columns: 4,
                rows: 4,
            },
        }
    }

    /// Total number of slots
    pub fn capacity(&self) -> u8 {
        self.columns * self.rows
    }
}

impl Default for RaidGridLayout {
    fn default() -> Self {
        Self {
            columns: 2,
            rows: 4,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Raid Overlay Config
// ─────────────────────────────────────────────────────────────────────────────

/// Effect size bounds (in pixels before scaling)
pub const EFFECT_SIZE_MIN: f32 = 8.0;
pub const EFFECT_SIZE_MAX: f32 = 36.0;
pub const EFFECT_SIZE_DEFAULT: f32 = 14.0;

/// Effect vertical offset bounds (relative to frame top)
/// Negative = above frame, Positive = further into frame
pub const EFFECT_OFFSET_MIN: f32 = -20.0;
pub const EFFECT_OFFSET_MAX: f32 = 30.0;
pub const EFFECT_OFFSET_DEFAULT: f32 = 3.0;

/// Configuration for the raid overlay appearance
#[derive(Debug, Clone)]
pub struct RaidOverlayConfig {
    /// Show role icons (tank shield, healer cross)
    pub show_role_icons: bool,
    /// Show class icons (white silhouette of the player's class)
    pub show_class_icons: bool,
    /// Maximum effects to display per frame
    pub max_effects_per_frame: u8,
    /// Frame background color (only visible in move mode)
    pub frame_bg_color: [u8; 4],
    /// Selected frame highlight color (rearrange mode)
    pub selection_color: [u8; 4],

    // ─── Effect Display Settings ───────────────────────────────────────────
    /// Size of effect squares in pixels (before scaling)
    /// Clamped to [EFFECT_SIZE_MIN, EFFECT_SIZE_MAX]
    pub effect_size: f32,
    /// Vertical offset of effects from frame top
    /// Negative = above frame, Positive = into frame
    /// Clamped to [EFFECT_OFFSET_MIN, EFFECT_OFFSET_MAX]
    pub effect_vertical_offset: f32,
    /// Opacity of the effect fill (0-255)
    /// Lower values useful when icons are displayed as background
    pub effect_fill_opacity: u8,
    /// Whether to render effect icons (true) or colored squares (false)
    pub show_effect_icons: bool,
    /// Spacing between raid frames in the grid (before scaling)
    /// Clamped to [0.0, 20.0]
    pub frame_spacing: f32,
}

impl Default for RaidOverlayConfig {
    fn default() -> Self {
        Self {
            show_role_icons: true,
            show_class_icons: false,
            max_effects_per_frame: 4,
            frame_bg_color: [40, 40, 40, 200],
            selection_color: [80, 120, 180, 220],
            effect_size: EFFECT_SIZE_DEFAULT,
            effect_vertical_offset: EFFECT_OFFSET_DEFAULT,
            effect_fill_opacity: 255, // Fully opaque when no icons
            show_effect_icons: false,
            frame_spacing: BASE_GAP,
        }
    }
}

impl RaidOverlayConfig {
    /// Get the clamped effect size
    pub fn effect_size(&self) -> f32 {
        self.effect_size.clamp(EFFECT_SIZE_MIN, EFFECT_SIZE_MAX)
    }

    /// Get the clamped effect vertical offset
    pub fn effect_vertical_offset(&self) -> f32 {
        self.effect_vertical_offset
            .clamp(EFFECT_OFFSET_MIN, EFFECT_OFFSET_MAX)
    }
}

impl From<baras_core::context::RaidOverlaySettings> for RaidOverlayConfig {
    fn from(settings: baras_core::context::RaidOverlaySettings) -> Self {
        Self {
            show_role_icons: settings.show_role_icons,
            show_class_icons: settings.show_class_icons,
            max_effects_per_frame: settings.max_effects_per_frame,
            frame_bg_color: settings.frame_bg_color,
            selection_color: [80, 120, 180, 220], // Keep hardcoded for now
            effect_size: settings.effect_size,
            effect_vertical_offset: settings.effect_vertical_offset,
            effect_fill_opacity: settings.effect_fill_opacity,
            show_effect_icons: settings.show_effect_icons,
            frame_spacing: settings.frame_spacing.clamp(0.0, 75.0),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Raid Frame Data (for OverlayData enum)
// ─────────────────────────────────────────────────────────────────────────────

/// Data update for the raid overlay
#[derive(Debug, Clone)]
pub struct RaidFrameData {
    pub frames: Vec<RaidFrame>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Raid Overlay
// ─────────────────────────────────────────────────────────────────────────────

/// Base dimensions for scaling calculations
const BASE_WIDTH: f32 = 220.0;
const BASE_HEIGHT: f32 = 180.0;
const BASE_GAP: f32 = 4.0;
const BASE_PADDING: f32 = 8.0;
const DETECT_BUTTON_EDGE_OVERLAP: f32 = 2.0;
const DETECT_BUTTON_HIT_PADDING: f32 = 5.0;
/// Make sure the overlay is fully blanked.
/// 60ms should be save, accounting for compositor and frame delay.
/// We can optimize this and actually ask the compositor, but, a lot of extra work...
const BLANK_SETTLE_MS: u64 = 60;
/// How long the information messages stay on screen
/// I think 8 is sweetspot.
/// Update if needed.
const DETECTION_MESSAGE_SECS: u64 = 8;

struct RaidSlotGeometry {
    padding: f32,
    gap: f32,
    frame_width: f32,
    frame_height: f32,
}

fn raid_slot_geometry(
    width: u32,
    height: u32,
    layout: RaidGridLayout,
    frame_spacing: f32,
) -> RaidSlotGeometry {
    let width = width as f32;
    let height = height as f32;
    let columns = layout.columns.max(1) as f32;
    let rows = layout.rows.max(1) as f32;
    let scale = ((width / BASE_WIDTH) * (height / BASE_HEIGHT)).sqrt();
    let padding = BASE_PADDING * scale;
    let gap = frame_spacing;

    RaidSlotGeometry {
        padding,
        gap,
        frame_width: ((width - 2.0 * padding - (columns - 1.0) * gap) / columns).max(20.0),
        frame_height: ((height - 2.0 * padding - (rows - 1.0) * gap) / rows).max(20.0),
    }
}

/// The slot rectangles used by both the overlay and OCR harness.
pub fn raid_slot_rects(
    width: u32,
    height: u32,
    layout: RaidGridLayout,
    frame_spacing: f32,
) -> Vec<(u8, i32, i32, u32, u32)> {
    let geometry = raid_slot_geometry(width, height, layout, frame_spacing);
    let rows = layout.rows.max(1);
    let capacity = layout.columns.max(1).saturating_mul(rows);

    (0..capacity)
        .map(|slot| {
            let col = (slot / rows) as f32;
            let row = (slot % rows) as f32;
            let x = geometry.padding + col * (geometry.frame_width + geometry.gap);
            let y = geometry.padding + row * (geometry.frame_height + geometry.gap);
            (
                slot,
                x.round() as i32,
                y.round() as i32,
                geometry.frame_width.round().max(1.0) as u32,
                geometry.frame_height.round().max(1.0) as u32,
            )
        })
        .collect()
}

/// Minimum interval between renders in Normal mode (10 FPS = 100ms)
/// This reduces CPU usage significantly while still providing smooth timer countdowns
const RENDER_INTERVAL: std::time::Duration = std::time::Duration::from_millis(100);

/// The complete raid frame overlay
pub struct RaidOverlay {
    frame: OverlayFrame,
    /// Player frames indexed by slot position
    frames: Vec<RaidFrame>,
    /// Grid layout configuration
    layout: RaidGridLayout,
    /// Current interaction mode
    interaction_mode: InteractionMode,
    /// Current swap selection state
    swap_state: SwapState,
    /// Appearance configuration
    config: RaidOverlayConfig,
    /// Number of players not tracked due to full slots
    overflow_count: u8,
    /// Dirty flag - when true, the overlay needs to be re-rendered
    /// In rearrange mode, we skip rendering when this is false to save CPU
    needs_render: bool,
    /// Last render timestamp for frame rate limiting
    last_render: Instant,
    /// Pending registry actions to be sent to the service
    pending_registry_actions: Vec<RaidRegistryAction>,
    detection_result_rx: Option<std::sync::mpsc::Receiver<String>>,
    detection_message: Option<(String, Instant)>,
    european_number_format: bool,
}

impl RaidOverlay {
    /// Create a new raid overlay
    pub fn new(
        window_config: OverlayConfig,
        layout: RaidGridLayout,
        config: RaidOverlayConfig,
        background_alpha: u8,
    ) -> Result<Self, PlatformError> {
        let mut frame = OverlayFrame::new(window_config, BASE_WIDTH, BASE_HEIGHT)?;
        frame.set_background_alpha(background_alpha);
        frame.set_label("Raid Frames");

        // Initialize empty frames for all slots
        let capacity = layout.capacity() as usize;
        let frames = (0..capacity).map(|i| RaidFrame::empty(i as u8)).collect();

        let mut overlay = Self {
            frame,
            frames,
            layout,
            interaction_mode: InteractionMode::Normal,
            swap_state: SwapState::default(),
            config,
            overflow_count: 0,
            needs_render: true,                            // Initial render needed
            last_render: Instant::now() - RENDER_INTERVAL, // Allow immediate first render
            pending_registry_actions: Vec::new(),
            detection_result_rx: None,
            detection_message: None,
            european_number_format: false,
        };

        // Establish correct initial state for Normal mode (background_alpha = 0)
        overlay.set_interaction_mode(InteractionMode::Normal);

        Ok(overlay)
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Scaling helpers
    // ─────────────────────────────────────────────────────────────────────────

    fn padding(&self) -> f32 {
        self.geometry().padding
    }

    fn gap(&self) -> f32 {
        // frame_spacing is a user-configured pixel value — use it directly
        // without scaling (unlike BASE_PADDING which is a design constant)
        self.geometry().gap
    }

    fn geometry(&self) -> RaidSlotGeometry {
        raid_slot_geometry(
            self.frame.width(),
            self.frame.height(),
            self.layout,
            self.config.frame_spacing,
        )
    }

    /// Calculate frame width based on container size and column count
    fn frame_width(&self) -> f32 {
        self.geometry().frame_width
    }

    /// Calculate frame height based on container size and row count
    fn frame_height(&self) -> f32 {
        self.geometry().frame_height
    }

    fn font_size(&self) -> f32 {
        // Scale font relative to frame height for readability
        (self.frame_height() * 0.28).clamp(8.0, 16.0)
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Layout & Hit Testing
    // ─────────────────────────────────────────────────────────────────────────

    /// Calculate the pixel bounds for a given slot (column-first ordering)
    fn slot_bounds(&self, slot: u8) -> (f32, f32, f32, f32) {
        let col = (slot / self.layout.rows) as f32;
        let row = (slot % self.layout.rows) as f32;

        let x = self.padding() + col * (self.frame_width() + self.gap());
        let y = self.padding() + row * (self.frame_height() + self.gap());

        (x, y, self.frame_width(), self.frame_height())
    }

    fn detect_button_bounds(&self) -> (f32, f32, f32, f32) {
        let padding = self.padding();
        let size = padding.clamp(14.0, 22.0);
        let x = (self.frame.width() as f32 - padding - DETECT_BUTTON_EDGE_OVERLAP)
            .min(self.frame.width() as f32 - size - 2.0)
            .max(2.0);
        let y = (padding - size + DETECT_BUTTON_EDGE_OVERLAP).max(2.0);
        (x, y, size, size)
    }

    fn detect_button_hit_bounds(&self) -> (f32, f32, f32, f32) {
        let (x, y, w, h) = self.detect_button_bounds();
        let padding = self
            .frame
            .scaled(DETECT_BUTTON_HIT_PADDING)
            .clamp(4.0, 8.0);
        let left = (x - padding).max(0.0);
        let top = (y - padding).max(0.0);
        let right = (x + w + padding).min(self.frame.width() as f32);
        let bottom = (y + h + padding).min(self.frame.height() as f32);
        (left, top, right - left, bottom - top)
    }

    fn hit_test_detect_button(&self, px: f32, py: f32) -> bool {
        let (x, y, w, h) = self.detect_button_hit_bounds();
        px >= x && px < x + w && py >= y && py < y + h
    }

    /// Find which slot (if any) contains the given point
    fn hit_test(&self, px: f32, py: f32) -> Option<u8> {
        for slot in 0..self.layout.capacity() {
            let (x, y, w, h) = self.slot_bounds(slot);
            if px >= x && px < x + w && py >= y && py < y + h {
                return Some(slot);
            }
        }
        None
    }

    /// Check if point is in the clear button for a slot
    fn hit_test_clear_button(&self, slot: u8, px: f32, py: f32) -> bool {
        let (x, y, w, h) = self.slot_bounds(slot);
        let btn_size = (h * 0.35).clamp(12.0, 18.0);
        let btn_x = x + w - btn_size - 3.0;
        let btn_y = y + 3.0;

        px >= btn_x && px < btn_x + btn_size && py >= btn_y && py < btn_y + btn_size
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Frame Management
    // ─────────────────────────────────────────────────────────────────────────

    /// Swap two frames by slot index
    pub fn swap_frames(&mut self, a: u8, b: u8) {
        let a_idx = a as usize;
        let b_idx = b as usize;

        if a_idx < self.frames.len() && b_idx < self.frames.len() {
            // Swap the frame contents but keep slot indices correct
            self.frames.swap(a_idx, b_idx);
            self.frames[a_idx].slot = a;
            self.frames[b_idx].slot = b;
            self.needs_render = true;
        }
    }

    /// Clear a specific frame
    pub fn clear_frame(&mut self, slot: u8) {
        if let Some(frame) = self.frames.get_mut(slot as usize) {
            // Don't allow clearing self
            if !frame.is_self {
                frame.clear();
                self.needs_render = true;
            }
        }
    }

    /// Clear all frames (except self)
    pub fn clear_all_frames(&mut self) {
        for frame in &mut self.frames {
            if !frame.is_self {
                frame.clear();
            }
        }
        self.overflow_count = 0;
        self.needs_render = true;
    }

    /// Update frames from data
    ///
    /// Important: Incoming data only contains occupied slots.
    /// We must clear all frames first, then apply incoming data,
    /// otherwise cleared slots retain their old content.
    pub fn set_frames(&mut self, new_frames: Vec<RaidFrame>) {
        // First, clear all frames to empty state
        for frame in &mut self.frames {
            frame.clear();
        }

        // Then apply incoming data (only occupied slots)
        for new_frame in new_frames {
            if let Some(existing) = self.frames.get_mut(new_frame.slot as usize) {
                *existing = new_frame;
            }
        }

        // Prune expired effects from all frames
        for frame in &mut self.frames {
            frame.prune_expired_effects();
        }

        self.needs_render = true;
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Interaction Mode
    // ─────────────────────────────────────────────────────────────────────────

    /// Keep clicks around the button out of the window drag handler.
    fn refresh_interactive_region(&mut self) {
        let region = if self.interaction_mode.shows_detect_button() {
            let (x, y, w, h) = self.detect_button_hit_bounds();
            Some((
                x.round() as i32,
                y.round() as i32,
                w.round().max(1.0) as u32,
                h.round().max(1.0) as u32,
            ))
        } else {
            None
        };
        self.frame.set_interactive_region(region);
    }

    fn capture_blanked(&mut self) -> Option<crate::capture::CapturedImage> {
        self.frame.begin_frame();
        self.frame.end_frame();
        std::thread::sleep(std::time::Duration::from_millis(BLANK_SETTLE_MS));

        let result = crate::capture::capture_region(
            self.frame.x(),
            self.frame.y(),
            self.frame.width(),
            self.frame.height(),
        );

        self.needs_render = true;

        match result {
            Ok(image) => Some(image),
            Err(e) => {
                tracing::warn!("Raid frame capture failed: {e}");
                self.set_detection_message("Screen capture failed: assign names manually".into());
                None
            }
        }
    }

    fn set_detection_message(&mut self, message: String) {
        self.detection_message = Some((message, Instant::now()));
        self.needs_render = true;
    }

    fn emit_detect_action(&mut self) {
        if self.detection_result_rx.is_some() {
            self.set_detection_message("Detection is already running".into());
            return;
        }
        let started_at = Instant::now();
        let Some(image) = self.capture_blanked() else {
            return;
        };

        // Slot bounds are logical; a scaled capture returns more pixels than were
        // asked for, so they follow the same factor. Exactly 1.0 on Windows.
        let scale = image.width as f32 / self.frame.width().max(1) as f32;
        let slots = (0..self.layout.capacity())
            .map(|slot| {
                let (x, y, w, h) = self.slot_bounds(slot);
                (
                    slot,
                    (x * scale).round() as i32,
                    (y * scale).round() as i32,
                    (w * scale).round().max(1.0) as u32,
                    (h * scale).round().max(1.0) as u32,
                )
            })
            .collect();

        let (result_tx, result_rx) = std::sync::mpsc::channel();
        self.detection_result_rx = Some(result_rx);
        self.set_detection_message("Reading raid frames...".into());
        self.pending_registry_actions.push(RaidRegistryAction::DetectNames {
            started_at,
            image,
            slots,
            result_tx,
        });
    }

    /// Set the interaction mode
    pub fn set_interaction_mode(&mut self, mode: InteractionMode) {
        self.interaction_mode = mode;
        self.needs_render = true;

        match mode {
            InteractionMode::Normal => {
                // Normal mode: fully transparent overlay, clicks pass through
                self.frame.set_click_through(true);
                self.frame.set_drag_enabled(true);
                self.frame.set_background_alpha(0); // Fully transparent container
                self.swap_state.cancel();
            }
            InteractionMode::Move => {
                // Move mode: semi-transparent container, dashed frame borders for alignment
                self.frame.set_click_through(false);
                self.frame.set_drag_enabled(true);
                self.frame.set_background_alpha(120); // Semi-transparent so overlay bounds are visible
                self.swap_state.cancel();
            }
            InteractionMode::Rearrange => {
                // Rearrange mode: transparent container, clicks go to overlay for swapping
                self.frame.set_click_through(false);
                self.frame.set_drag_enabled(false);
                self.frame.set_background_alpha(0); // Fully transparent container
            }
        }

        self.refresh_interactive_region();
    }

    /// Toggle rearrange mode
    pub fn toggle_rearrange(&mut self) {
        let new_mode = if self.interaction_mode == InteractionMode::Rearrange {
            InteractionMode::Normal
        } else {
            InteractionMode::Rearrange
        };
        self.set_interaction_mode(new_mode);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Rendering
    // ─────────────────────────────────────────────────────────────────────────

    /// Render the overlay
    ///
    /// Frame rate limiting by mode:
    /// - Normal mode: 10 FPS (100ms intervals) for effect timer countdowns
    /// - Move mode: No limit (responsive drag/resize feedback)
    /// - Rearrange mode: Only on state change (dirty flag)
    pub fn render(&mut self) {
        let now = Instant::now();

        match self.interaction_mode {
            InteractionMode::Rearrange => {
                // Only render on state change (click, data update, etc.)
                if !self.needs_render {
                    return;
                }
            }
            InteractionMode::Normal => {
                // Render at 10 FPS for smooth effect timer countdowns
                // Also render immediately if dirty (data update)
                if !self.needs_render && now.duration_since(self.last_render) < RENDER_INTERVAL {
                    return;
                }
            }
            InteractionMode::Move => {
                // No frame rate limit - responsive feedback during drag/resize
            }
        }

        // Clear dirty flag and update last render time
        self.needs_render = false;
        self.last_render = now;

        self.frame.begin_frame();

        // Temporarily move frames out of self so we can pass &RaidFrame while
        // also holding &mut self for drawing. No allocation — Vec header swap only.
        let frames = std::mem::take(&mut self.frames);

        for frame_data in &frames {
            self.render_frame(frame_data);
        }

        // Overlay the rearrange UI if in that mode
        if self.interaction_mode == InteractionMode::Rearrange {
            for frame_data in &frames {
                self.render_rearrange_overlay(frame_data);
            }
        }

        self.frames = frames;

        // Overflow indicator
        self.render_overflow_indicator();

        if self.interaction_mode.shows_detect_button() {
            self.render_detect_button();
        }
        self.render_detection_message();

        self.frame.end_frame();
    }

    fn render_detect_button(&mut self) {
        let (x, y, w, h) = self.detect_button_bounds();

        self.frame.fill_rounded_rect(
            x,
            y,
            w,
            h,
            (w * 0.25).max(2.0),
            Color::from_rgba8(20, 24, 32, 150),
        );

        let inset = w * 0.22;
        let lens_size = (w - inset * 2.0) * 0.78;
        self.frame.stroke_rounded_rect(
            x + inset,
            y + inset,
            lens_size,
            lens_size,
            lens_size / 2.0,
            (w * 0.09).max(1.2),
            Color::from_rgba8(226, 232, 240, 230),
        );

        let handle = (w * 0.22).max(2.0);
        self.frame.fill_rounded_rect(
            x + inset + lens_size * 0.82,
            y + inset + lens_size * 0.82,
            handle,
            handle,
            handle * 0.4,
            Color::from_rgba8(226, 232, 240, 230),
        );
    }

    fn fits(&mut self, text: &str, font_size: f32, max_width: f32) -> bool {
        self.frame.measure_text(text, font_size).0 <= max_width
    }



    /// Break a message into lines that fit `max_width`.
    ///
    /// Breaks at commas if possible, otherwise spaces.
    fn wrap_lines(&mut self, text: &str, font_size: f32, max_width: f32) -> Vec<String> {
        let text = text.trim();
        if text.is_empty() {
            return Vec::new();
        }
        if self.fits(text, font_size, max_width) {
            return vec![text.to_string()];
        }

        let mut lines = Vec::new();
        let mut current = String::new();

        for clause in comma_clauses(text) {
            let candidate = if current.is_empty() {
                clause.clone()
            } else {
                format!("{current} {clause}")
            };
            if current.is_empty() || self.fits(&candidate, font_size, max_width) {
                current = candidate;
            } else {
                lines.push(std::mem::replace(&mut current, clause));
            }
        }
        if !current.is_empty() {
            lines.push(current);
        }

        // A clause that still overflows gets broken on spaces instead.
        let mut wrapped = Vec::with_capacity(lines.len());
        for line in lines {
            if self.fits(&line, font_size, max_width) {
                wrapped.push(line);
            } else {
                wrapped.extend(self.wrap_words(&line, font_size, max_width));
            }
        }
        wrapped
    }

    /// Break on spaces. 
    // A too-long word will overflow.
    fn wrap_words(&mut self, text: &str, font_size: f32, max_width: f32) -> Vec<String> {
        let mut lines = Vec::new();
        let mut current = String::new();

        for word in text.split_whitespace() {
            let candidate = if current.is_empty() {
                word.to_string()
            } else {
                format!("{current} {word}")
            };
            if current.is_empty() || self.fits(&candidate, font_size, max_width) {
                current = candidate;
            } else {
                lines.push(std::mem::replace(&mut current, word.to_string()));
            }
        }

        if !current.is_empty() {
            lines.push(current);
        }
        lines
    }

    fn render_detection_message(&mut self) {
        let Some((message, _)) = &self.detection_message else {
            return;
        };
        // Wrapping needs the frame mutably, so the message cannot stay borrowed.
        let message = message.clone();
        let font_size = self.frame.scaled(9.0).clamp(8.0, 12.0);

        // Panel is inset 3px from the overlay edge and pads its text by 6px.
        let panel_max = self.frame.width() as f32 - 6.0;
        let lines = self.wrap_lines(&message, font_size, panel_max - 12.0);
        if lines.is_empty() {
            return;
        }

        let mut text_width: f32 = 0.0;
        let mut line_height: f32 = font_size;
        for line in &lines {
            let (w, h) = self.frame.measure_text(line, font_size);
            text_width = text_width.max(w);
            line_height = line_height.max(h);
        }

        let width = (text_width + 12.0).min(panel_max);
        let height = line_height * lines.len() as f32 + 8.0;
        let (_, button_y, _, button_height) = self.detect_button_bounds();
        let x = self.frame.width() as f32 - width - 3.0;
        let y = button_y + button_height + 3.0;

        self.frame.fill_rounded_rect(
            x,
            y,
            width,
            height,
            4.0,
            Color::from_rgba8(20, 24, 32, 225),
        );
        for (i, line) in lines.iter().enumerate() {
            self.frame.draw_text_styled(
                line,
                x + 6.0,
                y + 4.0 + font_size + i as f32 * line_height,
                font_size,
                Color::from_rgba8(240, 244, 250, 255),
                false,
                false,
            );
        }
    }

    /// Render a single player frame
    fn render_frame(&mut self, raid_frame: &RaidFrame) {
        let (x, y, w, h) = self.slot_bounds(raid_frame.slot);
        let corner_radius = (h * 0.1).clamp(2.0, 6.0);

        // Draw frame background/border based on interaction mode
        match self.interaction_mode {
            InteractionMode::Normal => {
                // Normal mode: FULLY INVISIBLE frames
                // Only effects are rendered (below), nothing else
            }
            InteractionMode::Move => {
                // Move mode: transparent frames with dashed border for alignment
                // (container background is set semi-transparent in set_interaction_mode)
                self.frame.stroke_rounded_rect_dashed(
                    x,
                    y,
                    w,
                    h,
                    corner_radius,
                    1.5, // stroke width
                    colors::raid_guide(),
                    6.0, // dash length
                    4.0, // gap length
                );
            }
            InteractionMode::Rearrange => {
                // Rearrange mode: nearly transparent frame backgrounds (90% transparent = 10% opacity)
                let bg = Color::from_rgba8(
                    self.config.frame_bg_color[0],
                    self.config.frame_bg_color[1],
                    self.config.frame_bg_color[2],
                    25, // ~10% opacity (255 * 0.1)
                );
                self.frame.fill_rounded_rect(x, y, w, h, corner_radius, bg);
            }
        }

        // In move mode: render a placeholder effect on ALL frames so user can see positioning
        if self.interaction_mode == InteractionMode::Move {
            self.render_placeholder_effect(x, y);
            return;
        }

        // Empty frames: nothing more to render (no effects, no role icons)
        if raid_frame.is_empty() {
            return;
        }

        // Effect indicators (TOP-LEFT, to match SWTOR's debuff placement)
        self.render_effects(raid_frame, x, y);

        // Role & class icons (BOTTOM-LEFT, anchored to frame bottom)
        let show_role = self.config.show_role_icons;
        let show_class = self.config.show_class_icons;
        if show_role || show_class {
            self.render_role_and_class_icons(
                raid_frame.role,
                raid_frame.class_icon.as_deref(),
                x,
                y,
                h,
                show_role,
                show_class,
            );
        }
    }

    /// Render role and/or class icons at the bottom-left of the frame.
    ///
    /// Icons are anchored to the frame bottom so they always render regardless
    /// of effect row size or frame spacing.
    ///
    /// Layout:
    /// - Role icon only: role glyph PNG at bottom-left
    /// - Class icon only: white class silhouette at bottom-left
    /// - Both: role glyph + class icon side by side (2px gap)
    /// - DPS has no role icon, so class icon shifts to the role icon position
    #[allow(clippy::too_many_arguments)]
    fn render_role_and_class_icons(
        &mut self,
        role: PlayerRole,
        class_icon_name: Option<&str>,
        x: f32,
        y: f32,
        h: f32,
        show_role: bool,
        show_class: bool,
    ) {
        let icon_size = (self.frame_height() * 0.3).clamp(10.0, 20.0);
        let icon_x = x + 3.0;
        // Anchor to frame bottom with small margin
        let icon_y = y + h - icon_size - 2.0;

        let mut cursor_x = icon_x;

        // Render role icon glyph (tank/healer only)
        let role_rendered = if show_role && role != PlayerRole::Dps {
            let role_icon_name = match role {
                PlayerRole::Tank => "icon_tank",
                PlayerRole::Healer => "icon_heal",
                PlayerRole::Dps => unreachable!(),
            };
            if let Some(icon) = crate::class_icons::get_role_icon(role_icon_name) {
                self.frame.draw_image(
                    &icon.rgba,
                    icon.width,
                    icon.height,
                    cursor_x,
                    icon_y,
                    icon_size,
                    icon_size,
                );
                cursor_x += icon_size + 2.0; // Advance past role icon + gap
                true
            } else {
                false
            }
        } else {
            false
        };

        // Render class icon (white silhouette with drop shadow) if enabled
        if show_class {
            if let Some(name) = class_icon_name {
                // Check width: don't overflow the frame
                if cursor_x + icon_size <= x + self.frame_width() - 2.0 {
                    if let Some(icon) = crate::class_icons::get_white_class_icon(name) {
                        self.frame.draw_image_with_shadow(
                            &icon.rgba,
                            icon.width,
                            icon.height,
                            cursor_x,
                            icon_y,
                            icon_size,
                            icon_size,
                        );
                    }
                }
            }
        }

        // Suppress unused variable warning when only class icons shown with no role icon
        let _ = role_rendered;
    }

    /// Render a single placeholder effect indicator in move mode
    /// Shows the user where effects will be positioned
    fn render_placeholder_effect(&mut self, x: f32, y: f32) {
        let effect_size = self.config.effect_size();
        let vertical_offset = self.config.effect_vertical_offset();
        let corner_radius = 2.0;

        // Position: same as first effect in render_effects
        let ex = x + 3.0;
        let ey = y + vertical_offset;

        // Semi-transparent background with dashed border to indicate placeholder
        self.frame.fill_rounded_rect(
            ex,
            ey,
            effect_size,
            effect_size,
            corner_radius,
            colors::effect_icon_bg(),
        );

        // Dashed border to indicate it's a placeholder
        self.frame.stroke_rounded_rect_dashed(
            ex,
            ey,
            effect_size,
            effect_size,
            corner_radius,
            1.0, // stroke width
            colors::effect_icon_border(),
            3.0, // dash length
            2.0, // gap length
        );
    }

    /// Render effect indicators on the LEFT side of the frame (matches SWTOR debuff placement)
    /// Effects with duration show a fill that depletes from bottom to top as time expires.
    /// When show_effect_icons is enabled, renders icons with wipedown effect instead of colored squares.
    /// Returns the effect row height for layout calculations
    fn render_effects(&mut self, raid_frame: &RaidFrame, x: f32, y: f32) -> f32 {
        let max_effects = self.config.max_effects_per_frame as usize;
        let effect_size = self.config.effect_size();
        let vertical_offset = self.config.effect_vertical_offset();
        let fill_opacity = self.config.effect_fill_opacity;
        let spacing = effect_size * 0.2;
        let corner_radius = 2.0;
        let border_width = 1.0;

        for (i, effect) in raid_frame.effects.iter().take(max_effects).enumerate() {
            // LEFT side positioning, growing rightward
            let ex = x + 3.0 + (i as f32 * (effect_size + spacing));
            let ey = y + vertical_offset;

            // Draw icon or colored square (overlay toggle AND per-effect flag)
            let has_icon = if self.config.show_effect_icons && effect.show_icon {
                if let Some(ref icon_arc) = effect.icon {
                    let (img_w, img_h, ref rgba) = **icon_arc;
                    self.frame
                        .draw_image(rgba, img_w, img_h, ex, ey, effect_size, effect_size);
                    true
                } else {
                    false
                }
            } else {
                false
            };

            if !has_icon {
                // Dark background (always visible even when fill is empty)
                self.frame.fill_rounded_rect(
                    ex,
                    ey,
                    effect_size,
                    effect_size,
                    corner_radius,
                    colors::effect_bar_bg(),
                );

                // Calculate fill based on remaining duration
                let fill_percent = effect.fill_percent();

                if fill_percent > 0.0 {
                    // Fill depletes from bottom to top (remaining time shrinks upward)
                    // Use explicit bottom coordinate to avoid floating-point rounding issues
                    let max_fill_height = effect_size - border_width * 2.0;
                    let fill_bottom = ey + effect_size - border_width;
                    let fill_height = (max_fill_height * fill_percent).round();
                    let fill_y = fill_bottom - fill_height;

                    // Combine per-effect alpha (from color) with config opacity
                    // This allows per-effect control while config acts as global multiplier
                    let effect_alpha = (effect.color.alpha() * 255.0) as u16;
                    let combined_alpha = ((effect_alpha * fill_opacity as u16) / 255).min(255) as u8;

                    let fill_color = Color::from_rgba8(
                        (effect.color.red() * 255.0) as u8,
                        (effect.color.green() * 255.0) as u8,
                        (effect.color.blue() * 255.0) as u8,
                        combined_alpha,
                    );

                    // Inner fill area (inset by border width)
                    // Use rounded coordinates to avoid sub-pixel rendering artifacts
                    self.frame.fill_rect(
                        (ex + border_width).round(),
                        fill_y.round(),
                        max_fill_height.round(),
                        fill_height,
                        fill_color,
                    );
                }
            }

            // Wipedown overlay (works for both icon and colored square)
            // Shows remaining duration as darkened area from top
            let progress = effect.fill_percent();
            let overlay_height = effect_size * (1.0 - progress);
            if overlay_height > 1.0 {
                self.frame.fill_rect(
                    ex,
                    ey,
                    effect_size,
                    overlay_height,
                    Color::from_rgba8(0, 0, 0, 140),
                );
            }

            // Thin border outline for visibility
            self.frame.stroke_rounded_rect(
                ex,
                ey,
                effect_size,
                effect_size,
                corner_radius,
                1.0,
                colors::effect_bar_border(),
            );

            // Stack count if applicable (centered in the effect square)
            if effect.charges > 1 {
                let count = format!("{}", effect.charges);
                let stack_font = (effect_size * 0.7).max(8.0);

                // Measure text for proper centering (bold for readability)
                let (text_w, _) =
                    self.frame.measure_text_styled(&count, stack_font, true, false);

                // Center horizontally, position in lower portion of square
                let text_x = ex + (effect_size - text_w) / 2.0;
                let text_y = ey + effect_size * 0.78;

                // Draw shadow (subtle drop shadow for readability)
                self.frame.draw_text_styled(
                    &count,
                    text_x + 1.0,
                    text_y + 1.0,
                    stack_font,
                    colors::text_shadow(),
                    true,
                    false,
                );

                // Draw text on top
                self.frame.draw_text_styled(
                    &count, text_x, text_y, stack_font, colors::white(), true, false,
                );
            }
        }

        // Return effect row height (used by placeholder rendering in move mode)
        effect_size + vertical_offset.max(3.0)
    }

    /// Render the clickable overlay for rearrange mode
    fn render_rearrange_overlay(&mut self, raid_frame: &RaidFrame) {
        let (x, y, w, h) = self.slot_bounds(raid_frame.slot);
        let is_selected = self.swap_state.selected_slot == Some(raid_frame.slot);

        // Semi-transparent clickable overlay
        let overlay_color = if is_selected {
            Color::from_rgba8(
                self.config.selection_color[0],
                self.config.selection_color[1],
                self.config.selection_color[2],
                (self.config.selection_color[3] as f32 * 0.7) as u8,
            )
        } else {
            colors::raid_empty_slot()
        };
        let corner_radius = (h * 0.1).clamp(2.0, 6.0);
        self.frame
            .fill_rounded_rect(x, y, w, h, corner_radius, overlay_color);

        // Border
        let border_color = if is_selected {
            colors::raid_slot_text()
        } else {
            colors::text_muted()
        };
        self.frame.stroke_rounded_rect(
            x + 1.0,
            y + 1.0,
            w - 2.0,
            h - 2.0,
            corner_radius - 1.0,
            2.0,
            border_color,
        );

        // Player name bottom-right (or "Empty")
        let font_size = self.font_size() * 1.1;
        let text = if raid_frame.is_empty() {
            "Empty".to_string()
        } else {
            truncate_name(&raid_frame.name, 12).to_string()
        };

        let (text_w, _text_h) = self.frame.measure_text(&text, font_size);
        let text_x = x + w - text_w - 4.0;
        // Note: draw_text y is baseline
        let text_y = y + h - 4.0;

        // Orange until a name is tied to a log player, green once it is.
        let text_color = if raid_frame.is_empty() {
            colors::raid_slot_number()
        } else if raid_frame.player_id.is_some() {
            colors::raid_name_confirmed()
        } else {
            colors::raid_name_provisional()
        };
        self.frame
            .draw_text_glowed(&text, text_x, text_y, font_size, text_color);

        //  Add an indicator that is different from colour, as that might not
        // be as intuitive: ✓ once the name is tied to a log player, ? until.
        if !raid_frame.is_empty() {
            let (label, label_color) = if raid_frame.player_id.is_some() {
                ("✓", colors::raid_name_confirmed())
            } else {
                ("?", colors::raid_name_provisional())
            };
            let label_size = font_size * 0.8;
            let (label_w, _) = self.frame.measure_text(label, label_size);
            self.frame.draw_text_glowed(
                label,
                x + w - label_w - 4.0,
                text_y - font_size,
                label_size,
                label_color,
            );
        }

        // Clear button (×) for ALL occupied frames (including self)
        if !raid_frame.is_empty() {
            let btn_size = (h * 0.35).clamp(12.0, 18.0);
            let btn_x = x + w - btn_size - 3.0;
            let btn_y = y + 3.0;

            self.frame.fill_rounded_rect(
                btn_x,
                btn_y,
                btn_size,
                btn_size,
                2.0,
                colors::raid_clear_button(),
            );
            // Note: draw_text y is baseline
            let btn_font = btn_size * 0.7;
            let text_x = btn_x + btn_size * 0.3;
            let text_y = btn_y + btn_size * 0.75; // Baseline near bottom
            self.frame
                .draw_text_glowed("x", text_x, text_y, btn_font, colors::white());
        }
    }

    /// Render overflow indicator
    fn render_overflow_indicator(&mut self) {
        if self.overflow_count == 0 {
            return;
        }

        let text = format!("+{}", self.overflow_count);
        let x = self.frame.width() as f32 - 24.0;
        let y = self.frame.height() as f32 - 16.0;

        self.frame
            .draw_text_glowed(&text, x, y, 10.0, colors::raid_overflow());
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Event Handling
    // ─────────────────────────────────────────────────────────────────────────

    /// Handle a click in rearrange mode
    /// Instead of modifying local state, we queue actions for the registry.
    fn handle_rearrange_click(&mut self, px: f32, py: f32) {
        // Check clear buttons first - queue ClearSlot action
        for i in 0..self.frames.len() {
            let frame = &self.frames[i];
            // All non-empty frames can be cleared (including self)
            if !frame.is_empty() && self.hit_test_clear_button(frame.slot, px, py) {
                self.pending_registry_actions
                    .push(RaidRegistryAction::ClearSlot(frame.slot));
                self.needs_render = true;
                return;
            }
        }

        // Then check slot selection for swapping
        if let Some(slot) = self.hit_test(px, py) {
            if let Some((a, b)) = self.swap_state.on_click(slot) {
                // Apply swap locally for immediate visual feedback
                self.swap_frames(a, b);
                // Queue swap action so the registry stays in sync
                self.pending_registry_actions
                    .push(RaidRegistryAction::SwapSlots(a, b));
            } else {
                // Selection changed (first click or deselect same slot)
                self.needs_render = true;
            }
        } else {
            // Clicked outside any slot - deselect
            self.swap_state.cancel();
            self.needs_render = true;
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Overlay Trait Implementation
// ─────────────────────────────────────────────────────────────────────────────

impl Overlay for RaidOverlay {
    fn update_data(&mut self, data: OverlayData) -> bool {
        if let OverlayData::Raid(raid_data) = data {
            // Skip render if both old and new have no players with effects
            let old_has_effects = self.frames.iter().any(|f| !f.effects.is_empty());
            let new_has_effects = raid_data.frames.iter().any(|f| !f.effects.is_empty());
            let skip_render =
                !old_has_effects && !new_has_effects && self.frames.len() == raid_data.frames.len();
            self.set_frames(raid_data.frames);
            !skip_render
        } else {
            false
        }
    }

    fn update_config(&mut self, config: OverlayConfigUpdate) {
        if let OverlayConfigUpdate::Raid(raid_config, _alpha, european) = config {
            self.config = raid_config;
            // background_alpha is owned by the interaction mode (set_interaction_mode),
            // not by config — Normal/Rearrange = 0, Move = 120 (hardcoded).
            self.european_number_format = european;
            self.needs_render = true;
        }
    }

    fn render(&mut self) {
        RaidOverlay::render(self);
    }

    fn poll_events(&mut self) -> bool {
        if !self.frame.poll_events() {
            return false;
        }

        let detection_result = self
            .detection_result_rx
            .as_ref()
            .and_then(|rx| match rx.try_recv() {
                Ok(message) => Some(message),
                Err(std::sync::mpsc::TryRecvError::Empty) => None,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    Some("Detection stopped: assign names manually".into())
                }
            });
        if let Some(message) = detection_result {
            self.detection_result_rx = None;
            self.set_detection_message(message);
        } else if self.detection_result_rx.is_none()
            && self
                .detection_message
                .as_ref()
                .is_some_and(|(_, shown_at)| {
                    shown_at.elapsed()
                        >= std::time::Duration::from_secs(DETECTION_MESSAGE_SECS)
                })
        {
            self.detection_message = None;
            self.needs_render = true;
        }

        // Mark dirty if window was resized/moved (affects layout calculations)
        if self.frame.take_position_dirty() {
            self.needs_render = true;
        }
        // SetSize is not reported as a position change on every backend.
        self.refresh_interactive_region();

        if let Some((px, py)) = self.frame.take_pending_click() {
            if self.interaction_mode.shows_detect_button() && self.hit_test_detect_button(px, py)
            {
                self.emit_detect_action();
            } else if self.interaction_mode == InteractionMode::Rearrange {
                self.handle_rearrange_click(px, py);
            }
        }

        true
    }

    fn frame(&self) -> &OverlayFrame {
        &self.frame
    }

    fn frame_mut(&mut self) -> &mut OverlayFrame {
        &mut self.frame
    }

    fn set_move_mode(&mut self, enabled: bool) {
        let new_mode = if enabled {
            InteractionMode::Move
        } else {
            InteractionMode::Normal
        };
        self.set_interaction_mode(new_mode);
    }

    fn set_rearrange_mode(&mut self, enabled: bool) {
        let new_mode = if enabled {
            InteractionMode::Rearrange
        } else {
            InteractionMode::Normal
        };
        self.set_interaction_mode(new_mode);
    }

    fn request_raid_detection(&mut self) {
        self.emit_detect_action();
    }

    fn take_pending_registry_actions(&mut self) -> Vec<RaidRegistryAction> {
        std::mem::take(&mut self.pending_registry_actions)
    }

    fn needs_render(&self) -> bool {
        self.needs_render
    }
}

/// Split a message after each comma, keeping the comma.
///
/// Whitespace is dropped.
fn comma_clauses(text: &str) -> Vec<String> {
    let mut clauses = Vec::new();
    let mut start = 0;

    for (index, c) in text.char_indices() {
        if c == ',' {
            let end = index + c.len_utf8();
            let clause = text[start..end].trim();
            if !clause.is_empty() {
                clauses.push(clause.to_string());
            }
            start = end;
        }
    }

    let tail = text[start..].trim();
    if !tail.is_empty() {
        clauses.push(tail.to_string());
    }
    clauses
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provisional_name_is_not_an_empty_frame() {
        let mut frame = RaidFrame::empty(2);
        assert!(frame.is_empty());

        frame.name = "TEST PLAYER".into();
        assert!(frame.player_id.is_none());
        assert!(!frame.is_empty());
    }

    #[test]
    fn detect_button_only_shows_in_editing_modes() {
        assert!(!InteractionMode::Normal.shows_detect_button());
        assert!(InteractionMode::Move.shows_detect_button());
        assert!(InteractionMode::Rearrange.shows_detect_button());
    }
}
