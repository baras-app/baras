pub mod combat_log;
pub mod context;
pub mod debug_log;
pub mod dsl;
pub mod effects;
pub mod encounter;
pub mod game_data;
pub mod query;
pub mod serde_defaults;
pub mod signal_processor;
pub mod state;
pub mod storage;
pub mod timers;

// Backward compatibility aliases
pub use game_data as swtor_data;
pub use dsl as boss;  // Alias for backward compatibility

// Re-exports for convenience
pub use combat_log::*;
pub use context::watcher as directory_watcher;
pub use effects::{ActiveEffect, DefinitionConfig, DefinitionSet, EffectCategory, EffectDefinition, EffectTracker, NewTargetInfo};
pub use dsl::EntityFilter;
pub use encounter::metrics::PlayerMetrics;
pub use encounter::summary::{EncounterSummary, EncounterHistory};
pub use encounter::{PhaseType, BossHealthEntry, CombatEncounter, ProcessingMode, ActiveBoss};
pub use signal_processor::{EventProcessor, GameSignal, SignalHandler};
pub use game_data::*;
pub use state::SessionCache;
pub use timers::{ActiveTimer, TimerDefinition, TimerKey, TimerManager, TimerTrigger};
pub use dsl::{AbilitySelector, EffectSelector, EntitySelector};
pub use dsl::AudioConfig;
pub use query::{EncounterQuery, AbilityBreakdown, EntityBreakdown, TimeSeriesPoint};
pub use dsl::{
    BossConfig, BossEncounterDefinition, BossTimerDefinition, CounterCondition,
    CounterDefinition, PhaseDefinition, PhaseTrigger, load_bosses_from_dir,
};
