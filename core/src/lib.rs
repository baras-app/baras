pub mod audio;
pub mod boss;
pub mod combat_log;
pub mod context;
pub mod debug_log;
pub mod effects;
pub mod encounter;
pub mod entity_filter;
pub mod signal_processor;
pub mod game_data;
pub mod query;
pub mod serde_defaults;
pub mod state;
pub mod storage;
pub mod timers;
pub mod triggers;

// Backward compatibility aliases
pub use game_data as swtor_data;

// Re-exports for convenience
pub use combat_log::*;
pub use context::watcher as directory_watcher;
pub use effects::{ActiveEffect, DefinitionConfig, DefinitionSet, EffectCategory, EffectDefinition, EffectTracker, NewTargetInfo};
pub use entity_filter::EntityFilter;
pub use encounter::metrics::PlayerMetrics;
pub use encounter::summary::{EncounterSummary, EncounterHistory};
pub use encounter::{PhaseType, BossHealthEntry, CombatEncounter, ProcessingMode, ActiveBoss};
pub use signal_processor::{EventProcessor, GameSignal, SignalHandler};
pub use game_data::*;
pub use state::SessionCache;
pub use timers::{ActiveTimer, TimerDefinition, TimerKey, TimerManager, TimerTrigger};
pub use triggers::{AbilitySelector, EffectSelector, EntitySelector};
pub use audio::AudioConfig;
pub use query::{EncounterQuery, AbilityBreakdown, EntityBreakdown, TimeSeriesPoint};
pub use boss::{
    BossConfig, BossEncounterDefinition, BossTimerDefinition, CounterCondition,
    CounterDefinition, PhaseDefinition, PhaseTrigger, load_bosses_from_dir,
};
