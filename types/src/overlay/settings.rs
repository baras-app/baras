//! Overlay profiles and combined overlay settings

use crate::appearance::{default_bar_radius, default_bar_spacing_ratio};
use crate::serde_defaults::{default_scaling_factor, default_true};
use crate::{
    AbilityCastOverlayConfig, AbilityQueueOverlayConfig, AlertsOverlayConfig, BossHealthConfig, ChallengeOverlayConfig,
    ClassColorConfig, ClassIconMode, CombatTimeOverlayConfig, CooldownTrackerConfig, DotTrackerConfig, EffectsAConfig,
    EffectsBConfig, EnemyFramesConfig, FrameStyle, FrameStyleOverride, MetricOptions, MetricOverrides, NotesOverlayConfig,
    OperationTimerOverlayConfig, OverlayAppearanceConfig, OverlayPositionConfig, PersonalOverlayConfig,
    RaidOverlaySettings, TimerOverlayConfig,
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
    /// Gap between metric bars as a fraction of bar height; overlays may
    /// override it with `OverlayAppearanceConfig::bar_spacing`
    #[serde(default = "default_bar_spacing_ratio")]
    pub metric_bar_spacing: f32,
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
        Self {
            positions: HashMap::new(),
            appearances: HashMap::new(),
            enabled: HashMap::new(),
            overlays_visible: true,
            overlay_font_family: default_overlay_font_family(),
            frame_style: FrameStyle::default(),
            frame_overrides: HashMap::new(),
            overlay_fps: default_overlay_fps(),
            personal_overlay: PersonalOverlayConfig::default(),
            metric_show_empty_bars: true,
            metric_stack_from_bottom: false,
            metric_scaling_factor: 1.0,
            metric_bar_spacing: default_bar_spacing_ratio(),
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
        }
    }
}

impl OverlaySettings {
    pub fn get_position(&self, overlay_type: &str) -> OverlayPositionConfig {
        self.positions.get(overlay_type).cloned().unwrap_or_default()
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

    /// Move the old per-overlay `*_opacity` fields into `frame_overrides`, old
    /// per-overlay bar spacing into overrides of `metric_bar_spacing`, and the
    /// old metric column toggles into templates.
    /// Idempotent: the legacy fields are consumed and never written back.
    pub fn migrate_legacy(&mut self) {
        let spacing = self.metric_bar_spacing;
        for a in self
            .appearances
            .values_mut()
            .chain(self.default_appearances.values_mut())
        {
            if let Some(legacy) = a.legacy_bar_spacing.take()
                && legacy != spacing
            {
                a.bar_spacing = Some(legacy);
            }
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

        /// Metric overlays that shared `metric_opacity`
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
