//! Effect edit form, shown in the editor drawer

use dioxus::prelude::*;

use super::discipline_selector::DisciplineSelector;
use super::effect_editor::effect_origin;
use super::effect_trigger::{
    RefreshAbilitiesEditor, TriggerAbilitiesEditor, effect_icon_id, get_effect_when_label, get_trigger_abilities,
    get_trigger_effects, get_trigger_filters, set_trigger_abilities, set_trigger_effects, set_trigger_source,
    set_trigger_target,
};
use super::encounter_editor::triggers::{EffectSelectorEditor, EntityFilterDropdown};
use super::sound_picker::SoundPicker;
use crate::api;
use crate::components::editor_common::{FormCard, ItemActions};
use crate::components::form_fields::{CheckField, Field, NumberField, SubSection, TextField, TogglePill};
use crate::types::{
    AlertTrigger, DisplayTarget, EffectListItem, EntityFilter, RefreshAbility, RefreshScope, Trigger, effect_alert_label,
};
use crate::utils::parse_hex_color;

/// UI-level trigger type for effect tracking
#[derive(Clone, Copy, PartialEq, Default)]
enum EffectTriggerType {
    /// Track based on game effect applied/removed
    #[default]
    EffectBased,
    /// Track based on ability cast (for procs/cooldowns)
    AbilityCast,
    /// Track based on damage taken from an ability
    DamageTaken,
    /// Track based on healing taken from an ability
    HealingTaken,
}

