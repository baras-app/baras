pub mod combat_log;
pub mod context;
pub mod debug_log;
pub mod dsl;
pub mod effects;
pub mod encounter;
pub mod game_data;
pub mod icons;
pub mod query;
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
    PhaseDefinition, PhaseTrigger, load_bosses_from_dir,
};
pub use effects::{
    ActiveEffect, DefinitionConfig, DefinitionSet, DisplayTarget, EffectCategory,
    EffectDefinition, EffectTracker, NewTargetInfo, EFFECTS_DSL_VERSION,
};
pub use encounter::metrics::PlayerMetrics;
pub use encounter::summary::{EncounterHistory, EncounterSummary};
pub use encounter::{ActiveBoss, OverlayHealthEntry, CombatEncounter, PhaseType, ProcessingMode};
pub use game_data::*;
pub use query::{AbilityBreakdown, EncounterQuery, EntityBreakdown, TimeSeriesPoint};
pub use signal_processor::{EventProcessor, GameSignal, SignalHandler};
pub use state::SessionCache;
pub use timers::{ActiveTimer, TimerDefinition, TimerKey, TimerManager, TimerTrigger};
pub use icons::{IconRegistry, calculate_effect_duration, TICK_BIAS_SECS};
