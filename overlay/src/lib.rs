//! Baras Overlay Library
//!
//! Cross-platform overlay rendering for combat log statistics.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────┐
//! │                    overlays/                        │
//! │   MeterOverlay, TimerOverlay, BossHealthOverlay     │
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

pub mod manager;
pub mod overlays;
pub mod platform;
pub mod renderer;
pub mod widgets;

// Re-export commonly used types
pub use manager::OverlayWindow;
pub use overlays::{MeterEntry, MeterOverlay};
pub use platform::{NativeOverlay, OverlayConfig, OverlayPlatform, PlatformError};
pub use renderer::{colors, Renderer};
pub use widgets::ProgressBar;

// Re-export tiny_skia Color for external use
pub use tiny_skia::Color;
