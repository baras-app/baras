//! StarParse XML timer import
//!
//! Parses StarParse timer XML exports and converts them to BARAS encounter
//! timers (_custom.toml) and personal effects (user effects file).

use std::collections::HashMap;
use std::path::PathBuf;

use quick_xml::de::from_str;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};

use baras_core::boss::{
    BossEncounterDefinition, BossTimerDefinition, find_custom_file, load_bosses_from_file,
    merge_boss_definition, save_bosses_to_file,
};
use baras_core::dsl::{AudioConfig, Trigger};
use baras_core::effects::{DisplayTarget, EFFECTS_DSL_VERSION, EffectDefinition};
use baras_types::{AbilitySelector, AlertTrigger, EffectSelector, EntityFilter};

use super::effects::{load_user_effects_file, save_user_effects};
use super::encounters::{ensure_user_dir, get_bundled_encounters_dir};
use crate::service::ServiceHandle;

// ─────────────────────────────────────────────────────────────────────────────
// Boss Lookup: (starparse_name, file_stem, boss_id)
// ─────────────────────────────────────────────────────────────────────────────

const BOSS_LOOKUP: &[(&str, &str, &str)] = &[
    // Eternity Vault
    ("Annihilation Droid XRR-3", "eternity_vault", "xrr3"),
    ("Gharj", "eternity_vault", "gharj"),
    ("Soa", "eternity_vault", "soa"),
    // Karagga's Palace
    ("Karagga The Unyielding", "karaggas_palace", "karagga"),
    ("Bonethrasher", "karaggas_palace", "bonethrasher"),
    ("Jarg & Sorno", "karaggas_palace", "jarg_sorno"),
    // Explosive Conflict
    ("Zorn & Toth", "explosive_conflict", "zorn_toth"),
    ("Firebrand & Stormcaller", "explosive_conflict", "firebrand_stormcaller"),
    ("Colonel Vorgath", "explosive_conflict", "colonel_vorgath"),
    ("Warlord Kephess", "explosive_conflict", "warlord_kephess"),
    // Terror From Beyond
    ("The Writhing Horror", "terror_from_beyond", "writhing_horror"),
    ("Dread Guards", "terror_from_beyond", "dread_guard"),
    ("Dreadful Entity", "terror_from_beyond", "dreadful"),
    ("Operator IX", "terror_from_beyond", "op_ix"),
    ("Kephess", "terror_from_beyond", "kephess_the_undying"),
    ("The Terror From Beyond", "terror_from_beyond", "the_terror_from_beyond"),
    // Scum and Villainy
    ("Dash'Roode", "scum_and_villainy", "dashroode"),
    ("Titan 6", "scum_and_villainy", "titan6"),
    ("Hateful Entity", "scum_and_villainy", "hateful_entity"),
    ("Thrasher", "scum_and_villainy", "thrasher"),
    ("Cartel Warlords", "scum_and_villainy", "cartel_warlords"),
    ("Dread Master Styrak", "scum_and_villainy", "styrak"),
    // Dread Fortress
    ("Nefra, Who Bars the Way", "the_dread_fortress", "nefra"),
    ("Gate Commander Draxus", "the_dread_fortress", "draxus"),
    ("Grob\u{2019}Thok, Who Feeds The Forge", "the_dread_fortress", "grobthok"),
    ("Grob'Thok, Who Feeds The Forge", "the_dread_fortress", "grobthok"),
    ("Corruptor Zero", "the_dread_fortress", "corruptor_zero"),
    ("Dread Master Brontes", "the_dread_fortress", "brontes"),
    // Dread Palace
    ("Dread Master Bestia", "the_dread_palace", "bestia"),
    ("Dread Master Tyrans", "the_dread_palace", "tyrans"),
    ("Dread Master Calphayus", "the_dread_palace", "calphayus"),
    ("Dread Master Raptus", "the_dread_palace", "raptus"),
    ("The Dread Council", "the_dread_palace", "dread_council"),
    ("Dread Council", "the_dread_palace", "dread_council"),
    // Ravagers
    ("Sparky", "the_ravagers", "sparky"),
    ("Quartermaster Bulo", "the_ravagers", "bulo"),
    ("Torque", "the_ravagers", "torque"),
    ("Blaster", "the_ravagers", "master_blaster"),
    ("Master & Blaster", "the_ravagers", "master_blaster"),
    ("Coratanni", "the_ravagers", "coratanni"),
    // Temple of Sacrifice
    ("Revan", "temple_of_sacrifice", "revan"),
    ("Sword Squadron", "temple_of_sacrifice", "sword_squadron"),
    ("The Underlurker", "temple_of_sacrifice", "underlurker"),
    // Gods from the Machine
    ("AIVELA & ESNE", "gods_from_the_machine", "aivela_esne"),
    ("Nahut", "gods_from_the_machine", "nahut"),
    ("IZAX", "gods_from_the_machine", "izax"),
    ("Tyth", "gods_from_the_machine", "tyth"),
    // R-4 Anomaly
    ("IP-CPT", "r4_anomaly", "ip_cpt"),
    ("Watchdog", "r4_anomaly", "watchdog"),
    ("Lord Kanoth", "r4_anomaly", "kanoth"),
    ("Lady Dominique", "r4_anomaly", "dominique"),
    // Dxun (The Nature of Progress)
    ("Apex Vanguard", "dxun", "apex_vanguard"),
    ("Trandoshan Squad", "dxun", "trandos"),
    ("The Pack Leader", "dxun", "huntmaster"),
    // World Bosses
    ("Colossal Monolith", "monolith", "monolith"),
    ("Geonosian Queen", "hive_of_the_mountain_queen", "mutated_geonosian_queen"),
];

