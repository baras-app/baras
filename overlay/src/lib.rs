//! Baras Overlay Library
//!
//! Cross-platform overlay rendering for combat log statistics.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────┐
//! │                    overlays/                        │
//! │   MetricOverlay, TimerOverlay, BossHealthOverlay     │
//! │          (complete overlay implementations)          │
//! ├─────────────────────────────────────────────────────┤
//! │                    widgets/                          │
//! │        ProgressBar, TimerBar, HealthBar              │
//! │            (reusable UI components)                  │
//! ├─────────────────────────────────────────────────────┤
//! │                    manager                           │
//! │                  OverlayWindow                       │
//! │          (window + renderer wrapper)                 │
//! ├─────────────────────────────────────────────────────┤
//! │                    renderer                          │
//! │            tiny-skia + cosmic-text                   │
//! │              (drawing primitives)                    │
//! ├─────────────────────────────────────────────────────┤
//! │                    platform/                         │
//! │         wayland, x11, windows, macos                 │
//! │            (OS window management)                    │
//! └─────────────────────────────────────────────────────┘
//! ```

pub mod frame;
pub mod manager;
pub mod overlays;
pub mod platform;
pub mod renderer;
pub mod utils;
pub mod widgets;

// Re-export commonly used types
pub use frame::OverlayFrame;
pub use manager::OverlayWindow;
pub use overlays::{
    MetricEntry, MetricOverlay, Overlay, OverlayConfigUpdate, OverlayData, OverlayPosition,
    PersonalOverlay, PersonalStats,
};
pub use platform::{
    clamp_to_virtual_screen, find_monitor_at, find_monitor_by_id, get_all_monitors,
    resolve_absolute_position, MonitorInfo, NativeOverlay, OverlayConfig, OverlayPlatform,
    PlatformError, VirtualScreenBounds,
};
pub use renderer::{colors, Renderer};
pub use utils::{color_from_rgba, format_number, format_time, truncate_name};
pub use widgets::{Footer, Header, LabeledValue, ProgressBar};

// Re-export tiny_skia Color for external use
pub use tiny_skia::Color;
