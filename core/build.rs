use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::{BufWriter, Write};
use std::path::Path;

const IMPERIAL: [&str; 4] = ["agent", "bounty_hunter", "sith_warrior", "sith_inquisitor"];
const REPUBLIC: [&str; 4] = ["trooper", "smuggler", "jedi_knight", "jedi_consular"];

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();

    generate_off_gcd_set(&out_dir);
    generate_attack_types_map(&out_dir);
    generate_alacrity_buffs_map(&out_dir);
    generate_channel_times_map(&out_dir);
    generate_discipline_abilities_map(&out_dir);
    generate_interrupt_abilities_set(&out_dir);
    generate_battle_rez_set(&out_dir);
    let mirrors = mirror_entries();
    generate_mirror_abilities_map(&out_dir, &mirrors);
    generate_defensive_cooldown_sets(&out_dir, &mirrors);

    println!("cargo:rerun-if-changed=data/off_gcd_extra.txt");
    println!("cargo:rerun-if-changed=data/defensive_cooldowns.txt");
    println!("cargo:rerun-if-changed=data/attack_types.csv");
    println!("cargo:rerun-if-changed=data/alacrity_abilities.csv");
    println!("cargo:rerun-if-changed=data/discipline_ids.csv");
    println!("cargo:rerun-if-changed=data/ability_data.csv");
    println!("cargo:rerun-if-changed=data/effect_data.csv");
}

/// Off-GCD abilities: no `ablGlobalCooldownTime` in the game data, plus the IDs in
/// data/off_gcd_extra.txt for what the extractor does not reach (adrenals, medpacs, mounts).
fn generate_off_gcd_set(out_dir: &str) {
    let extra = fs::read_to_string("data/off_gcd_extra.txt").expect("failed to read off_gcd_extra.txt");
    let ids = ability_rows()
        .iter()
        .filter(|f| f[5].is_empty())
        .filter_map(|f| f[1].parse().ok())
        .chain(extra.lines().filter_map(|l| l.trim().parse().ok()))
        .collect();
    write_id_set(out_dir, "off_gcd_abilities.rs", "OFF_GCD_ABILITIES", ids);
}

/// Rows of data/ability_data.csv, header skipped.
/// Columns: fqn(0), global_combat_id(1), name(2), ablGlobalCooldownTime(5), mirror_fqn(6),
/// is_interrupt(8), its_revive(10), channel_time(17), disciplines(20).
fn ability_rows() -> Vec<Vec<String>> {
    let csv = fs::read_to_string("data/ability_data.csv").expect("failed to read ability_data.csv");
    csv.trim_start_matches('\u{feff}')
        .lines()
        .skip(1)
        .map(split_csv_line)
        .filter(|f| f.len() > 10)
        .collect()
}

fn is_player_class(fqn: &str) -> bool {
    matches!(fqn.split('.').nth(1), Some(c) if IMPERIAL.contains(&c) || REPUBLIC.contains(&c))
}

fn write_id_set(out_dir: &str, file_name: &str, static_name: &str, mut ids: Vec<i64>) {
    ids.sort_unstable();
    ids.dedup();
    let mut file = BufWriter::new(fs::File::create(Path::new(out_dir).join(file_name)).unwrap());
    let mut builder = phf_codegen::Set::new();
    for id in &ids {
        builder.entry(*id);
    }
    writeln!(file, "pub static {static_name}: phf::Set<i64> = {};", builder.build()).unwrap();
}

/// Dedicated interrupts: abilities that interrupt and are off the GCD. Abilities that
/// interrupt as a side effect (Force Leap, Backstab) are on the GCD and stay out.
fn generate_interrupt_abilities_set(out_dir: &str) {
    let ids = ability_rows()
        .iter()
        .filter(|f| f[8] == "True" && f[5].is_empty())
        .filter_map(|f| f[1].parse().ok())
        .collect();
    write_id_set(out_dir, "interrupt_abilities.rs", "INTERRUPT_ABILITIES", ids);
}