// ─────────────────────────────────────────────────────────────────────────────
// XML Deserialization Types
// ─────────────────────────────────────────────────────────────────────────────

/// Root XML: `<com.ixale.starparse.domain.ConfigTimers>` → `<timers>` → ConfigTimer elements
#[derive(Debug, Deserialize)]
struct TimerListRoot {
    timers: TimerListInner,
}

#[derive(Debug, Deserialize)]
struct TimerListInner {
    #[serde(
        rename = "com.ixale.starparse.domain.ConfigTimer",
        default
    )]
    items: Vec<XmlConfigTimer>,
}

#[derive(Debug, Deserialize)]
struct XmlConfigTimer {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    folder: Option<String>,
    #[serde(default)]
    trigger: Option<XmlTrigger>,
    #[serde(default)]
    cancel: Option<XmlTrigger>,
    #[serde(default)]
    interval: Option<f32>,
    #[serde(default)]
    color: Option<String>,
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(rename = "ignoreRepeated", default)]
    ignore_repeated: Option<bool>,
    #[serde(default)]
    repeat: Option<u8>,
    #[serde(default)]
    audio: Option<String>,
    #[serde(default)]
    volume: Option<i32>,
    #[serde(rename = "soundOffset", default)]
    sound_offset: Option<u8>,
    #[serde(rename = "countdownCount", default)]
    countdown_count: Option<u8>,
    #[serde(rename = "countdownVoice", default)]
    countdown_voice: Option<String>,
    #[serde(default)]
    boss: Option<String>,
}

