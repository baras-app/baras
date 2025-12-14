use crate::CombatEvent;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Default)]
pub struct AppState {
    pub events: Vec<CombatEvent>,
    pub current_byte: Option<u64>,
    pub config: AppConfig,
    pub active_file: Option<PathBuf>,
}

impl AppState {
    pub fn new() -> Self {
        let config = confy::load("baras", None).unwrap_or_default();
        Self {
            events: vec![],
            current_byte: None,
            config,
            active_file: None,
        }
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
// /home/prescott/baras/test-log-files/50mb/combat_2025-12-10_18_12_15_087604.txt
