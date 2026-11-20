//! Shared configuration types for BARAS
//!
//! This crate contains serializable configuration types that are shared between
//! the native backend (baras-core) and the WASM frontend (app-ui).

pub mod characters;
pub mod formatting;
pub mod frame_style;
pub mod history;
pub mod metric_template;
pub mod preview;

mod appearance;
mod audio;
mod config;
mod dsl;
mod overlay;
mod query;
mod serde_defaults;
mod ui_state;

pub use appearance::*;
pub use audio::*;
pub use config::*;
pub use dsl::*;
pub use frame_style::{FrameStyle, FrameStyleOverride};
pub use overlay::*;
pub use query::*;
pub use ui_state::*;
