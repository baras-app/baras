//! Overlay profiles and combined overlay settings

use crate::appearance::{MAX_BAR_GAP, default_bar_gap, default_bar_radius, unmigrated_bar_gap};
use crate::serde_defaults::{default_scaling_factor, default_true};
use crate::{
    AbilityCastOverlayConfig, AbilityQueueOverlayConfig, AlertsOverlayConfig, BossHealthConfig, ChallengeOverlayConfig,
    ClassColorConfig, ClassIconMode, CombatTimeOverlayConfig, CooldownTrackerConfig, DotTrackerConfig, EffectsAConfig,
    EffectsBConfig, EnemyFramesConfig, FrameStyle, FrameStyleOverride, MetricOptions, MetricOverrides,
    NotesOverlayConfig, OperationTimerOverlayConfig, OverlayAppearanceConfig, OverlayPositionConfig,
    PersonalOverlayConfig, RaidOverlaySettings, TimerOverlayConfig,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ─────────────────────────────────────────────────────────────────────────────
// Overlay Profiles
// ─────────────────────────────────────────────────────────────────────────────

/// Maximum number of profiles a user can create
pub const MAX_PROFILES: usize = 12;

/// A named snapshot of all overlay settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlayProfile {
    pub name: String,
    pub settings: OverlaySettings,
}

