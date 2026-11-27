//! Hotkeys, Parsely, alacrity presets and AppConfig

use crate::serde_defaults::{default_lockout_tiers, default_true};
use crate::{AudioSettings, OverlayProfile, OverlaySettings, history};
use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────────────────────────────────────
// Hotkey Settings
// ─────────────────────────────────────────────────────────────────────────────

/// Global hotkey configuration
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct HotkeySettings {
    #[serde(default)]
    pub toggle_visibility: Option<String>,
    #[serde(default)]
    pub toggle_move_mode: Option<String>,
    #[serde(default)]
    pub toggle_rearrange_mode: Option<String>,
    #[serde(default)]
    pub toggle_operation_timer: Option<String>,
    #[serde(default)]
    pub toggle_live_mode: Option<String>,
    #[serde(default)]
    pub detect_raid_names: Option<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// App Config
// ─────────────────────────────────────────────────────────────────────────────

/// Parsely.io upload settings
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ParselySettings {
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
    /// Legacy single-guild field. Older configs stored a single guild here;
    /// kept for backwards-compatible deserialization. Migrated into `guilds`
    /// on load and not written back out when empty.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub guild: String,
    /// All configured guilds the user can upload to.
    #[serde(default)]
    pub guilds: Vec<String>,
    /// Last selected guild (used as the default in the upload modal).
    #[serde(default)]
    pub selected_guild: Option<String>,
}

impl ParselySettings {
    /// Migrate the legacy `guild: String` field into `guilds` + `selected_guild`.
    /// Idempotent — safe to call repeatedly.
    pub fn migrate_legacy(&mut self) {
        if !self.guild.is_empty() {
            if !self.guilds.iter().any(|g| g == &self.guild) {
                self.guilds.push(self.guild.clone());
            }
            if self.selected_guild.is_none() {
                self.selected_guild = Some(self.guild.clone());
            }
            self.guild.clear();
        }
        // Ensure selected_guild references a known guild (or is None).
        if let Some(sel) = &self.selected_guild
            && !self.guilds.iter().any(|g| g == sel)
        {
            self.selected_guild = self.guilds.first().cloned();
        }
    }

    /// Resolve the active guild for upload. Returns the selected guild if it's
    /// in the configured list, otherwise the first configured guild, otherwise None.
    pub fn active_guild(&self) -> Option<&str> {
        if let Some(sel) = self.selected_guild.as_deref()
            && self.guilds.iter().any(|g| g == sel)
        {
            return Some(sel);
        }
        self.guilds.first().map(|s| s.as_str())
    }
}

/// The alacrity a discipline runs at, optionally for one character only.
/// Gear is not logged, so one value per discipline and character is the limit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AlacrityPreset {
    /// Discipline display name, e.g. "Corruption"
    pub discipline: String,
    /// None applies to every character playing the discipline
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub character: Option<String>,
    pub alacrity_percent: f32,
}

impl AlacrityPreset {
    /// The preset for a character's discipline; one naming the character wins.
    pub fn find<'a>(presets: &'a [Self], discipline: &str, character: &str) -> Option<&'a Self> {
        let of = |named: bool| {
            presets.iter().find(|p| {
                p.discipline == discipline
                    && match &p.character {
                        Some(c) => named && c == character,
                        None => !named,
                    }
            })
        };
        of(true).or_else(|| of(false))
    }
}

