//! Raid Frame Overlay
//!
//! Displays a grid of player frames showing health, effects, and role icons.
//! Supports click-to-swap rearrangement of frames.

use std::time::Instant;
use tiny_skia::Color;

use super::{Overlay, OverlayConfigUpdate, OverlayData, RaidRegistryAction};
use crate::frame::OverlayFrame;
use crate::platform::{OverlayConfig, PlatformError};
use crate::widgets::colors;
use crate::utils::truncate_name;

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
                "immortal" | "darkness" | "shield specialist" | "shield tech"
                    | "defense" | "kinetic combat"
            )
        {
            PlayerRole::Tank
        }
        // Healer disciplines
        else if lower.contains("heal")
            || matches!(
                lower.as_str(),
                "corruption" | "medicine" | "bodyguard" | "combat medic"
                    | "seer" | "sawbones"
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
            effects: Vec::new(),
            is_self: false,
        }
    }

    /// Check if the frame is empty (no player assigned)
    pub fn is_empty(&self) -> bool {
        self.player_id.is_none()
    }

    /// Clear the frame (remove player)
    pub fn clear(&mut self) {
        self.player_id = None;
        self.name.clear();
        self.hp_percent = 0.0;
        self.role = PlayerRole::Dps;
        self.effects.clear();
        self.is_self = false;
    }

    /// Apply or refresh an effect
    pub fn apply_effect(&mut self, effect: RaidEffect, max_effects: usize) {
        // Check if effect already exists
        if let Some(existing) = self.effects.iter_mut().find(|e| e.effect_id == effect.effect_id) {
            // Refresh: update expiry and take higher stack count
            existing.expires_at = effect.expires_at;
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
    Normal,    // click_through = true, clicks pass through
    Move,      // click_through = false, drag = move window
    Rearrange, // click_through = false, click = swap slots
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
            columns: settings.grid_columns.clamp(1, 4),
            rows: settings.grid_rows.clamp(1, 8),
        }
    }

    /// Create a layout for the given player count
    pub fn for_player_count(count: u8) -> Self {
        match count {
            0..=4 => Self { columns: 1, rows: 4 },
            5..=8 => Self { columns: 2, rows: 4 },
            _ => Self { columns: 4, rows: 4 },
        }
    }

    /// Total number of slots
    pub fn capacity(&self) -> u8 {
        self.columns * self.rows
    }
}

