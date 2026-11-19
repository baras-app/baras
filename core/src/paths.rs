//! Where BARAS keeps its own files.

use std::path::PathBuf;

/// Points every BARAS file (config, history, definitions, caches, logs) at
/// another directory, so a development build leaves the installed one alone.
pub const CONFIG_DIR_ENV: &str = "BARAS_CONFIG_DIR";

fn override_dir() -> Option<PathBuf> {
    std::env::var_os(CONFIG_DIR_ENV)
        .filter(|d| !d.is_empty())
        .map(PathBuf::from)
}

/// Whether `BARAS_CONFIG_DIR` redirects this process away from the default directory
pub fn is_overridden() -> bool {
    override_dir().is_some()
}

/// `~/.config/baras` or the platform equivalent, unless `BARAS_CONFIG_DIR` is set
pub fn config_dir() -> Option<PathBuf> {
    override_dir().or_else(|| dirs::config_dir().map(|p| p.join("baras")))
}

/// `config.toml`. Without the override this is wherever confy has always kept
/// it, which on Windows and macOS is not directly under `config_dir()`.
pub fn config_file() -> Result<PathBuf, confy::ConfyError> {
    match override_dir() {
        Some(dir) => Ok(dir.join("config.toml")),
        None => confy::get_configuration_file_path("baras", "config"),
    }
}