/// Ability ID → base channel time in seconds (`channel_time` column).
fn generate_channel_times_map(out_dir: &str) {
    // BTreeMap for deterministic output (sorted by key)
    let mut entries = BTreeMap::new();
    for f in ability_rows() {
        let (Ok(id), Some(Ok(secs))) = (f[1].parse::<i64>(), f.get(17).map(|s| s.parse::<f32>())) else {
            continue;
        };
        entries.insert(id, format!("{secs}f32"));
    }

    let mut file = BufWriter::new(fs::File::create(Path::new(out_dir).join("channel_abilities.rs")).unwrap());
    let mut builder = phf_codegen::Map::new();
    for (id, secs) in &entries {
        builder.entry(*id, secs);
    }
    writeln!(file, "static CHANNEL_TIMES: phf::Map<i64, f32> = {};", builder.build()).unwrap();
}

/// In-combat resurrection abilities of the player classes.
fn generate_battle_rez_set(out_dir: &str) {
    let ids = ability_rows()
        .iter()
        .filter(|f| f[10] == "True" && is_player_class(&f[0]))
        .filter_map(|f| f[1].parse().ok())
        .collect();
    write_id_set(out_dir, "battle_rez_abilities.rs", "BATTLE_REZ_ABILITY_IDS", ids);
}

fn generate_attack_types_map(out_dir: &str) {
    let csv = fs::read_to_string("data/attack_types.csv").expect("failed to read attack_types.csv");

    // BTreeMap for deterministic output (sorted by key)
    let mut entries = BTreeMap::new();
    for line in csv.lines().skip(1) {
        let fields: Vec<&str> = line.split(',').collect();
        if fields.len() < 2 {
            continue;
        }
        let id: i64 = match fields[0].trim().parse() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let attack_type = fields[1].trim();
        if matches!(attack_type, "" | "None" | "God") {
            continue;
        }
        entries.entry(id).or_insert(attack_type.to_string());
    }

    let path = Path::new(out_dir).join("attack_types.rs");
    let mut file = BufWriter::new(fs::File::create(&path).unwrap());

    let mut builder = phf_codegen::Map::new();
    let quoted: Vec<_> = entries.iter().map(|(id, at)| (*id, format!("\"{}\"", at))).collect();
    for (id, at) in &quoted {
        builder.entry(*id, at);
    }

    writeln!(
        file,
        "pub static ATTACK_TYPES: phf::Map<i64, &'static str> = {};",
        builder.build()
    )
    .unwrap();
}

fn generate_alacrity_buffs_map(out_dir: &str) {
    let csv = fs::read_to_string("data/alacrity_abilities.csv").expect("failed to read alacrity_abilities.csv");

    // BTreeMap for deterministic output (sorted by key)
    let mut entries = BTreeMap::new();
    for line in csv.lines().skip(1) {
        let fields: Vec<&str> = line.split(',').collect();
        if fields.len() < 5 {
            continue;
        }
        let id: i64 = match fields[0].trim().parse() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let amount: f32 = match fields[2].trim().parse() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let is_stack = fields[3].trim().eq_ignore_ascii_case("true");
        let duration_secs: f32 = match fields[4].trim().parse() {
            Ok(v) => v,
            Err(_) => continue,
        };
        entries.insert(
            id,
            format!("AlacrityBuff {{ amount: {amount}f32, is_stack: {is_stack}, duration_secs: {duration_secs}f32 }}"),
        );
    }

    let path = Path::new(out_dir).join("alacrity_buffs.rs");
    let mut file = BufWriter::new(fs::File::create(&path).unwrap());

    let mut builder = phf_codegen::Map::new();
    for (id, buff) in &entries {
        builder.entry(*id, buff);
    }

    writeln!(
        file,
        "pub static ALACRITY_BUFFS: phf::Map<i64, AlacrityBuff> = {};",
        builder.build()
    )
    .unwrap();
}

