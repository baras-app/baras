//! Enemy HP Frames Overlay (PvP)
//!
//! An enemy-team mirror of the SWTOR operations frames: one compact cell per
//! enemy player showing HP (total + %), name + discipline, an HP bar colored
//! by class, and their current target. Frames keep stable first-seen slots.
//!
//! Extensibility: `EnemyFrame` carries a `guarded` flag (blue accent) and is
//! the intended home for future per-frame highlights (debuff tracking, etc.).

use baras_types::{formatting, ClassColorConfig};

use super::{Overlay, OverlayConfigUpdate, OverlayData};
use crate::frame::OverlayFrame;
use crate::platform::{OverlayConfig, PlatformError};
use crate::utils::color_from_rgba;
use crate::widgets::colors;
use crate::widgets::ProgressBar;

// ─────────────────────────────────────────────────────────────────────────────
// Configuration & Data
// ─────────────────────────────────────────────────────────────────────────────

/// Runtime configuration for the enemy frames overlay
#[derive(Debug, Clone)]
pub struct EnemyFramesConfig {
    /// Show each enemy's current target inside the frame
    pub show_target: bool,
    /// Font/element scale multiplier (0.5 - 2.0)
    pub scale: f32,
    /// Per-archetype bar colors (shared with metric overlays)
    pub class_colors: ClassColorConfig,
}

impl Default for EnemyFramesConfig {
    fn default() -> Self {
        Self {
            show_target: true,
            scale: 1.0,
            class_colors: ClassColorConfig::default(),
        }
    }
}

/// One enemy player's frame state
#[derive(Debug, Clone, PartialEq)]
pub struct EnemyFrame {
    pub entity_id: i64,
    pub name: String,
    /// Class name for HP bar color (e.g., "Sorcerer"); None until detected
    pub class_name: Option<String>,
    /// Discipline icon name (e.g., "lightning.png"); None until detected
    pub discipline_icon: Option<String>,
    pub current_hp: i64,
    pub max_hp: i64,
    /// Resolved name of this enemy's current target
    pub target_name: Option<String>,
    /// True when this enemy is targeting the local player
    pub targeting_you: bool,
    pub is_dead: bool,
    /// Blue accent border (guard highlight — data wiring lands later)
    pub guarded: bool,
    /// Seconds since this enemy last appeared in the log, once that is long
    /// enough to suggest they left; the frame dims and shows the count.
    pub stale_secs: Option<u32>,
}

