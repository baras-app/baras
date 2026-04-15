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

pub mod class_icons;
pub mod frame;
pub mod icons;
pub mod manager;
pub mod overlays;
pub mod platform;
pub mod renderer;
pub mod utils;
pub mod widgets;

// Re-export commonly used types
pub use class_icons::{
    ClassIcon, Role, get_class_icon, get_role_colored_class_icon, get_role_icon,
    get_tinted_class_icon, get_white_class_icon,
};
pub use frame::OverlayFrame;
pub use manager::OverlayWindow;
pub use overlays::{
    AlertEntry,
    AlertsData,
    AlertsOverlay,
    BossEffectIcon,
    BossHealthData,
    BossHealthOverlay,
    ChallengeData,
    ChallengeEntry,
    ChallengeOverlay,
    // Combat time overlay
    CombatTimeConfig,
    CombatTimeData,
    CombatTimeOverlay,
    // Cooldowns overlay
    CooldownConfig,
    CooldownData,
    CooldownEntry,
    CooldownOverlay,
    // DOT tracker overlay
    DotEntry,
    DotTarget,
    DotTrackerConfig,
    DotTrackerData,
    DotTrackerOverlay,
    // Effect config bounds
    EFFECT_OFFSET_DEFAULT,
    EFFECT_OFFSET_MAX,
    EFFECT_OFFSET_MIN,
    EFFECT_SIZE_DEFAULT,
    EFFECT_SIZE_MAX,
    EFFECT_SIZE_MIN,
    // Effects A/B overlay (consolidated personal effects)
    EffectABEntry,
    EffectEntry,
    EffectsABConfig,
    EffectsABData,
    EffectsABOverlay,
    EffectsData,
    EffectsLayout,
    EffectsOverlay,
    InteractionMode,
    MetricEntry,
    MetricOverlay,
    // Notes overlay
    NotesConfig,
    NotesData,
    NotesOverlay,
    // Operation timer overlay
    OperationTimerConfig,
    OperationTimerData,
    OperationTimerOverlay,
    Overlay,
    OverlayConfigUpdate,
    OverlayData,
    OverlayPosition,
    PersonalOverlay,
    PersonalStats,
    PlayerContribution,
    PlayerRole,
    RaidEffect,
    RaidFrame,
    RaidFrameData,
    RaidGridLayout,
    RaidOverlay,
    RaidOverlayConfig,
    RaidRegistryAction,
    SwapState,
    AbilityQueueConfig,
    AbilityQueueData,
    AbilityQueueEntry,
    AbilityQueueOverlay,
    TimerData,
    TimerEntry,
    TimerOverlay,
};
pub use platform::{
    MonitorInfo, NativeOverlay, OverlayConfig, OverlayPlatform, PlatformError, VirtualScreenBounds,
    clamp_to_virtual_screen, find_monitor_at, find_monitor_by_id, get_all_monitors,
    resolve_absolute_position,
};
pub use renderer::Renderer;
pub use utils::{color_from_rgba, format_number, format_time, truncate_name};
pub use widgets::{Footer, Header, LabeledValue, ProgressBar, colors};

// Re-export tiny_skia Color for external use
pub use tiny_skia::Color;