/// Ability ID → bitmask of the disciplines that have it (`disciplines` column), with
/// bit i standing for row i of data/discipline_ids.csv.
fn generate_discipline_abilities_map(out_dir: &str) {
    let ids = fs::read_to_string("data/discipline_ids.csv").expect("failed to read discipline_ids.csv");
    let disciplines: Vec<(&str, i64)> = ids
        .lines()
        .skip(1)
        .filter_map(|l| {
            let mut f = l.split(',');
            Some((f.next()?, f.next()?.trim().parse().ok()?))
        })
        .collect();
    assert!(disciplines.len() <= 64, "discipline mask is a u64");

    // BTreeMap for deterministic output (sorted by key)
    let mut entries = BTreeMap::new();
    for f in ability_rows() {
        let (Ok(id), Some(keys)) = (f[1].parse::<i64>(), f.get(20)) else {
            continue;
        };
        let mask = keys
            .split(';')
            .filter_map(|k| disciplines.iter().position(|(d, _)| *d == k))
            .fold(0u64, |m, bit| m | 1 << bit);
        if mask != 0 {
            entries.insert(id, format!("{mask}u64"));
        }
    }

    let path = Path::new(out_dir).join("discipline_abilities.rs");
    let mut file = BufWriter::new(fs::File::create(&path).unwrap());
    let guids: Vec<String> = disciplines.iter().map(|(_, guid)| guid.to_string()).collect();
    writeln!(
        file,
        "static DISCIPLINE_GUIDS: [i64; {}] = [{}];",
        guids.len(),
        guids.join(", ")
    )
    .unwrap();

    let mut builder = phf_codegen::Map::new();
    for (id, mask) in &entries {
        builder.entry(*id, mask);
    }
    writeln!(
        file,
        "static ABILITY_DISCIPLINES: phf::Map<i64, u64> = {};",
        builder.build()
    )
    .unwrap();
}

