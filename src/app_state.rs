use crate::CombatEvent;
use crate::directory_index::LogFileIndex;
use crate::session_cache::SessionCache;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use time::Date;
use time::format_description::well_known::Iso8601;

#[derive(Default)]
pub struct AppState {
    pub current_byte: Option<u64>,
    pub config: AppConfig,
    pub active_file: Option<PathBuf>,
    pub game_session_date: Option<Date>,
    pub session_cache: Option<SessionCache>,
    pub log_tail_task: Option<tokio::task::JoinHandle<()>>,
    pub file_index: Option<LogFileIndex>,
    pub watcher_task: Option<tokio::task::JoinHandle<()>>,
}

impl AppState {
    pub fn new() -> Self {
        let config = confy::load("baras", None).unwrap_or_default();
        Self {
            current_byte: None,
            config,
            active_file: None,
            game_session_date: None,
            session_cache: None,
            log_tail_task: None,
            file_index: None,
            watcher_task: None,
        }
    }

    pub fn set_active_file(&mut self, path: &str) {
        let given_path = Path::new(path);
        let stem = given_path
            .file_stem()
            .expect("missing file name")
            .to_str()
            .expect("invalid UTF-8 in file name")
            .split('_')
            .nth(1)
            .expect("invalid file format: expected combat_YYYY-MM-DD_...");

        let format = Iso8601::DATE;
        let date_stamp = Date::parse(stem, &format).expect("failed to parse date from file name");

        let resolved = if given_path.is_relative() {
            Path::new(&self.config.log_directory).join(given_path)
        } else {
            given_path.to_path_buf()
        };

        self.active_file = Some(resolved);
        self.game_session_date = Some(date_stamp);
        self.session_cache = Some(SessionCache::new(date_stamp));
    }

    pub fn process_event(&mut self, event: CombatEvent) {
        if let Some(cache) = &mut self.session_cache {
            cache.process_event(event);
        }
    }

    pub fn process_events(&mut self, events: Vec<CombatEvent>) {
        if let Some(cache) = &mut self.session_cache {
            for event in events {
                cache.process_event(event);
            }
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct AppConfig {
    pub log_directory: String,
    #[serde(default)]
    pub auto_delete_empty_files: bool,
    #[serde(default)]
    pub log_retention_days: u32,
}

impl ::std::default::Default for AppConfig {
    fn default() -> Self {
        Self {
            log_directory: "/home/prescott/baras/test-log-files/".to_string(),
            auto_delete_empty_files: false,
            log_retention_days: 21,
        }
    }
}
