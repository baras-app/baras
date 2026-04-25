//! Application configuration
//!
//! This module re-exports shared types from baras-types and provides
//! platform-specific Default implementation and persistence for AppConfig.

use super::error::ConfigError;

// Re-export all shared types
pub use baras_types::{
    overlay_colors, AlertsOverlayConfig, AppConfig, BossHealthConfig, ChallengeColumns,
    ChallengeLayout, ChallengeOverlayConfig, Color, HotkeySettings, OverlayAppearanceConfig,
    OverlayPositionConfig, OverlayProfile, OverlaySettings, PersonalOverlayConfig, PersonalStat,
    PersonalStatCategory, RaidOverlaySettings, TimerOverlayConfig, MAX_PROFILES,
};

// ─────────────────────────────────────────────────────────────────────────────
// Platform-Specific Defaults
// ─────────────────────────────────────────────────────────────────────────────

fn default_log_directory() -> String {
    #[cfg(target_os = "windows")]
    {
        dirs::document_dir()
            .map(|p| p.join("Star Wars - The Old Republic/CombatLogs"))
            .and_then(|p| p.to_str().map(String::from))
            .unwrap_or_default()
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        dirs::home_dir()
            .map(|p| {
                p.join(".local/share/Steam/steamapps/compatdata/1286830/pfx/drive_c/users/steamuser/Documents/Star Wars - The Old Republic/CombatLogs")
            })
            .and_then(|p| p.to_str().map(String::from))
            .unwrap_or_default()
    }
    #[cfg(target_os = "macos")]
    {
        String::new()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// AppConfig Extensions
// ─────────────────────────────────────────────────────────────────────────────

/// Extension trait for AppConfig persistence and profile management
pub trait AppConfigExt {
    fn load() -> Self;
    fn load_with_defaults() -> Self;
    fn save(self) -> Result<(), ConfigError>;
    fn save_profile(&mut self, name: String) -> Result<(), &'static str>;
    fn load_profile(&mut self, name: &str) -> Result<(), &'static str>;
    fn delete_profile(&mut self, name: &str) -> Result<(), &'static str>;
    fn rename_profile(&mut self, old_name: &str, new_name: String) -> Result<(), &'static str>;
    fn profile_names(&self) -> Vec<String>;
    fn is_profile_name_available(&self, name: &str) -> bool;
}

impl AppConfigExt for AppConfig {
    fn load() -> Self {
        match confy::load::<Self>("baras", "config") {
            Ok(mut config) => {
                config.parsely.migrate_legacy();
                config
            }
            Err(e) => {
                tracing::warn!("Failed to load config: {e}");

                // Back up the invalid config file so it isn't lost on next save
                if let Ok(path) = confy::get_configuration_file_path("baras", "config") {
                    if path.exists() {
                        let backup = path.with_extension("toml.bak");
                        if let Err(e) = std::fs::copy(&path, &backup) {
                            tracing::error!(
                                "Failed to back up config to {}: {e}",
                                backup.display()
                            );
                        } else {
                            tracing::info!("Backed up invalid config to {}", backup.display());
                        }
                    }
                }

                Self::load_with_defaults()
            }
        }
    }

    /// Load with platform-specific defaults (used when no config file exists)
    fn load_with_defaults() -> Self {
        AppConfig::with_log_directory(default_log_directory())
    }

    fn save(self) -> Result<(), ConfigError> {
        confy::store("baras", "config", self).map_err(ConfigError::Save)?;
        tracing::debug!("Configuration saved successfully");
        Ok(())
    }

    fn save_profile(&mut self, name: String) -> Result<(), &'static str> {
        // Clone settings but reset behavioral flags to defaults (they are independent of profiles)
        let mut settings_to_save = self.overlay_settings.clone();
        settings_to_save.overlays_visible = true;
        settings_to_save.hide_when_not_live = false;
        settings_to_save.hide_during_conversations = false;

        // Check if profile already exists (update case)
        if let Some(profile) = self.profiles.iter_mut().find(|p| p.name == name) {
            profile.settings = settings_to_save;
            self.active_profile_name = Some(name);
            return Ok(());
        }

        // New profile - check limit
        if self.profiles.len() >= MAX_PROFILES {
            return Err("Maximum number of profiles reached (12)");
        }

        self.profiles
            .push(OverlayProfile::new(name.clone(), settings_to_save));
        self.active_profile_name = Some(name);
        Ok(())
    }

    fn load_profile(&mut self, name: &str) -> Result<(), &'static str> {
        let profile = self
            .profiles
            .iter()
            .find(|p| p.name == name)
            .ok_or("Profile not found")?;

        // Preserve behavioral state - these are independent of profiles
        let was_visible = self.overlay_settings.overlays_visible;
        let was_hide_not_live = self.overlay_settings.hide_when_not_live;
        let was_hide_conversations = self.overlay_settings.hide_during_conversations;
        self.overlay_settings = profile.settings.clone();
        self.overlay_settings.overlays_visible = was_visible;
        self.overlay_settings.hide_when_not_live = was_hide_not_live;
        self.overlay_settings.hide_during_conversations = was_hide_conversations;

        self.active_profile_name = Some(name.to_string());
        Ok(())
    }

    fn delete_profile(&mut self, name: &str) -> Result<(), &'static str> {
        let len_before = self.profiles.len();
        self.profiles.retain(|p| p.name != name);
        if self.profiles.len() == len_before {
            return Err("Profile not found");
        }
        if self.active_profile_name.as_deref() == Some(name) {
            self.active_profile_name = None;
        }
        // Clear any role defaults pointing to this profile
        self.default_profile_per_role.retain(|_, v| v != name);
        Ok(())
    }

    fn rename_profile(&mut self, old_name: &str, new_name: String) -> Result<(), &'static str> {
        if self.profiles.iter().any(|p| p.name == new_name) {
            return Err("A profile with that name already exists");
        }

        let profile = self
            .profiles
            .iter_mut()
            .find(|p| p.name == old_name)
            .ok_or("Profile not found")?;
        profile.name = new_name.clone();

        if self.active_profile_name.as_deref() == Some(old_name) {
            self.active_profile_name = Some(new_name.clone());
        }
        // Update any role defaults pointing to the old name
        for value in self.default_profile_per_role.values_mut() {
            if value == old_name {
                *value = new_name.clone();
            }
        }
        Ok(())
    }

    fn profile_names(&self) -> Vec<String> {
        self.profiles.iter().map(|p| p.name.clone()).collect()
    }

    fn is_profile_name_available(&self, name: &str) -> bool {
        !self.profiles.iter().any(|p| p.name == name)
    }
}