// Keep the fixed browser-source URL below Windows' default TCP range
// (49152-65535), where outbound connections can otherwise claim the port.
pub const WEB_OVERLAY_PORT: u16 = 47684;
pub const WEB_OVERLAY_URL: &str = "http://127.0.0.1:47684/overlay";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub log_directory: String,
    /// Automatic log housekeeping (see Log Management)
    #[serde(default)]
    pub log_cleanup: history::LogCleanup,
    /// Scan new and out-of-date logs into the history store at startup
    #[serde(default = "default_true")]
    pub auto_scan_history: bool,
    #[serde(default = "default_true")]
    pub minimize_to_tray: bool,
    /// Agreed to downloading replay map images from the maps repository
    #[serde(default)]
    pub replay_map_downloads: bool,
    /// Serve the currently visible native overlays on a loopback-only web page.
    #[serde(default)]
    pub web_overlay_enabled: bool,
    #[serde(default)]
    pub overlay_settings: OverlaySettings,
    #[serde(default)]
    pub hotkeys: HotkeySettings,
    #[serde(default)]
    pub profiles: Vec<OverlayProfile>,
    #[serde(default)]
    pub active_profile_name: Option<String>,
    #[serde(default)]
    pub parsely: ParselySettings,
    #[serde(default)]
    pub audio: AudioSettings,
    #[serde(default)]
    pub show_only_bosses: bool,
    /// Hide Parsely percentile badges in the Data Explorer (display only)
    #[serde(default)]
    pub hide_percentiles: bool,

    /// Show ability/entity IDs in the combat log.
    #[serde(default)]
    pub show_log_ids: bool,

    /// Hide log files smaller than 1MB in the file browser (enabled by default).
    #[serde(default = "default_true")]
    pub hide_small_log_files: bool,

    /// Player alacrity percentage (e.g., 15.4 for 15.4% alacrity).
    /// Used to calculate actual effect durations.
    #[serde(default = "default_alacrity")]
    pub alacrity_percent: f32,

    /// Saved alacrity values, applied when the local player's discipline changes
    #[serde(default)]
    pub alacrity_presets: Vec<AlacrityPreset>,

    /// Bias adjustment in milliseconds applied to alacrity-scaled effect durations.
    /// Positive lengthens, negative shortens; compensates for a consistent
    /// observed discrepancy (e.g. network latency).
    #[serde(default = "default_latency")]
    pub latency_ms: i16,

    /// Last version for which the changelog was shown.
    /// Used to show "What's New" popup only once per version.
    #[serde(default)]
    pub last_viewed_changelog_version: Option<String>,

    /// Use European number formatting (swap `.` and `,` in numbers).
    /// e.g., `1.50K` becomes `1,50K` and `1,500` becomes `1.500`.
    #[serde(default)]
    pub european_number_format: bool,

    /// Show Parsely percentiles on boss kills in the Data Explorer (fetches
    /// the boss's rating table from parsely.io, at most once a day).
    #[serde(default = "default_true")]
    pub parsely_percentiles: bool,

    /// Use the light color theme in the main app window (overlays are unaffected).
    #[serde(default)]
    pub light_theme: bool,

    /// Raid Summaries "Include flashpoints" switch, remembered across restarts
    /// (seeds `PullFilter::flashpoints`)
    #[serde(default)]
    pub raid_summaries_include_flashpoints: bool,

    /// Characters tab: Story / Veteran / Master columns shown in the lockout matrix
    #[serde(default = "default_lockout_tiers")]
    pub lockout_tiers: [bool; 3],

    /// Automatically enter live mode when combat starts in the Data Explorer.
    /// When false, live mode must be activated manually via the Live button.
    #[serde(default)]
    pub data_explorer_auto_live: bool,

    /// Data Explorer overview table: columns shown and their order, per encounter kind
    #[serde(default)]
    pub overview_columns: crate::OverviewLayouts,

    /// Default profile name per role for auto-switching on discipline change.
    /// Keys are role names: "Tank", "Healer", "Dps".
    #[serde(default)]
    pub default_profile_per_role: std::collections::HashMap<String, String>,

    /// Write the crops handed to OCR, and what came back, to disk on every
    /// detection. Off by default: it is what makes a bad reading diagnosable,
    /// at the cost of a few dozen PNGs per press.
    #[serde(default)]
    pub ocr_debug_dump: bool,

    /// How many detections to keep. The oldest dumps are pruned before a new
    /// one is written, so leaving the option on cannot fill a disk.
    #[serde(default = "default_ocr_debug_max_dumps")]
    pub ocr_debug_max_dumps: u32,
    /// The user has opened the encounter editor's "why a timer may not fire"
    /// help; until then its callout is emphasized
    #[serde(default)]
    pub log_limits_seen: bool,
}

fn default_ocr_debug_max_dumps() -> u32 {
    100
}

fn default_alacrity() -> f32 {
    7.5
}

fn default_latency() -> i16 {
    80
}

/// confy falls back to this when no config file exists yet, so it must carry
/// the real defaults rather than derived `false`s
impl Default for AppConfig {
    fn default() -> Self {
        Self::with_log_directory(String::new())
    }
}

impl AppConfig {
    /// Create a new AppConfig with the specified log directory.
    /// Other fields use their default values.
    pub fn with_log_directory(log_directory: String) -> Self {
        Self {
            log_directory,
            log_cleanup: history::LogCleanup::default(),
            auto_scan_history: true,
            minimize_to_tray: false,
            replay_map_downloads: false,
            web_overlay_enabled: false,
            overlay_settings: OverlaySettings::default(),
            hotkeys: HotkeySettings::default(),
            profiles: Vec::new(),
            active_profile_name: None,
            parsely: ParselySettings::default(),
            audio: AudioSettings::default(),
            show_only_bosses: false,
            hide_percentiles: false,
            show_log_ids: false,
            hide_small_log_files: true,
            alacrity_percent: 0.0,
            alacrity_presets: Vec::new(),
            latency_ms: 0,
            last_viewed_changelog_version: None,
            european_number_format: false,
            parsely_percentiles: true,
            light_theme: false,
            raid_summaries_include_flashpoints: false,
            lockout_tiers: [true; 3],
            data_explorer_auto_live: false,
            overview_columns: Default::default(),
            default_profile_per_role: std::collections::HashMap::new(),
            ocr_debug_dump: false,
            ocr_debug_max_dumps: 100,
            log_limits_seen: false,
        }
    }
}
