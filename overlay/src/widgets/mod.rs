//! Reusable UI widgets for overlays
//!
//! These widgets provide building blocks for creating different overlay types.
//! Each widget renders to an `OverlayFrame`.
//!
//! # Available Widgets
//!
//! - [`ProgressBar`] - Horizontal progress bar with label and value
//! - [`LabeledValue`] - Key-value row with right-aligned value
//! - [`Header`] - Section title with separator line
//! - [`Footer`] - Summary footer with separator and value

pub mod colors;
mod header;
mod labeled_value;
mod progress_bar;

pub use colors::*;
pub use header::{Footer, Header};
pub use labeled_value::LabeledValue;
pub use progress_bar::ProgressBar;