impl OverlayProfile {
    pub fn new(name: String, settings: OverlaySettings) -> Self {
        Self { name, settings }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Overlay Settings (combined)
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlaySettings {
    #[serde(default)]
    pub positions: HashMap<String, OverlayPositionConfig>,
    #[serde(default)]
    pub appearances: HashMap<String, OverlayAppearanceConfig>,
    #[serde(default, alias = "visibility")]
    pub enabled: HashMap<String, bool>,
    #[serde(default = "default_true")]
    pub overlays_visible: bool,
    /// Global font family for all overlay text. Empty = bundled default (Inter).
    #[serde(default = "default_overlay_font_family")]
    pub overlay_font_family: String,
    /// Window chrome applied to every overlay unless overridden
    #[serde(default)]
    pub frame_style: FrameStyle,
    /// Per-overlay chrome and font overrides, keyed like `positions`
    #[serde(default)]
    pub frame_overrides: HashMap<String, FrameStyleOverride>,
    /// Fixed render scale of each list overlay, keyed like `positions`; the
    /// window size only sets bounds. Filled by `migrate_legacy`.
    #[serde(default)]
    pub scales: HashMap<String, f32>,
    /// Redraw rate (5–30 FPS) of the countdown overlays (timers, effects,
    /// raid frames, alerts, cooldowns, DoTs) while locked
    #[serde(default = "default_overlay_fps")]
    pub overlay_fps: u8,
    #[serde(default)]
    pub personal_overlay: PersonalOverlayConfig,
    #[serde(default = "default_true")]
    pub metric_show_empty_bars: bool,
    #[serde(default)]
    pub metric_stack_from_bottom: bool,
    #[serde(default = "default_scaling_factor")]
    pub metric_scaling_factor: f32,
    /// Gap between metric bars in pixels, unscaled; overlays may override it
    /// with `OverlayAppearanceConfig::bar_gap`
    #[serde(default = "unmigrated_bar_gap")]
    pub metric_bar_gap: f32,
    /// Pre-`metric_bar_gap` gap as a fraction of bar height; read once by `migrate_legacy`
    #[serde(default, skip_serializing, rename = "metric_bar_spacing")]
    pub legacy_metric_bar_spacing: Option<f32>,
    /// Corner radius of metric bars in pixels at base size; overlays may
    /// override it with `OverlayAppearanceConfig::bar_radius`
    #[serde(default = "default_bar_radius")]
    pub metric_bar_radius: f32,
    /// Font scale multiplier for metric overlays (0.3 - 3.0, default 1.0)
    #[serde(default = "default_scaling_factor")]
    pub metric_font_scale: f32,
    /// How strongly metric bar fills darken toward their trailing edge when the
    /// single-color gradient is enabled. 0.0 = flat color, ~0.32 = default,
    /// higher = more aggressive dark→light fade.
    #[serde(default = "default_gradient_intensity")]
    pub metric_gradient_intensity: f32,
    /// When true, metric overlay backgrounds shrink to fit content
    #[serde(default)]
    pub metric_dynamic_background: bool,
    /// When true, show grey background bar behind each player's fill bar
    #[serde(default)]
    pub metric_show_background_bar: bool,
    #[serde(default)]
    pub class_icon_mode: ClassIconMode,
    #[serde(default)]
    pub default_appearances: HashMap<String, OverlayAppearanceConfig>,
    #[serde(default)]
    pub raid_overlay: RaidOverlaySettings,
    #[serde(default)]
    pub boss_health: BossHealthConfig,
    #[serde(default, alias = "timer_overlay")]
    pub timers_a_overlay: TimerOverlayConfig,
    #[serde(default)]
    pub timers_b_overlay: TimerOverlayConfig,
    #[serde(default)]
    pub effects_overlay: TimerOverlayConfig,
    #[serde(default)]
    pub challenge_overlay: ChallengeOverlayConfig,
    #[serde(default)]
    pub alerts_overlay: AlertsOverlayConfig,
    #[serde(default, alias = "personal_buffs")]
    pub effects_a: EffectsAConfig,
    #[serde(default, alias = "personal_debuffs")]
    pub effects_b: EffectsBConfig,
    #[serde(default)]
    pub effects_c: EffectsAConfig,
    #[serde(default)]
    pub cooldown_tracker: CooldownTrackerConfig,
    #[serde(default)]
    pub cooldown_tracker_b: CooldownTrackerConfig,
    #[serde(default)]
    pub dot_tracker: DotTrackerConfig,
    #[serde(default)]
    pub notes_overlay: NotesOverlayConfig,
    #[serde(default)]
    pub combat_time: CombatTimeOverlayConfig,
    #[serde(default)]
    pub operation_timer: OperationTimerOverlayConfig,
    #[serde(default)]
    pub ability_queue: AbilityQueueOverlayConfig,
    #[serde(default)]
    pub enemy_frames: EnemyFramesConfig,
    #[serde(default)]
    pub ability_cast: AbilityCastOverlayConfig,
    // Pre-`frame_overrides` per-overlay opacities; read once by `migrate_legacy`
    #[serde(default, skip_serializing)]
    metric_opacity: Option<u8>,
    #[serde(default, skip_serializing)]
    personal_opacity: Option<u8>,
    #[serde(default, skip_serializing)]
    raid_opacity: Option<u8>,
    #[serde(default, skip_serializing)]
    boss_health_opacity: Option<u8>,
    #[serde(default, skip_serializing, alias = "timer_opacity")]
    timers_a_opacity: Option<u8>,
    #[serde(default, skip_serializing)]
    timers_b_opacity: Option<u8>,
    #[serde(default, skip_serializing)]
    effects_opacity: Option<u8>,
    #[serde(default, skip_serializing)]
    challenge_opacity: Option<u8>,
    #[serde(default, skip_serializing)]
    alerts_opacity: Option<u8>,
    #[serde(default, skip_serializing, alias = "personal_buffs_opacity")]
    effects_a_opacity: Option<u8>,
    #[serde(default, skip_serializing, alias = "personal_debuffs_opacity")]
    effects_b_opacity: Option<u8>,
    #[serde(default, skip_serializing)]
    effects_c_opacity: Option<u8>,
    #[serde(default, skip_serializing)]
    cooldown_tracker_opacity: Option<u8>,
    #[serde(default, skip_serializing)]
    cooldown_tracker_b_opacity: Option<u8>,
    #[serde(default, skip_serializing)]
    dot_tracker_opacity: Option<u8>,
    #[serde(default, skip_serializing)]
    notes_opacity: Option<u8>,
    #[serde(default, skip_serializing)]
    combat_time_opacity: Option<u8>,
    #[serde(default, skip_serializing)]
    operation_timer_opacity: Option<u8>,
    #[serde(default, skip_serializing)]
    ability_queue_opacity: Option<u8>,
    #[serde(default, skip_serializing)]
    enemy_frames_opacity: Option<u8>,
    #[serde(default, skip_serializing)]
    ability_cast_opacity: Option<u8>,
    /// Auto-hide overlays when local player is in a conversation
    #[serde(default)]
    pub hide_during_conversations: bool,
    /// Auto-hide overlays when not in a live session (historical, logged out, etc.)
    #[serde(default)]
    pub hide_when_not_live: bool,
    /// Per-archetype bar colors used when `use_class_color` is enabled on a metric overlay.
    #[serde(default)]
    pub class_colors: ClassColorConfig,
}

/// Default overlay font family. Must match `baras_overlay::DEFAULT_FONT_FAMILY`
/// (the bundled Inter face); kept as a literal here since `baras-types` is the
/// lowest crate and cannot depend on `baras-overlay`.
fn default_overlay_font_family() -> String {
    "Inter".to_string()
}

fn default_overlay_fps() -> u8 {
    10
}

/// Default metric bar gradient intensity. Must match
/// `baras_overlay::widgets::progress_bar::GRADIENT_DARKEN`.
fn default_gradient_intensity() -> f32 {
    0.32
}

impl Default for OverlaySettings {
    fn default() -> Self {
        let mut settings = Self {
            positions: HashMap::new(),
            appearances: HashMap::new(),
            enabled: HashMap::new(),
            overlays_visible: true,
            overlay_font_family: default_overlay_font_family(),
            frame_style: FrameStyle::default(),
            frame_overrides: HashMap::new(),
            scales: HashMap::new(),
            overlay_fps: default_overlay_fps(),
            personal_overlay: PersonalOverlayConfig::default(),
            metric_show_empty_bars: true,
            metric_stack_from_bottom: false,
            metric_scaling_factor: 1.0,
            metric_bar_gap: default_bar_gap(),
            legacy_metric_bar_spacing: None,
            metric_bar_radius: default_bar_radius(),
            metric_font_scale: 1.0,
            metric_gradient_intensity: 0.32,
            metric_dynamic_background: false,
            metric_show_background_bar: false,
            class_icon_mode: ClassIconMode::Class,
            default_appearances: HashMap::new(),
            class_colors: ClassColorConfig::default(),
            raid_overlay: RaidOverlaySettings::default(),
            boss_health: BossHealthConfig::default(),
            timers_a_overlay: TimerOverlayConfig::default(),
            timers_b_overlay: TimerOverlayConfig::default(),
            effects_overlay: TimerOverlayConfig::default(),
            challenge_overlay: ChallengeOverlayConfig::default(),
            alerts_overlay: AlertsOverlayConfig::default(),
            effects_a: EffectsAConfig::default(),
            effects_b: EffectsBConfig::default(),
            effects_c: EffectsAConfig::default(),
            cooldown_tracker: CooldownTrackerConfig::default(),
            cooldown_tracker_b: CooldownTrackerConfig::default(),
            dot_tracker: DotTrackerConfig::default(),
            notes_overlay: NotesOverlayConfig::default(),
            combat_time: CombatTimeOverlayConfig::default(),
            operation_timer: OperationTimerOverlayConfig::default(),
            ability_queue: AbilityQueueOverlayConfig::default(),
            enemy_frames: EnemyFramesConfig::default(),
            ability_cast: AbilityCastOverlayConfig::default(),
            metric_opacity: None,
            personal_opacity: None,
            raid_opacity: None,
            boss_health_opacity: None,
            timers_a_opacity: None,
            timers_b_opacity: None,
            effects_opacity: None,
            challenge_opacity: None,
            alerts_opacity: None,
            effects_a_opacity: None,
            effects_b_opacity: None,
            effects_c_opacity: None,
            cooldown_tracker_opacity: None,
            cooldown_tracker_b_opacity: None,
            dot_tracker_opacity: None,
            notes_opacity: None,
            combat_time_opacity: None,
            operation_timer_opacity: None,
            ability_queue_opacity: None,
            enemy_frames_opacity: None,
            ability_cast_opacity: None,
            hide_during_conversations: false,
            hide_when_not_live: false,
        };
        settings.freeze_list_scales();
        settings
    }
}

/// Metric overlay config keys
const METRIC_KEYS: [&str; 11] = [
    "dps",
    "edps",
    "bossdps",
    "hps",
    "ehps",
    "abs",
    "dtps",
    "tps",
    "apm",
    "interrupts",
    "incoming_damage",
];

/// Scale bounds offered for list overlays
pub const MIN_LIST_SCALE: f32 = 0.25;
pub const MAX_LIST_SCALE: f32 = 4.0;

/// Base window size of a list overlay (mirrors its `BASE_WIDTH`/`BASE_HEIGHT`
/// in baras-overlay), `None` for overlays that still scale with their window.
fn list_overlay_base_size(key: &str) -> Option<(f32, f32)> {
    Some(match key {
        "timers_a" | "timers_b" => (220.0, 150.0),
        "alerts" => (220.0, 120.0),
        "challenges" => (320.0, 400.0),
        "effects_a" | "effects_b" | "effects_c" => (300.0, 300.0),
        "cooldowns" | "cooldowns_b" => (180.0, 300.0),
        "dot_tracker" => (280.0, 200.0),
        "notes" => (400.0, 300.0),
        "ability_queue" => (220.0, 200.0),
        "ability_cast" => (200.0, 240.0),
        k if METRIC_KEYS.contains(&k) => (280.0, 200.0),
        _ => return None,
    })
}

const LIST_OVERLAY_KEYS: [&str; 13] = [
    "timers_a",
    "timers_b",
    "alerts",
    "challenges",
    "effects_a",
    "effects_b",
    "effects_c",
    "cooldowns",
    "cooldowns_b",
    "dot_tracker",
    "notes",
    "ability_queue",
    "ability_cast",
];

impl OverlaySettings {
    pub fn get_position(&self, overlay_type: &str) -> OverlayPositionConfig {
        self.positions.get(overlay_type).cloned().unwrap_or_else(|| {
            let default = OverlayPositionConfig::default();
            match overlay_type {
                // Tall enough for the default stat rows at the default width
                "personal" => OverlayPositionConfig { height: 240, ..default },
                _ => default,
            }
        })
    }

    pub fn set_position(&mut self, overlay_type: &str, config: OverlayPositionConfig) {
        self.positions.insert(overlay_type.to_string(), config);
    }

    pub fn get_appearance(&self, overlay_type: &str) -> OverlayAppearanceConfig {
        self.appearances.get(overlay_type).cloned().unwrap_or_default()
    }

    pub fn set_appearance(&mut self, overlay_type: &str, config: OverlayAppearanceConfig) {
        self.appearances.insert(overlay_type.to_string(), config);
    }

    /// Render scale of a list overlay, `None` for overlays that scale with
    /// their window. An overlay without a stored scale gets the one its saved
    /// window size produced under the old window-driven model.
    pub fn list_scale(&self, overlay_type: &str) -> Option<f32> {
        let (base_w, base_h) = list_overlay_base_size(overlay_type)?;
        Some(self.scales.get(overlay_type).copied().unwrap_or_else(|| {
            let pos = self.get_position(overlay_type);
            ((pos.width as f32 / base_w) * (pos.height as f32 / base_h)).sqrt()
        }))
    }

    pub fn set_list_scale(&mut self, overlay_type: &str, scale: f32) {
        self.scales
            .insert(overlay_type.to_string(), scale.clamp(MIN_LIST_SCALE, MAX_LIST_SCALE));
    }

    /// Pin every list overlay's scale to what it currently renders at, so
    /// later window resizes stop changing it. Stored scales are left alone.
    fn freeze_list_scales(&mut self) {
        for key in METRIC_KEYS.into_iter().chain(LIST_OVERLAY_KEYS) {
            if !self.scales.contains_key(key)
                && let Some(scale) = self.list_scale(key)
            {
                self.scales.insert(key.to_string(), scale);
            }
        }
    }

    /// Turn the old bar spacing (a fraction of bar height, scaled with the
    /// overlay) into pixel gaps: the global one as drawn at 100% scale, plus an
    /// override on each placed overlay whose scale drew it differently.
    fn migrate_bar_gaps(&mut self) {
        /// Metric bar height at 100% scale and thickness
        const BASE_BAR_HEIGHT: f32 = 20.0;
        let gap = |ratio: f32, thickness: f32, scale: f32| {
            (ratio.clamp(0.0, 0.6) * BASE_BAR_HEIGHT * thickness.clamp(0.3, 2.0) * scale)
                .round()
                .min(MAX_BAR_GAP)
        };
        let challenge_ratio = self.challenge_overlay.legacy_bar_spacing_ratio.take().unwrap_or(0.2);
        if self.challenge_overlay.bar_gap < 0.0 {
            /// Challenge bar height at 100% scale
            const CHALLENGE_BAR_HEIGHT: f32 = 18.0;
            let scale = self.list_scale("challenges").unwrap_or(1.0);
            self.challenge_overlay.bar_gap = gap(challenge_ratio * CHALLENGE_BAR_HEIGHT / BASE_BAR_HEIGHT, 1.0, scale);
        }

        let ratio = self.legacy_metric_bar_spacing.take().unwrap_or(0.2);
        let migrate = self.metric_bar_gap < 0.0;
        if migrate {
            self.metric_bar_gap = gap(ratio, self.metric_scaling_factor, 1.0);
        }
        for key in METRIC_KEYS {
            let a = self.appearances.get_mut(key);
            let old = a.and_then(|a| a.legacy_bar_spacing.take().or(a.legacy_bar_spacing_ratio.take()));
            if !migrate {
                continue;
            }
            let thickness = self
                .appearances
                .get(key)
                .and_then(|a| a.overrides.scaling_factor)
                .unwrap_or(self.metric_scaling_factor);
            let scale = match self.positions.contains_key(key) {
                true => self.list_scale(key).unwrap_or(1.0),
                false => 1.0,
            };
            let px = gap(old.unwrap_or(ratio), thickness, scale);
            if px != self.metric_bar_gap {
                self.appearances
                    .entry(key.to_string())
                    .or_insert_with(|| OverlayAppearanceConfig::default_for_type(key))
                    .bar_gap = Some(px);
            }
        }
    }

    /// Move the old per-overlay `*_opacity` fields into `frame_overrides`, old
    /// bar spacing fractions into pixel gaps, and the old metric column toggles
    /// into templates; freeze list overlay scales.
    /// Idempotent: the legacy fields are consumed and never written back.
    pub fn migrate_legacy(&mut self) {
        self.freeze_list_scales();
        self.migrate_bar_gaps();
        for a in self
            .appearances
            .values_mut()
            .chain(self.default_appearances.values_mut())
        {
            let (per_second, total) = (a.legacy_show_per_second.take(), a.legacy_show_total.take());
            if per_second.is_some() || total.is_some() {
                a.template = match (per_second.unwrap_or(true), total.unwrap_or(false)) {
                    (true, true) => "{total} | {rate}",
                    (true, false) => "{rate}",
                    (false, true) => "{total}",
                    (false, false) => "",
                }
                .to_string();
            }
        }

        // The raid and legacy effects overlays never drew this background
        self.raid_opacity = None;
        self.effects_opacity = None;

        let global = self.frame_style.opacity;
        let metric = self.metric_opacity.take();
        let single = [
            ("personal", self.personal_opacity.take()),
            ("boss_health", self.boss_health_opacity.take()),
            ("timers_a", self.timers_a_opacity.take()),
            ("timers_b", self.timers_b_opacity.take()),
            ("challenges", self.challenge_opacity.take()),
            ("alerts", self.alerts_opacity.take()),
            ("effects_a", self.effects_a_opacity.take()),
            ("effects_b", self.effects_b_opacity.take()),
            ("effects_c", self.effects_c_opacity.take()),
            ("cooldowns", self.cooldown_tracker_opacity.take()),
            ("cooldowns_b", self.cooldown_tracker_b_opacity.take()),
            ("dot_tracker", self.dot_tracker_opacity.take()),
            ("notes", self.notes_opacity.take()),
            ("combat_time", self.combat_time_opacity.take()),
            ("operation_timer", self.operation_timer_opacity.take()),
            ("ability_queue", self.ability_queue_opacity.take()),
            ("enemy_frames", self.enemy_frames_opacity.take()),
            ("ability_cast", self.ability_cast_opacity.take()),
        ];
        let legacy = METRIC_KEYS.into_iter().map(|k| (k, metric)).chain(single);
        for (key, opacity) in legacy {
            if let Some(opacity) = opacity
                && opacity != global
            {
                self.frame_overrides.entry(key.to_string()).or_default().opacity = Some(opacity);
            }
        }
    }

    /// The global metric options with one overlay's overrides applied
    pub fn metric_options_for(&self, o: &MetricOverrides) -> MetricOptions {
        MetricOptions {
            show_empty_bars: o.show_empty_bars.unwrap_or(self.metric_show_empty_bars),
            stack_from_bottom: o.stack_from_bottom.unwrap_or(self.metric_stack_from_bottom),
            scaling_factor: o.scaling_factor.unwrap_or(self.metric_scaling_factor),
            class_icon_mode: o.class_icon_mode.unwrap_or(self.class_icon_mode),
            font_scale: o.font_scale.unwrap_or(self.metric_font_scale),
            dynamic_background: o.dynamic_background.unwrap_or(self.metric_dynamic_background),
            show_background_bar: o.show_background_bar.unwrap_or(self.metric_show_background_bar),
            gradient_intensity: o.gradient_intensity.unwrap_or(self.metric_gradient_intensity),
        }
    }

    pub fn frame_style_for(&self, overlay_type: &str) -> FrameStyle {
        match self.frame_overrides.get(overlay_type) {
            Some(o) => self.frame_style.with_override(o),
            None => self.frame_style,
        }
    }

    pub fn font_family_for(&self, overlay_type: &str) -> &str {
        self.frame_overrides
            .get(overlay_type)
            .and_then(|o| o.font_family.as_deref())
            .unwrap_or(&self.overlay_font_family)
    }

    pub fn is_enabled(&self, overlay_type: &str) -> bool {
        self.enabled.get(overlay_type).copied().unwrap_or(false)
    }

    pub fn set_enabled(&mut self, overlay_type: &str, enabled: bool) {
        self.enabled.insert(overlay_type.to_string(), enabled);
    }

    pub fn enabled_types(&self) -> Vec<String> {
        self.enabled
            .iter()
            .filter_map(|(k, &v)| if v { Some(k.clone()) } else { None })
            .collect()
    }
}
