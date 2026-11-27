//! `FiredAlert` builders for effect definitions.

use chrono::NaiveDateTime;

use super::log_names::LogNames;
use super::{AlertTrigger, EffectDefinition};
use crate::timers::FiredAlert;

/// Text alert for `def` without audio; callers adjust fields as needed
pub(super) fn text_alert(def: &EffectDefinition, text: String, timestamp: NaiveDateTime) -> FiredAlert {
    FiredAlert {
        id: def.id.clone(),
        name: def.name.clone(),
        text,
        color: def.color,
        timestamp,
        alert_text_enabled: true,
        audio_enabled: false,
        audio_file: None,
        icon_ability_id: def.icon_ability_id,
        remaining_secs: None,
    }
}

/// Instant alert (`is_alert`, no active effect created).
///
/// Without `alert_text` only audio fires: the text still carries the
/// definition name for the TTS fallback but is not shown.
pub(super) fn instant(def: &EffectDefinition, timestamp: NaiveDateTime, names: LogNames, stacks: u8) -> FiredAlert {
    let raw_text = def.alert_text.as_deref().unwrap_or(&def.name);
    FiredAlert {
        alert_text_enabled: def.alert_text.is_some(),
        audio_enabled: def.audio.enabled,
        audio_file: def.audio.file.clone(),
        ..text_alert(def, names.format(raw_text, stacks), timestamp)
    }
}

/// On-apply alert for a created or refreshed effect, if configured
pub(super) fn on_apply(
    def: &EffectDefinition,
    timestamp: NaiveDateTime,
    names: LogNames,
    stacks: u8,
) -> Option<FiredAlert> {
    if def.alert_on != AlertTrigger::OnApply {
        return None;
    }
    let text = names.format(def.alert_text.as_ref()?, stacks);
    Some(text_alert(def, text, timestamp))
}