impl EffectTriggerType {
    fn label(&self) -> &'static str {
        match self {
            Self::EffectBased => "Effect Based",
            Self::AbilityCast => "Ability Cast",
            Self::DamageTaken => "Damage Taken",
            Self::HealingTaken => "Healing Taken",
        }
    }

    fn all() -> &'static [Self] {
        &[Self::EffectBased, Self::AbilityCast, Self::DamageTaken, Self::HealingTaken]
    }

    /// Determine trigger type from effect data
    fn from_effect(effect: &EffectListItem) -> Self {
        match &effect.trigger {
            Trigger::AbilityCast { .. } => Self::AbilityCast,
            Trigger::DamageTaken { .. } => Self::DamageTaken,
            Trigger::HealingTaken { .. } => Self::HealingTaken,
            _ => Self::EffectBased,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Effect Edit Form
// ─────────────────────────────────────────────────────────────────────────────

const ALERT_TEXT_HELP: &str = "Text shown in the alert notification. Leave blank for no alert. {source} and {target} will show the source/target names if available";

const REFRESH_HELP: &str = "Abilities that refresh this effect's duration. Per ability: 'Min Stacks' only refreshes when the effect has at least that many stacks. Trigger 'On Cast' refreshes the moment the ability is used; 'On Heal' waits for a cast-time heal to land; 'On Damage' refreshes on any damage dealt by the ability. For 'On Cast', the 'Refresh on' control chooses Cast vs First Damage timing — 'Default' defers DoT-tracker effects to the ability's first damaging hit and refreshes everything else immediately.";

/// Overlays drawn with personal icons (Effects A/B/C)
fn on_effects_overlay(targets: &[DisplayTarget]) -> bool {
    targets.iter().any(|t| matches!(t, DisplayTarget::EffectsA | DisplayTarget::EffectsB | DisplayTarget::EffectsC))
}

fn on_cooldowns(targets: &[DisplayTarget]) -> bool {
    targets.iter().any(|t| matches!(t, DisplayTarget::Cooldowns | DisplayTarget::CooldownsB))
}

fn refresh_scope_label(scope: RefreshScope) -> &'static str {
    match scope {
        RefreshScope::Both => "Both",
        RefreshScope::Source => "Source",
        RefreshScope::Target => "Target",
    }
}

#[component]
pub(crate) fn EffectEditForm(
    effect: EffectListItem,
    #[props(default = false)] is_draft: bool,
    #[props(default = false)] is_bundled: bool,
    #[props(default = false)] is_user_override: bool,
    on_save: EventHandler<EffectListItem>,
    on_delete: EventHandler<()>,
    on_duplicate: EventHandler<()>,
    #[props(default)] on_export: EventHandler<()>,
    #[props(default)] on_dirty: EventHandler<bool>,
) -> Element {
    let effect_for_draft = effect.clone();
    let effect_for_trigger = effect.clone();
    let effect_original = effect.clone();
    let mut draft = use_signal(|| effect_for_draft);
    let mut trigger_type = use_signal(|| EffectTriggerType::from_effect(&effect_for_trigger));
    let mut icon_preview_url = use_signal(|| None::<String>);

    // Track if form was just saved (resets dirty state)
    let mut just_saved = use_signal(|| false);

    // Load icon preview - use explicit icon_ability_id, or fall back to trigger ID
    use_effect(move || {
        let current_draft = draft(); // Read inside effect for reactivity
        let preview_id = effect_icon_id(current_draft.icon_ability_id, &current_draft.trigger);

        if let Some(ability_id) = preview_id {
            spawn(async move {
                if let Some(url) = api::get_icon_preview(ability_id).await {
                    icon_preview_url.set(Some(url));
                } else {
                    icon_preview_url.set(None);
                }
            });
        } else {
            icon_preview_url.set(None);
        }
    });

    // Reset just_saved when user makes new changes after saving
    let effect_original_for_effect = effect_original.clone();
    use_effect(move || {
        if draft() != effect_original_for_effect && just_saved() {
            just_saved.set(false);
        }
    });

    // For drafts, always enable save; for existing effects, only when changed
    let has_changes = use_memo(move || is_draft || (!just_saved() && draft() != effect_original));

    // Notify parent when dirty state changes
    use_effect(move || {
        on_dirty.call(has_changes());
    });

    let color = draft().color.unwrap_or([128, 128, 128, 255]);
    let color_hex = format!("#{:02x}{:02x}{:02x}", color[0], color[1], color[2]);
    let targets = draft().display_targets;

    rsx! {
        div { class: "list-item-body effect-edit-form",
            div { class: "edit-grid left-66",
                // ═══ LEFT COLUMN: Identity, Trigger ══════════════════════════════
                div { class: "edit-grid-left",
                    FormCard { icon: "fa-tag", title: "Identity",
                        // Read-only id; drafts have none yet
                        if !is_draft {
                            Field { label: "Effect ID",
                                code { class: "id-display", "{effect.id}" }
                            }
                        }
                        TextField { label: "Name", value: draft().name, on_input: move |v| draft.write().name = v }
                        if !draft().is_alert {
                            Field { label: "Display Text",
                                input {
                                    r#type: "text",
                                    placeholder: "{draft().name}",
                                    value: "{draft().display_text.unwrap_or_default()}",
                                    oninput: move |e| {
                                        let v = e.value();
                                        draft.write().display_text = (!v.is_empty()).then_some(v);
                                    }
                                }
                            }
                            Field {
                                label: "Display Overlays",
                                help: "Overlays this effect will appear on. Select multiple to show the same effect on more than one overlay.",
                                for target in DisplayTarget::all().iter().filter(|t| !matches!(t, DisplayTarget::None)).copied() {
                                    label { class: "field-check",
                                        input {
                                            r#type: "checkbox",
                                            checked: draft().display_targets.contains(&target),
                                            onchange: move |e| {
                                                let mut d = draft.write();
                                                d.display_targets.retain(|t| *t != target);
                                                if e.checked() {
                                                    d.display_targets.push(target);
                                                }
                                            }
                                        }
                                        "{target.label()}"
                                    }
                                }
                            }
                        }
                        Field {
                            label: "Your Disciplines",
                            help: "Only activate this effect when your local player is one of the selected disciplines. If empty, the effect applies to all disciplines.",
                            DisciplineSelector {
                                selected: draft().disciplines.clone(),
                                on_change: move |v: Vec<String>| draft.write().disciplines = v,
                            }
                        }
                        Field { label: "Color",
                            input {
                                r#type: "color",
                                value: "{color_hex}",
                                class: "color-picker",
                                oninput: move |e| {
                                    if let Some(c) = parse_hex_color(&e.value()) {
                                        draft.write().color = Some(c);
                                    }
                                }
                            }
                        }
                        if !draft().is_alert {
                            Field { label: "Icon ID", help: "Ability ID to use for the icon. Leave blank to auto-detect from trigger.",
                                if let Some(ref url) = icon_preview_url() {
                                    img { src: "{url}", class: "icon-preview", width: "24", height: "24", alt: "Icon preview" }
                                }
                                input {
                                    r#type: "text",
                                    placeholder: "(auto)",
                                    value: "{draft().icon_ability_id.map(|id| id.to_string()).unwrap_or_default()}",
                                    oninput: move |e| draft.write().icon_ability_id = e.value().parse::<u64>().ok(),
                                }
                                if icon_preview_url().is_none() && draft().icon_ability_id.is_some() {
                                    span { class: "field-unit", "(not found)" }
                                }
                            }
                        }
                        CheckField { label: "Entry enabled", checked: draft().enabled, on_change: move |v| draft.write().enabled = v }

                        if !draft().is_alert {
                            SubSection { title: "Display Options",
                                CheckField { label: "Show icon", checked: draft().show_icon, on_change: move |v| draft.write().show_icon = v }
                                // Source / target names: personal overlays only
                                if on_effects_overlay(&targets) || on_cooldowns(&targets) {
                                    CheckField {
                                        label: "Display source",
                                        help: "Show who applied this effect on the overlay",
                                        checked: draft().display_source,
                                        on_change: move |v| draft.write().display_source = v,
                                    }
                                    CheckField {
                                        label: "Display target",
                                        help: "Show who this effect targets (hidden when the target is the source)",
                                        checked: draft().show_target,
                                        on_change: move |v| draft.write().show_target = v,
                                    }
                                }
                                if on_effects_overlay(&targets) || targets.contains(&DisplayTarget::RaidFrames) {
                                    CheckField {
                                        label: "Show stack count at 1",
                                        help: "Always display a stack count for this effect, starting at 1. Non-stacking effects show \"1\"; real counts show as they climb. Raid frames normally hide counts below 2.",
                                        checked: draft().show_single_stack,
                                        on_change: move |v| draft.write().show_single_stack = v,
                                    }
                                }
                                if on_effects_overlay(&targets) {
                                    CheckField {
                                        label: "Show countdown",
                                        help: "Show the remaining-time text on this effect's icon. Uncheck to hide the countdown for just this effect. The overlay's own 'Show countdown' setting must also be on.",
                                        checked: draft().show_countdown,
                                        on_change: move |v| draft.write().show_countdown = v,
                                    }
                                    CheckField {
                                        label: "Emphasize stacks",
                                        help: "Draw this effect's stack count large and centered with the countdown in the corner. Works even when the overlay's own 'Emphasize stacked effects' setting is off.",
                                        checked: draft().stack_priority,
                                        on_change: move |v| draft.write().stack_priority = v,
                                    }
                                    CheckField {
                                        label: "Show when absent",
                                        help: "Always show this effect's icon when playing a matching discipline: grayed out while the effect is inactive, colored while active (e.g. tank guard, sniper cover). Requires the discipline to be detected.",
                                        checked: draft().track_uptime,
                                        on_change: move |v: bool| {
                                            let mut d = draft.write();
                                            d.track_uptime = v;
                                            if !v {
                                                d.track_uptime_after_first = false;
                                            }
                                        },
                                    }
                                    if draft().track_uptime {
                                        CheckField { nested: true,
                                            label: "Only show after first appearance",
                                            help: "Hide the grayed-out icon until this effect has been applied at least once this session. After that, it persists as usual while the effect is inactive.",
                                            checked: draft().track_uptime_after_first,
                                            on_change: move |v| draft.write().track_uptime_after_first = v,
                                        }
                                    }
                                }
                            }
                        }
                    }

                    FormCard { icon: "fa-bolt", title: "Trigger",
                        SubSection {
                            title: "Trigger",
                            help: "How this effect activates: Effect-based tracks game buffs/debuffs, Ability-based tracks when abilities are cast",
                            Field { label: "Type",
                                select {
                                    value: "{trigger_type().label()}",
                                    onchange: move |e| {
                                        let new_type = match e.value().as_str() {
                                            "Effect Based" => EffectTriggerType::EffectBased,
                                            "Ability Cast" => EffectTriggerType::AbilityCast,
                                            "Damage Taken" => EffectTriggerType::DamageTaken,
                                            "Healing Taken" => EffectTriggerType::HealingTaken,
                                            _ => trigger_type(),
                                        };
                                        trigger_type.set(new_type);
                                        let mut d = draft.write();
                                        // Convert trigger to new type, preserving source/target
                                        let (source, target) = get_trigger_filters(&d.trigger);
                                        d.trigger = match new_type {
                                            EffectTriggerType::EffectBased => Trigger::EffectApplied { effects: vec![], source, target, position: vec![] },
                                            EffectTriggerType::AbilityCast => Trigger::AbilityCast { abilities: vec![], source, target, position: vec![] },
                                            EffectTriggerType::DamageTaken => Trigger::DamageTaken { abilities: vec![], source, target, mitigation: vec![], position: vec![] },
                                            EffectTriggerType::HealingTaken => Trigger::HealingTaken { abilities: vec![], source, target, position: vec![] },
                                        };
                                    },
                                    for tt in EffectTriggerType::all() {
                                        option { value: "{tt.label()}", "{tt.label()}" }
                                    }
                                }
                            }
                            if trigger_type() == EffectTriggerType::EffectBased {
                                Field { label: "When",
                                    select {
                                        value: "{get_effect_when_label(&draft().trigger)}",
                                        onchange: move |e| {
                                            let mut d = draft.write();
                                            let (_, effects) = get_trigger_effects(&d.trigger);
                                            let (source, target) = get_trigger_filters(&d.trigger);
                                            match e.value().as_str() {
                                                "Effect Applied" => d.trigger = Trigger::EffectApplied { effects, source, target, position: vec![] },
                                                "Effect Removed" => d.trigger = Trigger::EffectRemoved { effects, source, target, position: vec![] },
                                                _ => {}
                                            }
                                        },
                                        option { value: "Effect Applied", "Effect Applied" }
                                        option { value: "Effect Removed", "Effect Removed" }
                                    }
                                }
                            }
                            Field { label: "Source", help: "Who must cast/apply for this effect to trigger (e.g., Local Player = you, Any = anyone)",
                                EntityFilterDropdown {
                                    label: "",
                                    value: get_trigger_filters(&draft().trigger).0.clone(),
                                    options: EntityFilter::source_options(),
                                    on_change: move |f| {
                                        let mut d = draft.write();
                                        d.trigger = set_trigger_source(d.trigger.clone(), f);
                                    }
                                }
                            }
                            Field { label: "Target", help: "Who must receive this effect for it to trigger (e.g., Any = track on anyone, Local Player = only on you)",
                                EntityFilterDropdown {
                                    label: "",
                                    value: get_trigger_filters(&draft().trigger).1.clone(),
                                    options: EntityFilter::target_options(),
                                    on_change: move |f| {
                                        let mut d = draft.write();
                                        d.trigger = set_trigger_target(d.trigger.clone(), f);
                                    }
                                }
                            }
                            Field { label: "Source Disc.", help: "Restrict the source to players of these disciplines/roles. If empty, no restriction. When set, NPCs, companions, and players whose discipline isn't known yet never match.",
                                DisciplineSelector {
                                    selected: draft().source_disciplines.clone(),
                                    include_roles: true,
                                    empty_label: "Any discipline",
                                    on_change: move |v: Vec<String>| draft.write().source_disciplines = v,
                                }
                            }
                            Field { label: "Target Disc.", help: "Restrict the target to players of these disciplines/roles. If empty, no restriction. When set, NPCs, companions, and players whose discipline isn't known yet never match.",
                                DisciplineSelector {
                                    selected: draft().target_disciplines.clone(),
                                    include_roles: true,
                                    empty_label: "Any discipline",
                                    on_change: move |v: Vec<String>| draft.write().target_disciplines = v,
                                }
                            }
                        }

                        // Effects or trigger abilities, by trigger type
                        if trigger_type() == EffectTriggerType::EffectBased {
                            SubSection { title: "Effects",
                                EffectSelectorEditor {
                                    label: "",
                                    selectors: get_trigger_effects(&draft().trigger).1,
                                    on_change: move |selectors| {
                                        let mut d = draft.write();
                                        d.trigger = set_trigger_effects(d.trigger.clone(), selectors);
                                    }
                                }
                            }
                        } else {
                            SubSection { title: "Trigger Abilities",
                                TriggerAbilitiesEditor {
                                    abilities: get_trigger_abilities(&draft().trigger),
                                    on_change: move |abilities| {
                                        let mut d = draft.write();
                                        d.trigger = set_trigger_abilities(d.trigger.clone(), abilities);
                                    }
                                }
                            }
                        }

                        if !draft().is_alert {
                            SubSection { title: "Refresh", help: REFRESH_HELP,
                                RefreshAbilitiesEditor {
                                    abilities: draft().refresh_abilities.clone(),
                                    on_change: move |abilities: Vec<RefreshAbility>| draft.write().refresh_abilities = abilities,
                                }
                                Field {
                                    label: "Refresh Scope",
                                    help: "Which (source, target) axis identifies the same effect for refresh / Ignore Refreshes. Both = per (source, target) (default). Source = collapse across targets (e.g. one HealingTaken timer regardless of who was healed). Target = collapse across sources (e.g. one debuff entry regardless of which player applied it).",
                                    for scope in RefreshScope::all().iter().copied() {
                                        TogglePill {
                                            label: refresh_scope_label(scope),
                                            active: scope == draft().refresh_scope,
                                            on_click: move |_| draft.write().refresh_scope = scope,
                                        }
                                    }
                                }
                            }

                            SubSection { title: "Behavior",
                                CheckField {
                                    label: "AoE refresh",
                                    help: "Use damage correlation to detect multi-target refreshes instead of requiring individual ApplyEffect signals per target",
                                    checked: draft().is_aoe_refresh,
                                    on_change: move |v: bool| {
                                        let mut d = draft.write();
                                        d.is_aoe_refresh = v;
                                        if !v {
                                            d.aoe_refresh_immediate = false;
                                        }
                                    },
                                }
                                if draft().is_aoe_refresh {
                                    CheckField { nested: true,
                                        label: "Immediate",
                                        help: "Off: only refreshes targets hit at the same time as the primary target (use for DOTs like Corrosive Grenade where ongoing ticks would cause false refreshes). On: any damage hit from the ability refreshes that target.",
                                        checked: draft().aoe_refresh_immediate,
                                        on_change: move |v| draft.write().aoe_refresh_immediate = v,
                                    }
                                }
                                CheckField {
                                    label: "Refresh duration when charges modified",
                                    help: "Reset timer when effect stacks change",
                                    checked: draft().is_refreshed_on_modify,
                                    on_change: move |v| draft.write().is_refreshed_on_modify = v,
                                }
                                CheckField {
                                    label: "Ignore refreshes",
                                    help: "When the effect is already active, do not reset its duration if the trigger fires again.",
                                    checked: draft().ignore_refreshes,
                                    on_change: move |v| draft.write().ignore_refreshes = v,
                                }
                                CheckField {
                                    label: "Persist past death",
                                    help: "Keep showing effect after player dies",
                                    checked: draft().persist_past_death,
                                    on_change: move |v| draft.write().persist_past_death = v,
                                }
                                CheckField {
                                    label: "Track outside combat",
                                    help: "Continue tracking this effect between combat encounters",
                                    checked: draft().track_outside_combat,
                                    on_change: move |v| draft.write().track_outside_combat = v,
                                }
                            }
                        }
                    }
                }

                // ═══ RIGHT COLUMN: Timing, Alerts, Audio ═════════════════════════
                div { class: "edit-grid-right",
                    FormCard { icon: "fa-clock", title: "Timing",
                        CheckField {
                            label: "Instant alert only",
                            help: "Shows a brief alert notification instead of tracking the effect. No duration, no overlay bar/icon — only alert text and audio fire on trigger.",
                            checked: draft().is_alert,
                            on_change: move |v| draft.write().is_alert = v,
                        }
                        if !draft().is_alert {
                            CheckField {
                                label: "Duration affected by alacrity",
                                help: "Adjusts the effect duration based on the player's alacrity stat",
                                checked: draft().is_affected_by_alacrity,
                                on_change: move |v| draft.write().is_affected_by_alacrity = v,
                            }
                            // Cooldowns always ignore effect removed, so they don't get the choice
                            if !on_cooldowns(&targets) {
                                CheckField {
                                    label: "Fixed duration",
                                    help: "Use the duration timer instead of tracking when the game removes the effect",
                                    checked: draft().ignore_effect_removed,
                                    on_change: move |v| draft.write().ignore_effect_removed = v,
                                }
                            }
                            NumberField {
                                label: "Duration",
                                help: "How long the effect displays (seconds). Set to 0 for effects that track via game events",
                                value: draft().duration_secs.unwrap_or(0.0).to_string(),
                                unit: "sec",
                                min: "0",
                                on_input: move |v: String| draft.write().duration_secs = v.parse::<f32>().ok().filter(|&v| v > 0.0),
                            }
                            NumberField {
                                label: "Show at",
                                help: "Only display the effect when this many seconds remain. 0 = always visible",
                                value: draft().show_at_secs.to_string(),
                                unit: "sec remaining",
                                min: "0",
                                max: draft().duration_secs.unwrap_or(999.0).to_string(),
                                on_input: move |v: String| if let Ok(v) = v.parse::<f32>() {
                                    let mut d = draft.write();
                                    d.show_at_secs = v.min(d.duration_secs.unwrap_or(f32::MAX)).max(0.0);
                                },
                            }
                            // Cooldowns always ignore effect removed
                            if on_cooldowns(&targets) {
                                NumberField {
                                    label: "Ready State",
                                    help: "Seconds before expiration to show the cooldown as ready",
                                    value: draft().cooldown_ready_secs.to_string(),
                                    unit: "sec",
                                    min: "0",
                                    step: "0.1",
                                    on_input: move |v: String| if let Ok(v) = v.parse::<f32>() { draft.write().cooldown_ready_secs = v.max(0.0) },
                                }
                            }
                        }
                    }

                    FormCard { icon: "fa-bell", title: "Alerts",
                        TextField {
                            label: "Alert Text",
                            help: ALERT_TEXT_HELP,
                            value: draft().alert_text.unwrap_or_default(),
                            placeholder: "(none)",
                            on_input: move |v: String| draft.write().alert_text = (!v.is_empty()).then_some(v),
                        }
                        if draft().is_alert {
                            Field { label: "Alert On", help: "Instant alerts always fire when triggered",
                                span { class: "field-unit", "On trigger (instant)" }
                            }
                        } else {
                            Field { label: "Alert On", help: "When to show alert text: on effect start, on effect end, or never",
                                select {
                                    value: "{effect_alert_label(&draft().alert_on)}",
                                    onchange: move |e| {
                                        let mut d = draft.write();
                                        d.alert_on = match e.value().as_str() {
                                            "Effect Start" => AlertTrigger::OnApply,
                                            "Effect End" => AlertTrigger::OnExpire,
                                            "Countdown" => AlertTrigger::Countdown,
                                            _ => AlertTrigger::None,
                                        };
                                        // Seed a sensible default the first time Countdown is selected
                                        if d.alert_on == AlertTrigger::Countdown && d.alert_countdown_secs.is_none() {
                                            d.alert_countdown_secs = Some(3.0);
                                        }
                                    },
                                    for trigger in AlertTrigger::all() {
                                        option {
                                            value: "{effect_alert_label(trigger)}",
                                            selected: *trigger == draft().alert_on,
                                            "{effect_alert_label(trigger)}"
                                        }
                                    }
                                }
                            }
                            if draft().alert_on == AlertTrigger::Countdown {
                                NumberField {
                                    label: "Countdown Window",
                                    help: "Show a live-updating alert during the last N seconds before the effect expires (0-10). Alert text shows remaining time to one decimal.",
                                    value: draft().alert_countdown_secs.unwrap_or(3.0).to_string(),
                                    unit: "sec",
                                    min: "0",
                                    max: "10",
                                    step: "0.5",
                                    on_input: move |v: String| if let Ok(v) = v.parse::<f32>() {
                                        draft.write().alert_countdown_secs = Some(v.clamp(0.0, 10.0));
                                    },
                                }
                            }
                        }
                    }

                    FormCard { icon: "fa-volume-up", title: "Audio",
                        CheckField { label: "Enable audio", checked: draft().audio.enabled, on_change: move |v| draft.write().audio.enabled = v }
                        if draft().audio.enabled {
                            SoundPicker { value: draft().audio.file.clone(), on_change: move |v| draft.write().audio.file = v }
                            if !draft().is_alert {
                                Field { label: "Audio Offset", help: "When to play the sound relative to effect expiration",
                                    select {
                                        value: "{draft().audio.offset}",
                                        onchange: move |e| if let Ok(v) = e.value().parse::<u8>() { draft.write().audio.offset = v },
                                        option { value: "0", "On expiration" }
                                        for s in 1..=10u8 { option { value: "{s}", "{s}s before" } }
                                    }
                                }
                                Field { label: "Voice", help: "Voice countdown starting at the specified seconds remaining",
                                    select {
                                        class: "field-short",
                                        value: "{draft().audio.countdown_start}",
                                        onchange: move |e| if let Ok(v) = e.value().parse::<u8>() { draft.write().audio.countdown_start = v },
                                        option { value: "0", "Off" }
                                        for s in 1..=10u8 { option { value: "{s}", "{s}s" } }
                                    }
                                    select {
                                        value: "{draft().audio.countdown_voice.unwrap_or_else(|| \"Amy\".to_string())}",
                                        onchange: move |e| {
                                            let v = e.value();
                                            draft.write().audio.countdown_voice = (v != "Amy").then_some(v);
                                        },
                                        for voice in ["Amy", "Jim", "Yolo", "Nerevar"] { option { value: voice, "{voice}" } }
                                    }
                                }
                            }
                        }
                    }

                    super::modifier_editor::ModifierListEditor {
                        modifiers: draft().modifiers.clone(),
                        on_change: move |new_mods: Vec<baras_types::EffectModifier>| draft.write().modifiers = new_mods,
                    }
                }
            }

            // Actions (outside grid, full width)
            ItemActions {
                has_changes: has_changes(),
                origin: effect_origin(is_draft, is_bundled, is_user_override),
                on_save: move |_| {
                    just_saved.set(true);
                    on_save.call(draft());
                },
                on_delete: move |_| on_delete.call(()),
                on_duplicate: (!is_draft).then_some(on_duplicate),
                if !is_draft && is_user_override {
                    button { class: "btn btn-sm", onclick: move |_| on_export.call(()), "Export" }
                }
            }
        }
    }
}