impl Default for RaidGridLayout {
    fn default() -> Self {
        Self { columns: 2, rows: 4 }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Raid Overlay Config
// ─────────────────────────────────────────────────────────────────────────────

/// Effect size bounds (in pixels before scaling)
pub const EFFECT_SIZE_MIN: f32 = 8.0;
pub const EFFECT_SIZE_MAX: f32 = 24.0;
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
}

impl Default for RaidOverlayConfig {
    fn default() -> Self {
        Self {
            show_role_icons: true,
            max_effects_per_frame: 4,
            frame_bg_color: [40, 40, 40, 200],
            selection_color: [80, 120, 180, 220],
            effect_size: EFFECT_SIZE_DEFAULT,
            effect_vertical_offset: EFFECT_OFFSET_DEFAULT,
            effect_fill_opacity: 255, // Fully opaque when no icons
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
        self.effect_vertical_offset.clamp(EFFECT_OFFSET_MIN, EFFECT_OFFSET_MAX)
    }
}

impl From<baras_core::context::RaidOverlaySettings> for RaidOverlayConfig {
    fn from(settings: baras_core::context::RaidOverlaySettings) -> Self {
        Self {
            show_role_icons: settings.show_role_icons,
            max_effects_per_frame: settings.max_effects_per_frame,
            frame_bg_color: settings.frame_bg_color,
            selection_color: [80, 120, 180, 220], // Keep hardcoded for now
            effect_size: settings.effect_size,
            effect_vertical_offset: settings.effect_vertical_offset,
            effect_fill_opacity: settings.effect_fill_opacity,
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

        Ok(Self {
            frame,
            frames,
            layout,
            interaction_mode: InteractionMode::Normal,
            swap_state: SwapState::default(),
            config,
            overflow_count: 0,
            needs_render: true, // Initial render needed
            last_render: Instant::now() - RENDER_INTERVAL, // Allow immediate first render
            pending_registry_actions: Vec::new(),
        })
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Scaling helpers
    // ─────────────────────────────────────────────────────────────────────────

    fn padding(&self) -> f32 {
        self.frame.scaled(BASE_PADDING)
    }

    fn gap(&self) -> f32 {
        self.frame.scaled(BASE_GAP)
    }

    /// Calculate frame width based on container size and column count
    fn frame_width(&self) -> f32 {
        let container_width = self.frame.width() as f32;
        let padding = self.padding();
        let gap = self.gap();
        let cols = self.layout.columns as f32;

        // Available width = container - 2*padding - (cols-1)*gap
        // Frame width = available / cols
        let available = container_width - (2.0 * padding) - ((cols - 1.0) * gap);
        (available / cols).max(20.0) // Minimum 20px width
    }

    /// Calculate frame height based on container size and row count
    fn frame_height(&self) -> f32 {
        let container_height = self.frame.height() as f32;
        let padding = self.padding();
        let gap = self.gap();
        let rows = self.layout.rows as f32;

        // Available height = container - 2*padding - (rows-1)*gap
        // Frame height = available / rows
        let available = container_height - (2.0 * padding) - ((rows - 1.0) * gap);
        (available / rows).max(20.0) // Minimum 20px height
    }

    fn font_size(&self) -> f32 {
        // Scale font relative to frame height for readability
        (self.frame_height() * 0.28).clamp(8.0, 16.0)
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Layout & Hit Testing
    // ─────────────────────────────────────────────────────────────────────────

    /// Calculate the pixel bounds for a given slot
    fn slot_bounds(&self, slot: u8) -> (f32, f32, f32, f32) {
        let col = (slot % self.layout.columns) as f32;
        let row = (slot / self.layout.columns) as f32;

        let x = self.padding() + col * (self.frame_width() + self.gap());
        let y = self.padding() + row * (self.frame_height() + self.gap());

        (x, y, self.frame_width(), self.frame_height())
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

        // Render all frames
        for i in 0..self.frames.len() {
            let frame_data = self.frames[i].clone();
            self.render_frame(&frame_data);
        }

        // Overlay the rearrange UI if in that mode
        if self.interaction_mode == InteractionMode::Rearrange {
            for i in 0..self.frames.len() {
                let frame_data = self.frames[i].clone();
                self.render_rearrange_overlay(&frame_data);
            }
        }

        // Overflow indicator
        self.render_overflow_indicator();

        self.frame.end_frame();
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
                    x, y, w, h,
                    corner_radius,
                    1.5,        // stroke width
                    colors::raid_guide(),
                    6.0,        // dash length
                    4.0,        // gap length
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
        let effect_size = self.render_effects(raid_frame, x, y);

        // Role icon (BOTTOM-LEFT, below effects row)
        if self.config.show_role_icons {
            self.render_role_icon(raid_frame.role, x, y, h, effect_size);
        }

        // NOTE: Player names are NOT shown in normal/move mode
        // They are redundant since the game already displays them
        // Names only appear in rearrange mode for identification during swap
    }

    /// Get color based on a percentage value (0.0 - 1.0)
    /// Useful for effect duration indicators, cooldowns, etc.
    #[allow(dead_code)]
    fn percent_color(percent: f32) -> Color {
        if percent > 0.5 {
            colors::health_high()
        } else if percent > 0.25 {
            colors::health_medium()
        } else {
            colors::health_low()
        }
    }

    /// Render the role icon at bottom-left, below the effects row
    fn render_role_icon(&mut self, role: PlayerRole, x: f32, y: f32, h: f32, effect_size: f32) {
        let icon_size = (self.frame_height() * 0.3).clamp(10.0, 16.0);
        let icon_x = x + 3.0;
        // Position below effects row: y + effect_row_height + small gap
        let icon_y = y + effect_size + 6.0;

        // Don't render if it would overflow the frame
        if icon_y + icon_size > y + h - 2.0 {
            return;
        }

        match role {
            PlayerRole::Tank => {
                // Blue shield
                self.frame.fill_rounded_rect(icon_x, icon_y, icon_size, icon_size, 2.0, colors::role_tank());
                // "T" label centered in the icon
                // Note: draw_text y is baseline, so add font_size to push text down into box
                let icon_font = icon_size * 0.7;
                let text_x = icon_x + icon_size * 0.25;
                let text_y = icon_y + icon_size * 0.75; // Baseline near bottom of icon
                self.frame.draw_text("T", text_x, text_y, icon_font, colors::white());
            }
            PlayerRole::Healer => {
                // Green cross
                let bar_w = icon_size * 0.35;
                // Vertical bar
                self.frame.fill_rect(
                    icon_x + (icon_size - bar_w) / 2.0,
                    icon_y,
                    bar_w,
                    icon_size,
                    colors::role_healer(),
                );
                // Horizontal bar
                self.frame.fill_rect(
                    icon_x,
                    icon_y + (icon_size - bar_w) / 2.0,
                    icon_size,
                    bar_w,
                    colors::role_healer(),
                );
            }
            PlayerRole::Dps => {
                // No icon for DPS
            }
        }
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
        self.frame.fill_rounded_rect(ex, ey, effect_size, effect_size, corner_radius, colors::effect_icon_bg());

        // Dashed border to indicate it's a placeholder
        self.frame.stroke_rounded_rect_dashed(
            ex, ey, effect_size, effect_size,
            corner_radius,
            1.0,        // stroke width
            colors::effect_icon_border(),
            3.0,        // dash length
            2.0,        // gap length
        );
    }

    /// Render effect indicators on the LEFT side of the frame (matches SWTOR debuff placement)
    /// Effects with duration show a fill that depletes from bottom to top as time expires.
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

            // Dark background (always visible even when fill is empty)
            self.frame.fill_rounded_rect(ex, ey, effect_size, effect_size, corner_radius, colors::effect_bar_bg());

            // Calculate fill based on remaining duration
            let fill_percent = effect.fill_percent();

            if fill_percent > 0.0 {
                // Fill depletes from bottom to top (remaining time shrinks upward)
                let fill_height = (effect_size - border_width * 2.0) * fill_percent;
                let fill_y = ey + effect_size - border_width - fill_height;

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
                self.frame.fill_rect(
                    ex + border_width,
                    fill_y,
                    effect_size - border_width * 2.0,
                    fill_height,
                    fill_color,
                );
            }

            // Thin border outline for visibility
            self.frame.stroke_rounded_rect(
                ex, ey, effect_size, effect_size,
                corner_radius, 1.0, colors::effect_bar_border(),
            );

            // Stack count if applicable (centered in the effect square)
            if effect.charges > 1 {
                let count = format!("{}", effect.charges);
                let stack_font = (effect_size * 0.55).max(8.0);

                // Measure text for proper centering
                let (text_w, _) = self.frame.measure_text(&count, stack_font);

                // Center horizontally, position in lower portion of square
                let text_x = ex + (effect_size - text_w) / 2.0;
                let text_y = ey + effect_size * 0.78;

                // Draw shadow (subtle drop shadow for readability)
                self.frame.draw_text(&count, text_x + 1.0, text_y + 1.0, stack_font, colors::text_shadow());

                // Draw text on top
                self.frame.draw_text(&count, text_x, text_y, stack_font, colors::white());
            }
        }

        // Return effect row height for role icon positioning
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
                self.config.selection_color[3],
            )
        } else {
            colors::raid_empty_slot()
        };
        let corner_radius = (h * 0.1).clamp(2.0, 6.0);
        self.frame.fill_rounded_rect(x, y, w, h, corner_radius, overlay_color);