#[derive(Debug, Deserialize)]
struct XmlTrigger {
    #[serde(rename = "type", default)]
    trigger_type: Option<String>,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    target: Option<String>,
    #[serde(rename = "abilityGuid", default)]
    ability_guid: Option<String>,
    #[serde(default)]
    ability: Option<String>,
    #[serde(rename = "effectGuid", default)]
    effect_guid: Option<String>,
    #[serde(default)]
    effect: Option<String>,
    #[serde(default)]
    timer: Option<String>,
    #[serde(default)]
    boss: Option<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Conversion Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn slugify(name: &str) -> String {
    name.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect::<String>()
        .split('_')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("_")
}

fn parse_color(hex: &str) -> Option<[u8; 4]> {
    let hex = hex.trim_start_matches("0x").trim_start_matches("0X");
    if hex.len() != 8 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    let a = u8::from_str_radix(&hex[6..8], 16).ok()?;
    Some([r, g, b, a])
}

fn convert_entity_filter(value: &str) -> EntityFilter {
    match value {
        "@Self" => EntityFilter::LocalPlayer,
        "@Other" => EntityFilter::Any,
        name => EntityFilter::Selector(vec![baras_types::EntitySelector::Name(
            name.to_string(),
        )]),
    }
}

/// Build ability selector from guid or name (prefer guid)
fn build_ability_selectors(guid: Option<&str>, name: Option<&str>) -> Vec<AbilitySelector> {
    match guid.or(name) {
        Some(input) => vec![AbilitySelector::from_input(input)],
        None => Vec::new(),
    }
}

/// Build effect selector from guid or name (prefer guid)
fn build_effect_selectors(guid: Option<&str>, name: Option<&str>) -> Vec<EffectSelector> {
    match guid.or(name) {
        Some(input) => vec![EffectSelector::from_input(input)],
        None => Vec::new(),
    }
}

fn convert_trigger(xml: &XmlTrigger) -> Trigger {
    let trigger_type = xml.trigger_type.as_deref().unwrap_or("");
    let source = xml.source.as_deref().map(convert_entity_filter).unwrap_or_default();
    let target = xml.target.as_deref().map(convert_entity_filter).unwrap_or_default();

    match trigger_type {
        "EFFECT_GAINED" => Trigger::EffectApplied {
            effects: build_effect_selectors(
                xml.effect_guid.as_deref(),
                xml.effect.as_deref(),
            ),
            source,
            target,
        },
        "EFFECT_LOST" => Trigger::EffectRemoved {
            effects: build_effect_selectors(
                xml.effect_guid.as_deref(),
                xml.effect.as_deref(),
            ),
            source,
            target,
        },
        "DAMAGE" => Trigger::DamageTaken {
            abilities: build_ability_selectors(
                xml.ability_guid.as_deref(),
                xml.ability.as_deref(),
            ),
            source,
            target,
            mitigation: vec![],
        },
        "HEAL" => Trigger::HealingTaken {
            abilities: build_ability_selectors(
                xml.ability_guid.as_deref(),
                xml.ability.as_deref(),
            ),
            source,
            target,
        },
        "ABILITY_ACTIVATED" => Trigger::AbilityCast {
            abilities: build_ability_selectors(
                xml.ability_guid.as_deref(),
                xml.ability.as_deref(),
            ),
            source,
            target,
        },
        "COMBAT_START" => Trigger::CombatStart,
        "TIMER_STARTED" => Trigger::TimerStarted {
            timer_id: xml
                .timer
                .as_deref()
                .map(|t| format!("sp_{}", slugify(t)))
                .unwrap_or_default(),
        },
        "TIMER_FINISHED" => Trigger::TimerExpires {
            timer_id: xml
                .timer
                .as_deref()
                .map(|t| format!("sp_{}", slugify(t)))
                .unwrap_or_default(),
        },
        // COMBAT_END has no direct trigger equivalent — use Never as placeholder
        _ => Trigger::Never,
    }
}

fn convert_audio(xml: &XmlConfigTimer) -> AudioConfig {
    let has_file = xml.audio.is_some();
    let volume = xml.volume.unwrap_or(0);
    let enabled = volume > 0 || has_file;

    AudioConfig {
        enabled,
        file: xml.audio.clone(),
        offset: xml.sound_offset.unwrap_or(0),
        countdown_start: xml.countdown_count.unwrap_or(0),
        countdown_voice: xml.countdown_voice.clone(),
        alert_text: None,
    }
}

fn convert_to_boss_timer(xml: &XmlConfigTimer) -> BossTimerDefinition {
    let name = xml.name.as_deref().unwrap_or("Unnamed");
    let id = format!("sp_{}", slugify(name));

    let trigger = xml
        .trigger
        .as_ref()
        .map(convert_trigger)
        .unwrap_or(Trigger::Never);

    let cancel_trigger = xml.cancel.as_ref().map(convert_trigger);

    let color = xml
        .color
        .as_deref()
        .and_then(parse_color)
        .unwrap_or([255, 128, 0, 255]);

    let is_alert = xml.interval.map_or(true, |v| v <= 0.0);

    BossTimerDefinition {
        id,
        name: name.to_string(),
        display_text: None,
        trigger,
        duration_secs: xml.interval.unwrap_or(0.0),
        is_alert,
        alert_on: if is_alert { AlertTrigger::OnApply } else { Default::default() },
        alert_text: if is_alert { Some(name.to_string()) } else { None },
        color,
        icon_ability_id: None,
        conditions: Vec::new(),
        phases: Vec::new(),
        counter_condition: None,
        difficulties: Vec::new(),
        group_size: None,
        enabled: xml.enabled.unwrap_or(true),
        can_be_refreshed: !xml.ignore_repeated.unwrap_or(false),
        repeats: xml.repeat.unwrap_or(0),
        chains_to: None,
        cancel_trigger,
        alert_at_secs: None,
        show_on_raid_frames: false,
        show_at_secs: 0.0,
        display_target: Default::default(),
        audio: convert_audio(xml),
        per_target: false,
        roles: Vec::new(),
        gcd_secs: None,
        queue_on_expire: false,
        queue_priority: 0,
        queue_remove_trigger: None,
        queue_blocking_timers: Vec::new(),
        queue_countdown_bar: false,
        queue_hide_from_next: false,
    }
}

fn convert_to_effect(xml: &XmlConfigTimer) -> EffectDefinition {
    let name = xml.name.as_deref().unwrap_or("Unnamed");
    let id = format!("sp_{}", slugify(name));

    let trigger = xml
        .trigger
        .as_ref()
        .map(convert_trigger)
        .unwrap_or(Trigger::Never);

    let color = xml.color.as_deref().and_then(parse_color);

    let is_alert = xml.interval.map_or(true, |v| v <= 0.0);

    EffectDefinition {
        id,
        name: name.to_string(),
        display_text: None,
        enabled: xml.enabled.unwrap_or(true),
        trigger,
        ignore_effect_removed: false,
        refresh_abilities: Vec::new(),
        is_aoe_refresh: false,
        is_refreshed_on_modify: false,
        default_charges: None,
        duration_secs: xml.interval.filter(|&v| v > 0.0),
        is_affected_by_alacrity: false,
        cooldown_ready_secs: 0.0,
        color,
        show_at_secs: 0.0,
        display_targets: if is_alert { vec![] } else { vec![DisplayTarget::EffectsA] },
        icon_ability_id: None,
        show_icon: true,
        display_source: false,
        disciplines: vec![],
        persist_past_death: false,
        track_outside_combat: true,
        on_apply_trigger_timer: None,
        on_expire_trigger_timer: None,
        is_alert,
        alert_text: if is_alert { Some(name.to_string()) } else { None },
        alert_on: if is_alert { AlertTrigger::OnApply } else { Default::default() },
        audio: convert_audio(xml),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Grouping
// ─────────────────────────────────────────────────────────────────────────────

struct GroupedTimers {
    /// file_stem → boss_id → timers
    encounter_timers: HashMap<String, HashMap<String, Vec<BossTimerDefinition>>>,
    /// Personal/class effects (no boss tag)
    effects: Vec<EffectDefinition>,
    /// Boss names that couldn't be mapped
    unmapped_bosses: Vec<String>,
    /// Built-in timers that were skipped
    skipped_builtin: usize,
    /// Personal timers with unsupported trigger types (DAMAGE, ABILITY_ACTIVATED, etc.)
    skipped_unsupported_effects: usize,
    /// Total encounter timers across all operations
    encounter_timer_count: usize,
}

fn group_timers(xml_timers: &[XmlConfigTimer]) -> GroupedTimers {
    let lookup: HashMap<&str, (&str, &str)> = BOSS_LOOKUP
        .iter()
        .map(|&(name, stem, id)| (name, (stem, id)))
        .collect();

    let mut encounter_timers: HashMap<String, HashMap<String, Vec<BossTimerDefinition>>> =
        HashMap::new();
    let mut effects = Vec::new();
    let mut unmapped_set: HashMap<String, ()> = HashMap::new();
    let mut skipped_builtin = 0usize;
    let mut skipped_unsupported_effects = 0usize;
    let mut encounter_timer_count = 0usize;

    for xml in xml_timers {
        let folder = xml.folder.as_deref().unwrap_or("");
        if folder.starts_with("Built-in:") {
            skipped_builtin += 1;
            continue;
        }

        // Boss name can be on the trigger element or the timer itself
        let boss_name = xml
            .trigger
            .as_ref()
            .and_then(|t| t.boss.as_deref())
            .or(xml.boss.as_deref());

        let Some(boss_name) = boss_name else {
            // No boss → personal effect, but only if trigger is effect-based
            let trigger_type = xml
                .trigger
                .as_ref()
                .and_then(|t| t.trigger_type.as_deref())
                .unwrap_or("");
            if matches!(trigger_type, "EFFECT_GAINED" | "EFFECT_LOST" | "DAMAGE" | "HEAL" | "ABILITY_ACTIVATED") {
                effects.push(convert_to_effect(xml));
            } else {
                skipped_unsupported_effects += 1;
            }
            continue;
        };

        if let Some(&(file_stem, boss_id)) = lookup.get(boss_name) {
            let timer = convert_to_boss_timer(xml);
            encounter_timers
                .entry(file_stem.to_string())
                .or_default()
                .entry(boss_id.to_string())
                .or_default()
                .push(timer);
            encounter_timer_count += 1;
        } else {
            unmapped_set.insert(boss_name.to_string(), ());
        }
    }

    GroupedTimers {
        encounter_timers,
        effects,
        unmapped_bosses: unmapped_set.into_keys().collect(),
        skipped_builtin,
        skipped_unsupported_effects,
        encounter_timer_count,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Response Types
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationPreview {
    pub name: String,
    pub timer_count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StarParsePreview {
    pub encounter_timers: usize,
    pub effect_timers: usize,
    pub operations: Vec<OperationPreview>,
    pub unmapped_bosses: Vec<String>,
    pub skipped_builtin: usize,
    pub skipped_unsupported_effects: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StarParseImportResult {
    pub files_written: usize,
    pub encounter_timers_imported: usize,
    pub effects_imported: usize,
}

// ─────────────────────────────────────────────────────────────────────────────
// XML Parsing
// ─────────────────────────────────────────────────────────────────────────────

fn parse_xml(content: &str) -> Result<Vec<XmlConfigTimer>, String> {
    let root: TimerListRoot =
        from_str(content).map_err(|e| format!("Failed to parse StarParse XML: {}", e))?;
    Ok(root.timers.items)
}

// ─────────────────────────────────────────────────────────────────────────────
// Tauri Commands
// ─────────────────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn preview_starparse_import(path: String) -> Result<StarParsePreview, String> {
    let content =
        std::fs::read_to_string(&path).map_err(|e| format!("Failed to read file: {}", e))?;
    let xml_timers = parse_xml(&content)?;
    let grouped = group_timers(&xml_timers);

    let mut operations: Vec<OperationPreview> = grouped
        .encounter_timers
        .iter()
        .map(|(stem, bosses)| {
            let timer_count: usize = bosses.values().map(|v| v.len()).sum();
            OperationPreview {
                name: stem.replace('_', " "),
                timer_count,
            }
        })
        .collect();
    operations.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(StarParsePreview {
        encounter_timers: grouped.encounter_timer_count,
        effect_timers: grouped.effects.len(),
        operations,
        unmapped_bosses: grouped.unmapped_bosses,
        skipped_builtin: grouped.skipped_builtin,
        skipped_unsupported_effects: grouped.skipped_unsupported_effects,
    })
}

#[tauri::command]
pub async fn import_starparse_timers(
    app_handle: AppHandle,
    service: State<'_, ServiceHandle>,
    path: String,
) -> Result<StarParseImportResult, String> {
    let content =
        std::fs::read_to_string(&path).map_err(|e| format!("Failed to read file: {}", e))?;
    let xml_timers = parse_xml(&content)?;
    let grouped = group_timers(&xml_timers);

    let user_dir = ensure_user_dir()?;
    let bundled_dir = get_bundled_encounters_dir(&app_handle);

    let mut files_written = 0usize;
    let encounter_timers_imported = grouped.encounter_timer_count;

    // Write encounter timers to _custom.toml files
    for (file_stem, boss_map) in &grouped.encounter_timers {
        // Resolve the custom file path
        let custom_path = if let Some(ref bundled) = bundled_dir {
            // Look for the bundled file to derive the custom path
            let bundled_file = find_bundled_toml(bundled, file_stem);
            if let Some(bf) = bundled_file {
                find_custom_file(&bf, &user_dir)
                    .unwrap_or_else(|| {
                        // No existing custom file — create one
                        let custom_name = format!("{}_custom.toml", file_stem);
                        // Preserve subdirectory structure
                        if let Ok(rel) = bf.strip_prefix(bundled) {
                            if let Some(parent) = rel.parent().filter(|p| p != &PathBuf::new()) {
                                return user_dir.join(parent).join(custom_name);
                            }
                        }
                        user_dir.join(custom_name)
                    })
            } else {
                // No bundled file found — write directly to user dir
                user_dir.join(format!("{}_custom.toml", file_stem))
            }
        } else {
            user_dir.join(format!("{}_custom.toml", file_stem))
        };

        // Ensure parent directory exists
        if let Some(parent) = custom_path.parent() {
            std::fs::create_dir_all(parent).ok();
        }

        // Load existing custom bosses
        let mut existing = if custom_path.exists() {
            load_bosses_from_file(&custom_path).unwrap_or_default()
        } else {
            Vec::new()
        };

        // Merge each boss's timers
        for (boss_id, timers) in boss_map {
            let import_boss = BossEncounterDefinition {
                id: boss_id.clone(),
                timers: timers.clone(),
                ..Default::default()
            };

            if let Some(existing_boss) = existing.iter_mut().find(|b| b.id == *boss_id) {
                merge_boss_definition(existing_boss, import_boss);
            } else {
                existing.push(import_boss);
            }
        }

        save_bosses_to_file(&existing, &custom_path)?;
        files_written += 1;
    }

    // Write personal effects to user effects file
    let effects_imported = grouped.effects.len();
    if !grouped.effects.is_empty() {
        let mut user_effects: Vec<EffectDefinition> = load_user_effects_file()
            .filter(|(v, _)| *v == EFFECTS_DSL_VERSION)
            .map(|(_, e)| e)
            .unwrap_or_default();

        for imported in &grouped.effects {
            if let Some(existing) = user_effects.iter_mut().find(|e| e.id == imported.id) {
                *existing = imported.clone();
            } else {
                user_effects.push(imported.clone());
            }
        }

        save_user_effects(&user_effects)?;
        files_written += 1;
    }

    // Reload definitions in running service
    let _ = service.reload_timer_definitions().await;
    let _ = service.reload_effect_definitions().await;

    Ok(StarParseImportResult {
        files_written,
        encounter_timers_imported,
        effects_imported,
    })
}

/// Find a bundled .toml file by stem (e.g., "dxun" → "operations/dxun.toml")
fn find_bundled_toml(bundled_dir: &std::path::Path, file_stem: &str) -> Option<PathBuf> {
    find_toml_recursive(bundled_dir, file_stem)
}

fn find_toml_recursive(dir: &std::path::Path, stem: &str) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_toml_recursive(&path, stem) {
                return Some(found);
            }
        } else if path.extension().is_some_and(|e| e == "toml")
            && path.file_stem().is_some_and(|s| s == stem)
        {
            return Some(path);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_real_xml() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../scripts/starparse-timers v15.xml");
        let content = std::fs::read_to_string(path).expect("test XML file");
        let timers = parse_xml(&content).expect("parse XML");
        assert!(!timers.is_empty(), "should parse timers");

        let grouped = group_timers(&timers);
        println!("Total timers parsed: {}", timers.len());
        println!("Encounter timers: {}", grouped.encounter_timer_count);
        println!("Effects: {}", grouped.effects.len());
        println!("Skipped builtin: {}", grouped.skipped_builtin);
        println!("Unmapped bosses: {:?}", grouped.unmapped_bosses);
        println!("Operations: {:?}", grouped.encounter_timers.keys().collect::<Vec<_>>());

        assert!(grouped.encounter_timer_count > 0, "should have encounter timers");
    }
}
