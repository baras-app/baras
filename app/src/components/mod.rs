//! UI Components
//!
//! This module contains reusable UI components extracted from app.rs
//! to improve code organization and reduce file size.

pub mod effect_editor;
pub mod encounter_editor;
pub mod history_panel;
pub mod settings_panel;

pub use effect_editor::EffectEditorPanel;
pub use encounter_editor::EncounterEditorPanel;
pub use history_panel::HistoryPanel;
pub use settings_panel::SettingsPanel;
