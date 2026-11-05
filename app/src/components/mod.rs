//! UI Components
//!
//! This module contains reusable UI components extracted from app.rs
//! to improve code organization and reduce file size.

pub mod ability_icon;
pub mod charts_panel;
pub mod class_icons;
pub mod combat_log;
pub mod contributors_modal;
pub mod data_explorer;
pub mod death_recap_modal;
pub mod effect_editor;
pub mod encounter_editor;
pub mod modifier_editor;
pub mod encounter_types;
pub mod file_open;
pub mod history;
pub mod hotkey_input;
pub mod mirror_names;
pub mod overview_table;
pub mod parsely_upload_modal;
pub mod phase_timeline;
pub mod rotation_view;
pub mod settings_panel;
pub mod slider;
pub mod sound_picker;
pub mod toast;

pub use contributors_modal::ContributorsModal;
pub use data_explorer::DataExplorerPanel;
pub use effect_editor::EffectEditorPanel;
pub use file_open::{FileOpenBanner, use_file_opener, use_file_opener_provider};
pub use encounter_editor::EncounterEditorPanel;
pub use history::HistoryPanel;
pub use hotkey_input::HotkeyInput;
pub use parsely_upload_modal::{ParselyUploadModal, use_parsely_upload, use_parsely_upload_provider};
pub use settings_panel::SettingsPanel;
pub use slider::Slider;
pub use sound_picker::SoundPicker;
pub use toast::{ToastFrame, ToastSeverity, use_toast, use_toast_provider};
