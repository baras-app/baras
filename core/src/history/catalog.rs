//! What the History tab needs from the encounter definitions: each area's
//! bosses in operation order, which one is final, and the timed-run rule.
//! Loaded once from the bundled and user definition directories.

use std::path::Path;

use hashbrown::HashMap;

use crate::dsl::{AreaType, build_area_index, load_area_config, load_bosses_with_custom};

#[derive(Debug, Clone)]
pub struct CatalogBoss {
    pub name: String,
    pub is_final: bool,
    /// Secret / bonus boss: never part of a clear or a boss total
    pub optional: bool,
    /// Config keys ("master", "veteran_8"); empty = every difficulty
    difficulties: Vec<String>,
}

impl CatalogBoss {
    /// `tier` is a config key like "master"; `size` the group size.
    pub fn exists_at(&self, tier: &str, size: u8) -> bool {
        self.difficulties.is_empty() || self.difficulties.iter().any(|k| key_matches(k, tier, size))
    }
}

/// Timed-run achievement with the limit from `[area] timed_run_minutes`. It
/// applies at the `[area.timer_start]` difficulties when that region names
/// them, else at the operation's highest difficulty. Where the clock starts is
/// that region, or the first pull when none was logged.
#[derive(Debug, Clone)]
pub struct TimerRule {
    /// Config keys ("master", "veteran_8")
    tiers: Vec<String>,
    pub limit_secs: Option<u32>,
}

impl TimerRule {
    pub fn applies_to(&self, tier: &str, size: u8) -> bool {
        self.tiers.iter().any(|k| key_matches(k, tier, size))
    }
}

/// "master" if any required boss lists it, else "veteran", else "story".
/// Optional encounters are skipped: R4's Hallway lists a master mode the
/// operation does not have.
fn highest_tier(bosses: &[CatalogBoss]) -> String {
    let required = || bosses.iter().filter(|b| !b.optional);
    let has = |t: &str| required().any(|b| b.difficulties.is_empty() || b.difficulties.iter().any(|k| k.split('_').next() == Some(t)));
    ["master", "veteran", "story"].into_iter().find(|t| has(t)).unwrap_or("master").to_string()
}

#[derive(Debug, Clone)]
pub struct AreaDef {
    pub name: String,
    pub area_type: AreaType,
    /// Definition order is operation order
    pub bosses: Vec<CatalogBoss>,
    pub timer: Option<TimerRule>,
}

impl AreaDef {
    /// Bosses that make up a clear at this difficulty (optional ones excluded)
    pub fn bosses_at<'a>(&'a self, tier: &'a str, size: u8) -> impl Iterator<Item = &'a CatalogBoss> + 'a {
        self.bosses.iter().filter(move |b| !b.optional && b.exists_at(tier, size))
    }

    pub fn is_final(&self, boss_name: &str) -> bool {
        self.bosses.iter().any(|b| b.is_final && b.name == boss_name)
    }

    /// Bonus boss or trash gauntlet (R4's Hallway): killing it is not a boss kill
    pub fn is_optional(&self, boss_name: &str) -> bool {
        self.bosses.iter().any(|b| b.optional && b.name == boss_name)
    }

    pub fn timer_at(&self, tier: &str, size: u8) -> Option<&TimerRule> {
        self.timer.as_ref().filter(|t| t.applies_to(tier, size))
    }
}

/// `"master"` matches any size; `"master_8"` only 8-man.
fn key_matches(key: &str, tier: &str, size: u8) -> bool {
    match key.split_once('_') {
        Some((k, s)) => k.eq_ignore_ascii_case(tier) && s.parse::<u8>().is_ok_and(|n| n == size),
        None => key.eq_ignore_ascii_case(tier),
    }
}

#[derive(Debug, Default)]
pub struct AreaCatalog {
    areas: HashMap<i64, AreaDef>,
}

impl AreaCatalog {
    /// Bundled definitions, overlaid with the user's custom files. Unreadable
    /// files are skipped with a warning.
    pub fn load(bundled_dir: Option<&Path>, user_dir: Option<&Path>) -> Self {
        let mut index = HashMap::new();
        for dir in [bundled_dir, user_dir].into_iter().flatten().filter(|d| d.exists()) {
            match build_area_index(dir) {
                Ok(i) => index.extend(i),
                Err(e) => tracing::warn!(dir = %dir.display(), error = %e, "Area index failed"),
            }
        }
        let mut areas = HashMap::with_capacity(index.len());
        for (area_id, entry) in index {
            let area = match load_area_config(&entry.file_path) {
                Ok(Some(a)) => a,
                Ok(None) => continue,
                Err(e) => {
                    tracing::warn!(path = %entry.file_path.display(), error = %e, "Skipping area");
                    continue;
                }
            };
            let bosses: Vec<CatalogBoss> = load_bosses_with_custom(&entry.file_path, user_dir)
                .unwrap_or_default()
                .into_iter()
                .map(|b| CatalogBoss {
                    name: b.name,
                    is_final: b.is_final_boss,
                    optional: b.is_optional_boss,
                    difficulties: b.difficulties,
                })
                .collect();
            let timer = area.timed_run_minutes.map(|m| {
                let tiers = match area.timer_start.as_ref().map(|t| &t.difficulties) {
                    Some(d) if !d.is_empty() => d.clone(),
                    _ => vec![highest_tier(&bosses)],
                };
                TimerRule { tiers, limit_secs: Some(m * 60) }
            });
            areas.insert(
                area_id,
                AreaDef {
                    name: area.name,
                    area_type: area.area_type,
                    bosses,
                    timer,
                },
            );
        }
        Self { areas }
    }

    pub fn area(&self, area_id: i64) -> Option<&AreaDef> {
        self.areas.get(&area_id)
    }
}
