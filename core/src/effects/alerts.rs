//! `FiredAlert` builders for effect definitions.

use chrono::NaiveDateTime;

use super::{AlertTrigger, EffectDefinition};
use crate::context::IStr;
use crate::timers::FiredAlert;

/// Resolve `{source}`, `{target}` and `{stack}` in alert text
pub(super) fn format_text(text: &str, source_name: IStr, target_name: IStr, stacks: u8) -> String {
    crate::dsl::format_placeholders(
        text,
        crate::context::resolve(source_name),
        crate::context::resolve(target_name),
        Some(stacks),
    )
}

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
pub(super) fn instant(
    def: &EffectDefinition,
    timestamp: NaiveDateTime,
    source_name: IStr,
    target_name: IStr,
    stacks: u8,
) -> FiredAlert {
    let raw_text = def.alert_text.as_deref().unwrap_or(&def.name);
    FiredAlert {
        alert_text_enabled: def.alert_text.is_some(),
        audio_enabled: def.audio.enabled,
        audio_file: def.audio.file.clone(),
        ..text_alert(def, format_text(raw_text, source_name, target_name, stacks), timestamp)
    }
}

/// On-apply alert for a created or refreshed effect, if configured
pub(super) fn on_apply(
    def: &EffectDefinition,
    timestamp: NaiveDateTime,
    source_name: IStr,
    target_name: IStr,
    stacks: u8,
) -> Option<FiredAlert> {
    if def.alert_on != AlertTrigger::OnApply {
        return None;
    }
    let text = format_text(def.alert_text.as_ref()?, source_name, target_name, stacks);
    Some(text_alert(def, text, timestamp))
}
