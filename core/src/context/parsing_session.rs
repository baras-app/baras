use crate::context::AppConfig;
use crate::CombatEvent;
use crate::session_cache::SessionCache;
use chrono::NaiveDateTime;
use std::path::{Path, PathBuf};

#[derive(Default)]
pub struct ParsingSession {
    pub current_byte: Option<u64>,
    pub active_file: Option<PathBuf>,
    pub game_session_date: Option<NaiveDateTime>,
    pub session_cache: Option<SessionCache>,
}

impl ParsingSession {
    pub fn new(path: PathBuf) -> Self {
        let date_stamp = parse_log_timestamp(&path);
        Self {
            current_byte: None,
            active_file: Some(path),
            game_session_date: date_stamp,
            session_cache: Some(SessionCache::new()),
        }
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

fn parse_log_timestamp(path: &Path) -> Option<NaiveDateTime> {
    let stem = path
        .file_stem()?
        .to_str()?
        .trim_start_matches("combat_");
    NaiveDateTime::parse_from_str(stem, "%Y-%m-%d_%H_%M_%S_%f").ok()
}

/// Resolve a log file path, joining with log_directory if relative.
pub fn resolve_log_path(config: &AppConfig, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        Path::new(&config.log_directory).join(path)
    }
}
