mod config;
mod background_tasks;
mod log_files;
mod interner;
mod parser;
pub mod watcher;

pub use config::{
    AppConfig, AppConfigExt, BossHealthConfig, Color, HotkeySettings,
    OverlayAppearanceConfig, OverlayPositionConfig, OverlayProfile, OverlaySettings,
    PersonalOverlayConfig, PersonalStat, RaidOverlaySettings, MAX_PROFILES, overlay_colors,
};
pub use background_tasks::BackgroundTasks;
pub use log_files::{DirectoryIndex, parse_log_filename};
pub use interner::{IStr, intern, resolve, empty_istr};
pub use parser::{ParsingSession, ParseResult, parse_file, resolve_log_path};