        // Border
        let border_color = if is_selected {
            colors::raid_slot_text()
        } else {
            colors::text_muted()
        };
        self.frame.stroke_rounded_rect(x + 1.0, y + 1.0, w - 2.0, h - 2.0, corner_radius - 1.0, 2.0, border_color);

        // Player name centered (or "Empty")
        let font_size = self.font_size() * 1.1;
        let text = if raid_frame.is_empty() {
            "Empty".to_string()
        } else {
            truncate_name(&raid_frame.name, 12).to_string()
        };

        let (text_w, _text_h) = self.frame.measure_text(&text, font_size);
        let text_x = x + (w - text_w) / 2.0;
        // Note: draw_text y is baseline, so add font_size * 0.7 to center visually
        // (baseline is roughly 70-80% down from top of capital letters)
        let text_y = y + (h / 2.0) + (font_size * 0.35);

        let text_color = if raid_frame.is_empty() {
            colors::raid_slot_number()
        } else {
            colors::white()
        };
        self.frame.draw_text(&text, text_x, text_y, font_size, text_color);

        // Clear button (×) for ALL occupied frames (including self)
        if !raid_frame.is_empty() {
            let btn_size = (h * 0.35).clamp(12.0, 18.0);
            let btn_x = x + w - btn_size - 3.0;
            let btn_y = y + 3.0;

            self.frame.fill_rounded_rect(
                btn_x, btn_y, btn_size, btn_size, 2.0,
                colors::raid_clear_button(),
            );
            // Note: draw_text y is baseline
            let btn_font = btn_size * 0.7;
            let text_x = btn_x + btn_size * 0.3;
            let text_y = btn_y + btn_size * 0.75; // Baseline near bottom
            self.frame.draw_text("x", text_x, text_y, btn_font, colors::white());
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

        self.frame.draw_text(&text, x, y, 10.0, colors::raid_overflow());
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
                eprintln!("[RAID-OVERLAY] Queuing ClearSlot({})", frame.slot);
                self.pending_registry_actions.push(RaidRegistryAction::ClearSlot(frame.slot));
                self.needs_render = true;
                return;
            }
        }