/// Data update for the enemy frames overlay
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EnemyFramesData {
    pub frames: Vec<EnemyFrame>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Layout Constants
// ─────────────────────────────────────────────────────────────────────────────

/// Base dimensions for scaling calculations
const BASE_WIDTH: f32 = 320.0;
const BASE_HEIGHT: f32 = 240.0;
const BASE_PADDING: f32 = 4.0;
const BASE_GAP: f32 = 3.0;

// ─────────────────────────────────────────────────────────────────────────────
// Overlay Implementation
// ─────────────────────────────────────────────────────────────────────────────

pub struct EnemyFramesOverlay {
    frame: OverlayFrame,
    config: EnemyFramesConfig,
    data: EnemyFramesData,
    european_number_format: bool,
}

impl EnemyFramesOverlay {
    pub fn new(
        window_config: OverlayConfig,
        config: EnemyFramesConfig,
        background_alpha: u8,
    ) -> Result<Self, PlatformError> {
        let mut frame = OverlayFrame::new(window_config, BASE_WIDTH, BASE_HEIGHT)?;
        frame.set_background_alpha(background_alpha);
        frame.set_label("Enemy Frames");

        Ok(Self {
            frame,
            config,
            data: EnemyFramesData::default(),
            european_number_format: false,
        })
    }

    pub fn set_config(&mut self, config: EnemyFramesConfig) {
        self.config = config;
    }

    pub fn set_background_alpha(&mut self, alpha: u8) {
        self.frame.set_background_alpha(alpha);
    }

    pub fn render(&mut self) {
        let width = self.frame.width() as f32;
        let height = self.frame.height() as f32;
        let padding = self.frame.scaled(BASE_PADDING);
        let gap = self.frame.scaled(BASE_GAP);

        // Move mode with no live data: show placeholder frames so the user can
        // position/size the overlay outside a match
        let placeholders;
        let frames: &[EnemyFrame] = if self.data.frames.is_empty() && self.frame.is_in_move_mode() {
            placeholders = placeholder_frames();
            &placeholders
        } else {
            &self.data.frames
        };

        // Fixed 4-row, ops-frame layout: 4 enemies (arena) fill one column,
        // 8 (warzone) two, filling column-major. Cells are always sized for
        // the full 4x2 warzone grid — a partially-seen team leaves empty
        // window space rather than stretching frames to fill it.
        let n = frames.len();
        if n == 0 {
            self.frame.begin_frame_with_content_height(0.0);
            self.frame.end_frame();
            return;
        }
        const GRID_ROWS: usize = 4;

        self.frame.begin_frame();

        let cell_w = ((width - padding * 2.0 - gap) / 2.0).max(20.0);
        let cell_h = ((height - padding * 2.0 - (GRID_ROWS - 1) as f32 * gap)
            / GRID_ROWS as f32)
            .max(20.0);

        // Split borrows: frames may borrow self.data, cells draw via self.frame
        let (frame, config, eu) = (&mut self.frame, &self.config, self.european_number_format);
        for (i, ef) in frames.iter().enumerate() {
            // Column-major fill: slots 0-3 down the first column, 4-7 the second
            let col = i / GRID_ROWS;
            let row = i % GRID_ROWS;
            let x = padding + col as f32 * (cell_w + gap);
            let y = padding + row as f32 * (cell_h + gap);
            render_cell(frame, config, eu, ef, x, y, cell_w, cell_h);
        }

        self.frame.end_frame();
    }
}

#[allow(clippy::too_many_arguments)]
fn render_cell(
    frame: &mut OverlayFrame,
    config: &EnemyFramesConfig,
    european_number_format: bool,
    ef: &EnemyFrame,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
) {
    let scale = config.scale.clamp(0.5, 2.0);
    let radius = 3.0 * frame.scale_factor();
    let inset = frame.scaled(3.0);

    // Row proportions: name / HP bar / target (target row collapses when hidden)
    // Bar is the tallest row — it carries the HP value + percent text
    let (name_h, bar_h) = if config.show_target {
        (h * 0.30, h * 0.40)
    } else {
        (h * 0.45, h * 0.50)
    };
    let name_font = (name_h * 0.78).min(frame.scaled(14.0 * scale));
    let bar_font = (bar_h * 0.62).min(frame.scaled(13.0 * scale));
    let target_font = name_font * 0.85;

    // Cell background
    frame.fill_rounded_rect(x, y, w, h, radius, colors::enemy_cell_bg());

    // Guard highlight: blue border accent (data wiring lands later)
    if ef.guarded {
        frame.stroke_rounded_rect(x, y, w, h, radius, 1.5, colors::guard_blue());
    }

    // ─── Line 1: discipline icon + name ───────────────────────────────────
    let mut cursor_x = x + inset;
    let icon_size = name_h - 2.0;
    if let Some(icon_name) = ef.discipline_icon.as_deref()
        && let Some(icon) = crate::class_icons::get_discipline_icon(icon_name)
    {
        frame.draw_image(
            &icon.rgba,
            icon.width,
            icon.height,
            cursor_x,
            y + 1.0,
            icon_size,
            icon_size,
        );
        cursor_x += icon_size + inset * 0.7;
    }

    let name_baseline = y + name_h * 0.82;
    let name_color = if ef.is_dead {
        colors::label_dim()
    } else {
        colors::white()
    };
    // Stale "Ns" label is right-aligned on the name line; the name gets the rest
    let stale = ef.stale_secs.map(|secs| {
        let text = format!("{secs}s");
        let w = frame.measure_text(&text, name_font).0;
        (text, w)
    });
    let stale_w = stale.as_ref().map_or(0.0, |(_, w)| w + inset);
    let name_max = (x + w - inset - stale_w - cursor_x).max(0.0);
    let name = frame.truncate_to_width(&ef.name, name_max, name_font, false);
    frame.draw_text_styled(&name, cursor_x, name_baseline, name_font, name_color, false, false);

    if let Some((text, text_w)) = stale {
        frame.draw_text_styled(
            &text,
            x + w - inset - text_w,
            name_baseline,
            name_font,
            colors::label_dim(),
            false,
            false,
        );
    }

    // ─── Line 2: HP bar (value left, percent right — boss HP overlay style) ──
    let bar_y = y + name_h;
    let bar_w = w - inset * 2.0;
    let bar_x = x + inset;
    let bar_radius = 2.0 * frame.scale_factor();

    let hp_frac = if ef.is_dead || ef.max_hp <= 0 {
        0.0
    } else {
        (ef.current_hp as f32 / ef.max_hp as f32).clamp(0.0, 1.0)
    };
    let fill_color = ef
        .class_name
        .as_deref()
        .and_then(|n| config.class_colors.for_class_name(n))
        .map(color_from_rgba)
        .unwrap_or_else(colors::enemy_unknown_class);

    let (hp_text, pct_text) = if ef.is_dead {
        ("DEAD".to_string(), String::new())
    } else if ef.max_hp > 0 {
        (
            formatting::format_compact(ef.current_hp, european_number_format),
            formatting::format_pct(hp_frac as f64 * 100.0, european_number_format),
        )
    } else {
        (String::new(), String::new())
    };

    let mut bar = ProgressBar::new(&hp_text, hp_frac)
        .with_fill_color(fill_color)
        .with_bg_color(colors::dps_bar_bg())
        .with_text_color(colors::white())
        .with_text_glow();
    if !pct_text.is_empty() {
        bar = bar.with_right_text(pct_text);
    }
    bar.render(frame, bar_x, bar_y, bar_w, bar_h, bar_font, bar_radius);

    // ─── Line 3: current target ───────────────────────────────────────────
    if config.show_target {
        let target_baseline = bar_y + bar_h + (h - name_h - bar_h) * 0.78;
        let (text, color, bold) = if ef.is_dead {
            (String::new(), colors::label_dim(), false)
        } else if ef.targeting_you {
            ("→ YOU".to_string(), colors::targeting_you_red(), true)
        } else if let Some(target) = ef.target_name.as_deref() {
            (format!("→ {target}"), colors::label_dim(), false)
        } else {
            (String::new(), colors::label_dim(), false)
        };
        if !text.is_empty() {
            let text = frame.truncate_to_width(&text, w - inset * 2.0, target_font, bold);
            frame.draw_text_styled(
                &text,
                x + inset,
                target_baseline,
                target_font,
                color,
                bold,
                false,
            );
        }
    }

    // Gone from the log: dim the whole cell so the live enemies stand out
    if ef.stale_secs.is_some() {
        frame.fill_rounded_rect(x, y, w, h, radius, colors::enemy_stale_dim());
    }
}

/// Dummy frames rendered in move mode so the overlay can be positioned
fn placeholder_frames() -> Vec<EnemyFrame> {
    (1..=8)
        .map(|i| EnemyFrame {
            entity_id: i,
            name: format!("Enemy {i}"),
            class_name: None,
            discipline_icon: None,
            current_hp: 50_000 * i,
            max_hp: 400_000,
            target_name: Some("Teammate".to_string()),
            targeting_you: i == 2,
            is_dead: false,
            guarded: false,
            stale_secs: None,
        })
        .collect()
}

// ─────────────────────────────────────────────────────────────────────────────
// Overlay Trait Implementation
// ─────────────────────────────────────────────────────────────────────────────

impl Overlay for EnemyFramesOverlay {
    fn update_data(&mut self, data: OverlayData) -> bool {
        if let OverlayData::EnemyFrames(new_data) = data {
            let changed = new_data != self.data;
            self.data = new_data;
            changed
        } else {
            false
        }
    }

    fn update_config(&mut self, config: OverlayConfigUpdate) {
        if let OverlayConfigUpdate::EnemyFrames(cfg, alpha, european) = config {
            self.set_config(cfg);
            self.set_background_alpha(alpha);
            self.european_number_format = european;
        }
    }

    fn render(&mut self) {
        EnemyFramesOverlay::render(self);
    }

    fn poll_events(&mut self) -> bool {
        self.frame.poll_events()
    }

    fn frame(&self) -> &OverlayFrame {
        &self.frame
    }

    fn frame_mut(&mut self) -> &mut OverlayFrame {
        &mut self.frame
    }
}
