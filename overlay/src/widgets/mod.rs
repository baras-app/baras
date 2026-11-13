//! Reusable UI widgets for overlays
//!
//! These widgets provide building blocks for creating different overlay types.
//! Each widget renders to an `OverlayFrame`.
//!
//! # Available Widgets
//!
//! - [`ProgressBar`] - Horizontal progress bar with label and value
//! - [`LabeledValue`] - Key-value row with right-aligned value
//! - [`CompoundRow`] - Category label with multiple distributed values
//! - [`Header`] - Section title with separator line
//! - [`Footer`] - Summary footer with separator and value
//! - [`IconBarRow`] - Bar row with a reserved icon column

pub mod colors;
mod compound_row;
mod header;
mod icon_bar;
mod labeled_value;
mod progress_bar;

pub use colors::*;
pub use compound_row::{CompoundRow, CompoundValue};
pub use header::{Footer, Header};
pub use icon_bar::{IconBarRow, RowIcon};
pub use labeled_value::LabeledValue;
pub use progress_bar::{darken_color, draw_icon_placeholder, lighten_color, ProgressBar};
