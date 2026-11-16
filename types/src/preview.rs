use serde::{Deserialize, Serialize};

/// Overlay a previewed timer/effect entry is shown on
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PreviewTarget {
    TimersA,
    TimersB,
    EffectsA,
    EffectsB,
    EffectsC,
    Cooldowns,
    CooldownsB,
    DotTracker,
    RaidFrames,
    BossHealth,
    Alerts,
}

impl PreviewTarget {
    pub const COUNT: usize = 11;

    pub fn label(&self) -> &'static str {
        match self {
            Self::TimersA => "Timers A",
            Self::TimersB => "Timers B",
            Self::EffectsA => "Effects A",
            Self::EffectsB => "Effects B",
            Self::EffectsC => "Effects C",
            Self::Cooldowns => "Cooldowns",
            Self::CooldownsB => "Cooldowns B",
            Self::DotTracker => "DOT Tracker",
            Self::RaidFrames => "Raid Frames",
            Self::BossHealth => "Boss Health",
            Self::Alerts => "Alerts",
        }
    }
}

/// An unsaved timer/effect draft reduced to what the overlays render
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OverlayPreviewRequest {
    /// Display text (placeholders unresolved)
    pub text: String,
    /// Alert text for the `Alerts` target (placeholders unresolved)
    pub alert_text: Option<String>,
    /// When the `Alerts` target shows its text
    pub alert_timing: PreviewAlertTiming,
    pub duration_secs: f32,
    pub color: [u8; 4],
    pub icon_ability_id: Option<u64>,
    pub show_icon: bool,
    pub display_source: bool,
    pub show_target: bool,
    pub show_single_stack: bool,
    pub show_countdown: bool,
    pub stack_priority: bool,
    /// Sample stack count; `None` for entries without stacks (timers)
    pub stacks: Option<u8>,
    pub targets: Vec<PreviewTarget>,
    /// Audio cues to play at their real timing; `None` when audio is disabled
    pub audio: Option<PreviewAudio>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreviewAudio {
    pub file: Option<String>,
    /// Seconds before expiry the sound plays (0 = on expiry)
    pub offset: u8,
    /// Voice countdown starts at this many seconds remaining (0 = off)
    pub countdown_start: u8,
    pub countdown_voice: Option<String>,
    /// Instant alert: the sound plays immediately, there is no countdown
    pub instant: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum PreviewAlertTiming {
    Start,
    Expire,
    /// Live "text (X.X)" countdown over the last N seconds
    Countdown(f32),
}
