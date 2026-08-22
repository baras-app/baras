pub mod combat_log;
pub mod context;
pub mod debug_log;
pub mod dsl;
pub mod effects;
pub mod encounter;
pub mod game_data;
pub mod icons;
#[cfg(feature = "query")]
pub mod query;
pub mod raid_detect;
pub mod serde_defaults;
pub mod signal_processor;
pub mod state;
pub mod storage;
pub mod timers;

// Backward compatibility aliases
pub use dsl as boss;
pub use game_data as swtor_data; // Alias for backward compatibility

// Re-exports for convenience
pub use combat_log::*;
pub use context::watcher as directory_watcher;
pub use dsl::AudioConfig;
pub use dsl::EntityFilter;
pub use dsl::{AbilitySelector, EffectSelector, EntitySelector};
pub use dsl::{
    BossConfig, BossEncounterDefinition, BossTimerDefinition, CounterCondition, CounterDefinition,
    OperationTimerStart, PhaseDefinition, PhaseTrigger, load_bosses_from_dir,
};
pub use effects::{
    ActiveEffect, DefinitionConfig, DefinitionSet, DisplayTarget, EFFECTS_DSL_VERSION,
    EffectDefinition, EffectTracker, NewTargetInfo,
};
pub use encounter::metrics::{IncomingDamageRow, PlayerMetrics};
pub use encounter::summary::{EncounterHistory, EncounterSummary};
pub use encounter::{ActiveBoss, CombatEncounter, OverlayHealthEntry, PhaseType, ProcessingMode};
pub use game_data::*;
pub use icons::{IconRegistry, TICK_BIAS_SECS, calculate_effect_duration};
#[cfg(feature = "query")]
pub use query::{AbilityBreakdown, EncounterQuery, EntityBreakdown, TimeSeriesPoint};
pub use signal_processor::{EventProcessor, GameSignal, SignalHandler};
pub use state::SessionCache;
pub use timers::{ActiveTimer, TimerDefinition, TimerKey, TimerManager, TimerTrigger};