/// Split a CSV line, honoring double-quoted fields (ability names can contain commas).
fn split_csv_line(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    for c in line.chars() {
        match c {
            '"' => in_quotes = !in_quotes,
            ',' if !in_quotes => fields.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    fields.push(cur);
    fields
}

/// Republic/Imperial mirror pairs from the full ability dump, plus the named
/// effects of those abilities: an effect's mirror is the effect with the same
/// number on the mirror ability.
/// Columns used: fqn(0), global_combat_id(1), name(2), mirror_fqn(6); in
/// effect_data.csv effect_id(0), ability_fqn(2), effect_number(3), name(4),
/// has_own_name(5).
/// Returns id → (mirror id, mirror name, the keyed side is Imperial).
fn mirror_entries() -> BTreeMap<i64, (i64, String, bool)> {
    // Pass 1: fqn → (id, name, is_imperial) for every row that has a mirror.
    let mut by_fqn: BTreeMap<String, (i64, String, bool)> = BTreeMap::new();
    let mut pairs: Vec<(String, String)> = Vec::new();
    for f in ability_rows() {
        if f[6].is_empty() {
            continue;
        }
        let Ok(id) = f[1].trim().parse::<i64>() else { continue };
        let class = f[0].split('.').nth(1).unwrap_or("");
        by_fqn.insert(f[0].clone(), (id, f[2].clone(), IMPERIAL.contains(&class)));
        pairs.push((f[0].clone(), f[6].clone()));
    }

    // Pass 2: id → mirror (id, name, faction). BTreeMap for deterministic output.
    let mut entries = BTreeMap::new();
    for (fqn, mirror_fqn) in &pairs {
        let (Some((id, _, is_imperial)), Some((mid, mname, _))) = (by_fqn.get(fqn), by_fqn.get(mirror_fqn)) else {
            continue;
        };
        entries.insert(*id, (*mid, mname.clone(), *is_imperial));
    }

    // Named effects log under their own ID. One the mirror ability has no
    // counterpart for maps to itself, so it is not renamed after the ability.
    let csv = fs::read_to_string("data/effect_data.csv").expect("failed to read effect_data.csv");
    let effects: Vec<Vec<String>> = csv
        .trim_start_matches('\u{feff}')
        .lines()
        .skip(1)
        .map(split_csv_line)
        .filter(|f| f.len() > 5)
        .collect();
    // (ability fqn, effect number) → (effect id, name), first row wins
    let mut by_number: BTreeMap<(&str, &str), (i64, &str)> = BTreeMap::new();
    for f in &effects {
        if let Ok(id) = f[0].parse::<i64>() {
            by_number.entry((&f[2], &f[3])).or_insert((id, &f[4]));
        }
    }
    let mirror_of: BTreeMap<&str, &str> = pairs.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
    for f in effects.iter().filter(|f| f[5] == "True") {
        let (Ok(id), Some(mirror_fqn)) = (f[0].parse::<i64>(), mirror_of.get(f[2].as_str())) else {
            continue;
        };
        if !by_fqn.contains_key(*mirror_fqn) {
            continue;
        }
        let Some((_, _, is_imperial)) = by_fqn.get(&f[2]) else {
            continue;
        };
        let (mid, mname) = by_number
            .get(&(*mirror_fqn, f[3].as_str()))
            .copied()
            .unwrap_or((id, &f[4]));
        entries
            .entry(id)
            .or_insert_with(|| (mid, mname.to_string(), *is_imperial));
    }
    entries
}

fn generate_mirror_abilities_map(out_dir: &str, entries: &BTreeMap<i64, (i64, String, bool)>) {
    let path = Path::new(out_dir).join("mirror_abilities.rs");
    let mut file = BufWriter::new(fs::File::create(&path).unwrap());

    let mut builder = phf_codegen::Map::new();
    for (id, (mid, mname, is_imperial)) in entries {
        builder.entry(
            *id,
            format!("MirrorAbility {{ mirror_id: {mid}i64, mirror_name: {mname:?}, is_imperial: {is_imperial} }}"),
        );
    }

    writeln!(
        file,
        "pub static MIRROR_ABILITIES: phf::Map<i64, MirrorAbility> = {};",
        builder.build()
    )
    .unwrap();
}

/// Abilities and effects of data/defensive_cooldowns.txt (lines of
/// `<ability or -> : <effects>`, `#` comments) and their Republic mirrors.
fn generate_defensive_cooldown_sets(out_dir: &str, mirrors: &BTreeMap<i64, (i64, String, bool)>) {
    let list = fs::read_to_string("data/defensive_cooldowns.txt").expect("failed to read defensive_cooldowns.txt");
    let with_mirrors = |ids: &str| -> Vec<i64> {
        ids.split_whitespace()
            .filter_map(|id| id.parse::<i64>().ok())
            .flat_map(|id| [Some(id), mirrors.get(&id).map(|m| m.0)])
            .flatten()
            .collect()
    };
    let (mut abilities, mut effects) = (Vec::new(), Vec::new());
    for line in list.lines() {
        if let Some((ability, line_effects)) = line.split('#').next().unwrap_or("").split_once(':') {
            abilities.extend(with_mirrors(ability));
            effects.extend(with_mirrors(line_effects));
        }
    }
    let path = Path::new(out_dir).join("defensive_cooldowns.rs");
    let mut file = BufWriter::new(fs::File::create(path).unwrap());
    for (name, mut ids) in [("DEFENSIVE_ABILITIES", abilities), ("DEFENSIVE_EFFECTS", effects)] {
        ids.sort_unstable();
        ids.dedup();
        let mut builder = phf_codegen::Set::new();
        for id in &ids {
            builder.entry(*id);
        }
        writeln!(file, "pub static {name}: phf::Set<i64> = {};", builder.build()).unwrap();
    }
}
