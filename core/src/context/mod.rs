mod area_index;
mod background_tasks;
mod config;
mod error;
mod interner;
mod log_files;
mod parser;
pub mod watcher;

pub use error::{ConfigError, WatcherError};

pub use area_index::{
    FileAreaEntry, FileAreaIndex, LogAreaCache, default_cache_path, extract_areas_from_file,
};
pub use background_tasks::BackgroundTasks;
pub use config::{
    AlertsOverlayConfig, AppConfig, AppConfigExt, BossHealthConfig, ChallengeColumns,
    ChallengeLayout, ChallengeOverlayConfig, Color, HotkeySettings, IconPosition, MAX_PROFILES,
    OverlayAppearanceConfig, OverlayPositionConfig, OverlayProfile, OverlaySettings,
    PersonalOverlayConfig, PersonalStat, PersonalStatCategory, RaidOverlaySettings,
    TimerOverlayConfig, overlay_colors,
};
pub use interner::{IStr, empty_istr, intern, resolve};
pub use log_files::{DirectoryIndex, LogFileMetaData, parse_log_filename};
pub use parser::{DefinitionLoader, ParseResult, ParsingSession, parse_file, resolve_log_path};
