//! Lifecycle state of an `ActiveEffect`.

/// Where an effect is between creation and being dropped by `tick()`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Lifecycle {
    /// Shown and counting down
    #[default]
    Live,
    /// Past its duration estimate: hidden, but a refresh can still revive it
    /// until the grace period ends (the in-game effect may outlast our timer)
    TimerExpired,
    /// Ended (game removal, cancel or grace GC); dropped on the next tick
    Removed,
    /// Wiped by tracker housekeeping (combat end, zone change, death) with its
    /// end alert and sound suppressed. Holds the alert/audio flags it
    /// overrode, restored if the game confirms the removal before the drop.
    Discarded { alert_fired: bool, audio_played: bool },
}
