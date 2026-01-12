//! UI Components
//!
//! This module contains reusable UI components extracted from app.rs
//! to improve code organization and reduce file size.

pub mod ability_icon;
pub mod charts_panel;
pub mod class_icons;
pub mod combat_log;
pub mod data_explorer;
pub mod effect_editor;
pub mod encounter_editor;
pub mod history_panel;
pub mod phase_timeline;
pub mod settings_panel;

pub use data_explorer::DataExplorerPanel;
pub use effect_editor::EffectEditorPanel;
pub use encounter_editor::EncounterEditorPanel;
pub use history_panel::HistoryPanel;
pub use settings_panel::SettingsPanel;
