use crate::CombatEvent;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use time::Date;
use time::format_description::well_known::Iso8601;

#[derive(Default)]
pub struct AppState {
    pub events: Vec<CombatEvent>,
    pub current_byte: Option<u64>,
    pub config: AppConfig,
    pub active_file: Option<PathBuf>,
    pub game_session_date: Option<Date>,
}

impl AppState {
    pub fn new() -> Self {
        let config = confy::load("baras", None).unwrap_or_default();
        Self {
            events: vec![],
            current_byte: None,
            config,
            active_file: None,
            game_session_date: None,
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
    }
}

#[derive(Serialize, Deserialize)]
pub struct AppConfig {
    pub log_directory: String,
}

impl ::std::default::Default for AppConfig {
    fn default() -> Self {
        Self {
            log_directory: "/home/prescott/baras/test-log-files/".to_string(),
        }
    }
}
