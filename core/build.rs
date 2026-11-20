use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::{BufWriter, Write};
use std::path::Path;

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();

    generate_off_gcd_set(&out_dir);
    generate_attack_types_map(&out_dir);
    generate_alacrity_buffs_map(&out_dir);
    generate_discipline_abilities_map(&out_dir);
    generate_interrupt_abilities_set(&out_dir);
    generate_mirror_abilities_map(&out_dir);

    println!("cargo:rerun-if-changed=data/off_gcd.json");
    println!("cargo:rerun-if-changed=data/attack_types.csv");
    println!("cargo:rerun-if-changed=data/alacrity_abilities.csv");
    println!("cargo:rerun-if-changed=data/discipline_unique_abilities.csv");
    println!("cargo:rerun-if-changed=data/interrupts.csv");
    println!("cargo:rerun-if-changed=data/ability_data.csv");
}

fn generate_off_gcd_set(out_dir: &str) {
    let json = fs::read_to_string("data/off_gcd.json").expect("failed to read off_gcd.json");

    // Simple JSON object parse: extract all numeric keys
    let mut ids: Vec<i64> = json
        .split('"')
        .enumerate()
        .filter_map(|(i, s)| if i % 2 == 1 { s.parse::<i64>().ok() } else { None })
        .collect();
    ids.sort_unstable();
    ids.dedup();

    let path = Path::new(out_dir).join("off_gcd_abilities.rs");
    let mut file = BufWriter::new(fs::File::create(&path).unwrap());

    let mut builder = phf_codegen::Set::new();
    for id in &ids {
        builder.entry(*id);
    }

    writeln!(
        file,
        "pub static OFF_GCD_ABILITIES: phf::Set<i64> = {};",
        builder.build()
    )
    .unwrap();
}

fn generate_interrupt_abilities_set(out_dir: &str) {
    let csv = fs::read_to_string("data/interrupts.csv").expect("failed to read interrupts.csv");

    let mut ids: Vec<i64> = csv
        .lines()
        .skip(1)
        .filter_map(|line| line.trim().parse::<i64>().ok())
        .collect();
    ids.sort_unstable();
    ids.dedup();

    let path = Path::new(out_dir).join("interrupt_abilities.rs");
    let mut file = BufWriter::new(fs::File::create(&path).unwrap());

    let mut builder = phf_codegen::Set::new();
    for id in &ids {
        builder.entry(*id);
    }

    writeln!(
        file,
        "pub static INTERRUPT_ABILITIES: phf::Set<i64> = {};",
        builder.build()
    )
    .unwrap();
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

fn generate_discipline_abilities_map(out_dir: &str) {
    let csv = fs::read_to_string("data/discipline_unique_abilities.csv")
        .expect("failed to read discipline_unique_abilities.csv");

    // Ability ID → discipline GUID. BTreeMap for deterministic output (sorted by key)
    let mut entries = BTreeMap::new();
    for line in csv.lines().skip(1) {
        let fields: Vec<&str> = line.split(',').collect();
        if fields.len() < 4 {
            continue;
        }
        let discipline_id: i64 = match fields[1].trim().parse() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let ability_id: i64 = match fields[3].trim().parse() {
            Ok(v) => v,
            Err(_) => continue,
        };
        entries.insert(ability_id, discipline_id);
    }

    let path = Path::new(out_dir).join("discipline_abilities.rs");
    let mut file = BufWriter::new(fs::File::create(&path).unwrap());

    let mut builder = phf_codegen::Map::new();
    let values: Vec<_> = entries.iter().map(|(id, disc)| (*id, format!("{disc}i64"))).collect();
    for (id, disc) in &values {
        builder.entry(*id, disc);
    }

    writeln!(
        file,
        "pub static DISCIPLINE_ABILITIES: phf::Map<i64, i64> = {};",
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

/// Republic/Imperial mirror pairs from the full ability dump.
/// Columns used: fqn(0), global_combat_id(1), name(2), mirror_fqn(6).
fn generate_mirror_abilities_map(out_dir: &str) {
    let csv = fs::read_to_string("data/ability_data.csv").expect("failed to read ability_data.csv");
    let csv = csv.trim_start_matches('\u{feff}');

    const IMPERIAL: [&str; 4] = ["agent", "bounty_hunter", "sith_warrior", "sith_inquisitor"];

    // Pass 1: fqn → (id, name, is_imperial) for every row that has a mirror.
    let mut by_fqn: BTreeMap<String, (i64, String, bool)> = BTreeMap::new();
    let mut pairs: Vec<(String, String)> = Vec::new();
    for line in csv.lines().skip(1) {
        let f = split_csv_line(line);
        if f.len() < 7 || f[6].is_empty() {
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
        entries.insert(
            *id,
            format!("MirrorAbility {{ mirror_id: {mid}i64, mirror_name: {mname:?}, is_imperial: {is_imperial} }}"),
        );
    }

    let path = Path::new(out_dir).join("mirror_abilities.rs");
    let mut file = BufWriter::new(fs::File::create(&path).unwrap());

    let mut builder = phf_codegen::Map::new();
    for (id, entry) in &entries {
        builder.entry(*id, entry);
    }

    writeln!(
        file,
        "pub static MIRROR_ABILITIES: phf::Map<i64, MirrorAbility> = {};",
        builder.build()
    )
    .unwrap();
}
