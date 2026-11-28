//! Map pages per boss (`--map-pages`).
//!
//! Replays a whole log and, for each boss fight in it, tallies which pages of
//! the map index (`pages.toml` of the maps repository) the players stood on.
//! Prints one `boss id<TAB>image` line per page a fight really used, by the
//! rule the Replay tab narrows an area's pages with: a position counts for the
//! first (smallest) page containing it, and a page needs 1/20 of a fight's
//! positions. `just bake-boss-maps` unions these over many logs into the
//! `map_pages` of the bundled boss definitions.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use encoding_rs::WINDOWS_1252;
use serde::Deserialize;

use baras_core::boss::{build_area_index, load_bosses_from_file};
use baras_core::combat_log::{EntityType, LogParser};
use baras_core::context::{DefinitionLoader, ParsingSession, parse_log_filename};
use baras_core::effects::DefinitionSet;
use baras_core::encounter::EncounterState;

/// Same share as the Replay tab's `MAP_SHARE_DIVISOR`
const SHARE_DIVISOR: u32 = 20;

#[derive(Deserialize)]
struct Index {
    #[serde(default)]
    area: Vec<Area>,
}

#[derive(Deserialize)]
struct Area {
    id: i64,
    #[serde(default)]
    maps: Vec<Page>,
}

#[derive(Deserialize)]
struct Page {
    image: String,
    min_x: f32,
    max_x: f32,
    min_y: f32,
    max_y: f32,
    min_z: Option<f32>,
    max_z: Option<f32>,
}

impl Page {
    fn covers(&self, x: f32, y: f32, z: f32) -> bool {
        (self.min_x.min(self.max_x)..=self.min_x.max(self.max_x)).contains(&x)
            && (self.min_y.min(self.max_y)..=self.min_y.max(self.max_y)).contains(&y)
            && self.min_z.is_none_or(|min| z >= min)
            && self.max_z.is_none_or(|max| z <= max)
    }
}

pub fn run(log: &Path, definitions_root: Option<&Path>, index: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let root = definitions_root.map(Path::to_path_buf).unwrap_or_else(|| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("core/definitions")
    });
    let index: Index = toml::from_str(&std::fs::read_to_string(index)?)?;

    let mut bytes = Vec::new();
    File::open(log)?.read_to_end(&mut bytes)?;
    let (content, _, _) = WINDOWS_1252.decode(&bytes);
    let session_date = log
        .file_name()
        .and_then(|f| f.to_str())
        .and_then(parse_log_filename)
        .map(|(_, dt)| dt)
        .unwrap_or_else(|| chrono::Local::now().naive_local());
    let parser = LogParser::new(session_date);

    let mut session = ParsingSession::new(log.to_path_buf(), DefinitionSet::new());
    let area_index = build_area_index(&root.join("encounters"))?;
    let loader: DefinitionLoader = Box::new(move |area_id| {
        area_index
            .get(&area_id)
            .and_then(|entry| load_bosses_from_file(&entry.file_path).ok())
    });
    session.set_definition_loader(Arc::new(loader));

    // (encounter, boss id) → positions in the fight, and per page image
    let mut fights: BTreeMap<(u64, String), (u32, BTreeMap<&str, u32>)> = BTreeMap::new();
    for event in content
        .lines()
        .enumerate()
        .filter_map(|(line_num, line)| parser.parse_line(line_num as u64, line))
    {
        let positions = [&event.source_entity, &event.target_entity]
            .map(|e| e.position.filter(|_| e.entity_type == EntityType::Player));
        session.process_event(event);
        let Some(enc) = session.session_cache.as_ref().and_then(|c| c.current_encounter()) else {
            continue;
        };
        if enc.state != EncounterState::InCombat {
            continue;
        }
        let (Some(boss), Some(area)) = (
            enc.active_boss_definition(),
            enc.area_id.and_then(|id| index.area.iter().find(|a| a.id == id)),
        ) else {
            continue;
        };
        let (total, pages) = fights.entry((enc.id, boss.id.clone())).or_default();
        for at in positions.into_iter().flatten() {
            *total += 1;
            if let Some(page) = area.maps.iter().find(|p| p.covers(at.x, at.y, at.z)) {
                *pages.entry(page.image.as_str()).or_default() += 1;
            }
        }
    }

    for ((_, boss), (total, pages)) in &fights {
        for (image, n) in pages {
            if n * SHARE_DIVISOR >= (*total).max(1) {
                println!("{boss}\t{image}");
            }
        }
    }
    Ok(())
}
