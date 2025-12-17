use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
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

impl AppConfig {
    pub fn load() -> Self {
        confy::load("baras", None).unwrap_or_default()
    }
}
