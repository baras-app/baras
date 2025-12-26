pub mod boss;
pub mod combat_log;
pub mod context;
pub mod effects;
pub mod encounter;
pub mod events;
pub mod game_data;
pub mod state;
pub mod timers;

// Backward compatibility aliases
pub use game_data as swtor_data;

// Re-exports for convenience
pub use combat_log::*;
pub use context::watcher as directory_watcher;
pub use effects::{ActiveEffect, DefinitionConfig, DefinitionSet, EffectCategory, EffectDefinition, EffectTracker, EntityFilter, NewTargetInfo};
pub use encounter::metrics::PlayerMetrics;
pub use encounter::summary::{EncounterSummary, EncounterHistory};
pub use encounter::{PhaseType, BossHealthEntry};
pub use events::{EventProcessor, GameSignal, SignalHandler};
pub use game_data::*;
pub use state::SessionCache;
pub use timers::{ActiveTimer, TimerDefinition, TimerKey, TimerManager, TimerTrigger};
pub use boss::{
    BossConfig, BossEncounterDefinition, BossEncounterState, BossTimerDefinition, CounterCondition,
    CounterDefinition, PhaseDefinition, PhaseTrigger, load_bosses_from_dir,
};

// Backward compatibility - re-export deprecated alias
#[allow(deprecated)]
pub use boss::BossDefinition;
