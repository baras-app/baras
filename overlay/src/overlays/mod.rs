//! Complete overlay implementations
//!
//! Each overlay type is a self-contained window that displays specific
//! combat information. Overlays use widgets for rendering and the platform
//! layer for window management.

mod metric;
mod personal;

pub use metric::{MeterEntry, MetricOverlay};
pub use personal::{PersonalOverlay, PersonalStats};