        // Then check slot selection for swapping
        if let Some(slot) = self.hit_test(px, py) {
            if let Some((a, b)) = self.swap_state.on_click(slot) {
                // Queue swap action - registry will update, then data will flow back
                eprintln!("[RAID-OVERLAY] Queuing SwapSlots({}, {})", a, b);
                self.pending_registry_actions.push(RaidRegistryAction::SwapSlots(a, b));
                self.needs_render = true;
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
            let skip_render = !old_has_effects && !new_has_effects && self.frames.len() == raid_data.frames.len();
            self.set_frames(raid_data.frames);
            !skip_render
        } else {
            false
        }
    }

    fn update_config(&mut self, config: OverlayConfigUpdate) {
        if let OverlayConfigUpdate::Raid(raid_config, alpha) = config {
            self.config = raid_config;
            self.frame.set_background_alpha(alpha);
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

        // Mark dirty if window was resized/moved (affects layout calculations)
        if self.frame.take_position_dirty() {
            self.needs_render = true;
        }

        // Handle clicks in rearrange mode (platform reports clicks when drag is disabled)
        if self.interaction_mode == InteractionMode::Rearrange
            && let Some((px, py)) = self.frame.take_pending_click() {
                self.handle_rearrange_click(px, py);
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

    fn take_pending_registry_actions(&mut self) -> Vec<RaidRegistryAction> {
        std::mem::take(&mut self.pending_registry_actions)
    }
}
