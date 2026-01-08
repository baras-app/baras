//! Application configuration
//!
//! This module re-exports shared types from baras-types and provides
//! platform-specific Default implementation and persistence for AppConfig.

// Re-export all shared types
pub use baras_types::{
    AlertsOverlayConfig, AppConfig, BossHealthConfig, ChallengeColumns, ChallengeLayout,
    ChallengeOverlayConfig, Color, HotkeySettings, MAX_PROFILES, OverlayAppearanceConfig,
    OverlayPositionConfig, OverlayProfile, OverlaySettings, PersonalOverlayConfig, PersonalStat,
    RaidOverlaySettings, TimerOverlayConfig, overlay_colors,
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
    fn save(self);
    fn save_profile(&mut self, name: String) -> Result<(), &'static str>;
    fn load_profile(&mut self, name: &str) -> Result<(), &'static str>;
    fn delete_profile(&mut self, name: &str) -> Result<(), &'static str>;
    fn rename_profile(&mut self, old_name: &str, new_name: String) -> Result<(), &'static str>;
    fn profile_names(&self) -> Vec<String>;
    fn is_profile_name_available(&self, name: &str) -> bool;
}

impl AppConfigExt for AppConfig {
    fn load() -> Self {
        confy::load("baras", "config").unwrap_or_else(|_| Self::load_with_defaults())
    }

    /// Load with platform-specific defaults (used when no config file exists)
    fn load_with_defaults() -> Self {
        AppConfig::with_log_directory(default_log_directory())
    }

    fn save(self) {
        confy::store("baras", "config", self).expect("Failed to save configuration");
    }

    fn save_profile(&mut self, name: String) -> Result<(), &'static str> {
        // Check if profile already exists (update case)
        if let Some(profile) = self.profiles.iter_mut().find(|p| p.name == name) {
            profile.settings = self.overlay_settings.clone();
            self.active_profile_name = Some(name);
            return Ok(());
        }

        // New profile - check limit
        if self.profiles.len() >= MAX_PROFILES {
            return Err("Maximum number of profiles reached (12)");
        }

        self.profiles.push(OverlayProfile::new(
            name.clone(),
            self.overlay_settings.clone(),
        ));
        self.active_profile_name = Some(name);
        Ok(())
    }

    fn load_profile(&mut self, name: &str) -> Result<(), &'static str> {
        let profile = self
            .profiles
            .iter()
            .find(|p| p.name == name)
            .ok_or("Profile not found")?;
        self.overlay_settings = profile.settings.clone();
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
            self.active_profile_name = Some(new_name);
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
