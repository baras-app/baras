//! Effect-tracker timeline replay (`--effects-timeline`).
//!
//! Replays a whole log through a live-mode `ParsingSession` with the bundled
//! effect and encounter definitions, ticks the tracker in log time after every
//! event, and writes one line per observable change. Only the tracker's public
//! outputs are read, so the timeline stays comparable across internal
//! refactors: an unchanged timeline means unchanged behavior on that log.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::NaiveDateTime;
use encoding_rs::WINDOWS_1252;

use baras_core::boss::{build_area_index, load_bosses_from_file};
use baras_core::combat_log::LogParser;
use baras_core::context::{DefinitionLoader, ParsingSession, parse_log_filename, resolve};
use baras_core::effects::{DefinitionSet, EffectDefinition};

/// (definition id, source id, target id)
type Key = (String, i64, i64);

struct Snap {
    expires_at: Option<NaiveDateTime>,
    stacks: u8,
    displayed: bool,
}

/// Event counts per definition, for the coverage summary
#[derive(Default)]
struct Counts {
    apply: u32,
    expires: u32,
    stacks: u32,
    hide: u32,
    revive: u32,
    end: u32,
    alert: u32,
}

pub fn run(log: &Path, definitions_root: Option<&Path>, out: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let root = definitions_root.map(Path::to_path_buf).unwrap_or_else(|| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("core/definitions")
    });

    let mut defs = DefinitionSet::new();
    defs.add_from_dir(&root.join("effects"));
    eprintln!(
        "Loaded {} effect definitions from {}",
        defs.effects.len(),
        root.display()
    );

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

    let mut session = ParsingSession::new(log.to_path_buf(), defs.clone());
    let area_index = build_area_index(&root.join("encounters"))?;
    let loader: DefinitionLoader = Box::new(move |area_id| {
        area_index
            .get(&area_id)
            .and_then(|entry| load_bosses_from_file(&entry.file_path).ok())
    });
    session.set_definition_loader(Arc::new(loader));
    let tracker = session.effect_tracker().ok_or("live session has no effect tracker")?;

    let mut w = BufWriter::new(File::create(out)?);
    let mut prev: BTreeMap<Key, Snap> = BTreeMap::new();
    let mut counts: BTreeMap<String, Counts> = BTreeMap::new();
    let mut countdowns_prev: Vec<String> = Vec::new();
    let mut lines_out: Vec<String> = Vec::new();

    for (line_num, line) in content.lines().enumerate() {
        let Some(event) = parser.parse_line(line_num as u64, line) else {
            continue;
        };
        let ts = event.timestamp;
        session.process_event(event);

        let mut t = tracker.lock().unwrap_or_else(|p| p.into_inner());
        t.tick_at(ts);

        // ─── Alerts ─────────────────────────────────────────────────────────
        // Countdown alerts re-fire every tick; record only when one starts.
        let mut countdowns: Vec<String> = Vec::new();
        for alert in t.take_fired_alerts() {
            if alert.remaining_secs.is_some() {
                if !countdowns_prev.contains(&alert.id) {
                    lines_out.push(format!("COUNTDOWN {}", alert.id));
                    counts.entry(alert.id.clone()).or_default().alert += 1;
                }
                countdowns.push(alert.id);
            } else {
                lines_out.push(format!(
                    "ALERT {} text={:?} audio={} file={:?}",
                    alert.id, alert.text, alert.audio_enabled, alert.audio_file
                ));
                counts.entry(alert.id).or_default().alert += 1;
            }
        }
        countdowns_prev = countdowns;

        for target in t.take_new_targets() {
            lines_out.push(format!("REGISTER {} {}", resolve(target.name), target.entity_id));
        }

        // ─── Effect state diff ──────────────────────────────────────────────
        let mut next: BTreeMap<Key, Snap> = BTreeMap::new();
        for e in t.active_effects() {
            let key = (e.definition_id.clone(), e.source_entity_id, e.target_entity_id);
            let snap = Snap {
                expires_at: e.expires_at,
                stacks: e.stacks,
                displayed: e.is_displayed(),
            };
            let label = || {
                format!(
                    "{} {}#{}>{}#{}",
                    e.definition_id,
                    resolve(e.source_name),
                    e.source_entity_id,
                    resolve(e.target_name),
                    e.target_entity_id
                )
            };
            let c = counts.entry(e.definition_id.clone()).or_default();
            match prev.get(&key) {
                None => {
                    c.apply += 1;
                    lines_out.push(format!(
                        "APPLY {} stacks={} expires={} shown={} text={:?}",
                        label(),
                        snap.stacks,
                        fmt_time(snap.expires_at),
                        snap.displayed,
                        e.display_text
                    ));
                }
                Some(old) => {
                    if old.expires_at != snap.expires_at {
                        c.expires += 1;
                        lines_out.push(format!("EXPIRES {} {}", label(), fmt_time(snap.expires_at)));
                    }
                    if old.stacks != snap.stacks {
                        c.stacks += 1;
                        lines_out.push(format!("STACKS {} {}", label(), snap.stacks));
                    }
                    if old.displayed != snap.displayed {
                        if snap.displayed {
                            c.revive += 1;
                        } else {
                            c.hide += 1;
                        }
                        lines_out.push(format!(
                            "{} {}",
                            if snap.displayed { "REVIVE" } else { "HIDE" },
                            label()
                        ));
                    }
                }
            }
            next.insert(key, snap);
        }
        for key in prev.keys().filter(|k| !next.contains_key(*k)) {
            counts.entry(key.0.clone()).or_default().end += 1;
            lines_out.push(format!("END {} {}>{}", key.0, key.1, key.2));
        }
        prev = next;
        drop(t);

        if !lines_out.is_empty() {
            // HashMap iteration order inside the tracker is arbitrary
            lines_out.sort();
            let stamp = ts.format("%H:%M:%S%.3f");
            for l in lines_out.drain(..) {
                writeln!(w, "{stamp} {l}")?;
            }
        }
    }
    w.flush()?;

    print_coverage(&defs, &counts);
    Ok(())
}

fn fmt_time(t: Option<NaiveDateTime>) -> String {
    t.map_or_else(|| "-".to_string(), |t| t.format("%H:%M:%S%.3f").to_string())
}

/// Per code path: definitions that exercise it and how many timeline events
/// they produced. A zero row means this log can't confirm behavior there.
fn print_coverage(defs: &DefinitionSet, counts: &BTreeMap<String, Counts>) {
    let paths: [(&str, fn(&EffectDefinition) -> bool); 8] = [
        ("single_instance", |d| d.single_instance_per_target),
        ("aoe_refresh", |d| d.is_aoe_refresh),
        ("aoe_immediate", |d| d.aoe_refresh_immediate),
        ("refresh_by_name", |d| {
            d.refresh_abilities
                .iter()
                .any(|r| matches!(r.ability(), baras_core::AbilitySelector::Name(_)))
        }),
        ("dot_tracker", |d| d.displays_on(baras_core::DisplayTarget::DotTracker)),
        ("modifiers", |d| !d.modifiers.is_empty()),
        ("refreshed_on_modify", |d| d.is_refreshed_on_modify),
        ("alert_on_apply", |d| {
            d.alert_on == baras_core::effects::AlertTrigger::OnApply
        }),
    ];

    eprintln!(
        "{:<20} {:>5} {:>8} {:>8} {:>7} {:>6}",
        "path", "defs", "applies", "changes", "alerts", "ends"
    );
    for (name, pred) in paths {
        let (mut n, mut apply, mut changes, mut alerts, mut ends) = (0, 0, 0, 0, 0);
        for def in defs.enabled().filter(|d| pred(d)) {
            if let Some(c) = counts.get(&def.id) {
                n += 1;
                apply += c.apply;
                changes += c.expires + c.stacks + c.hide + c.revive;
                alerts += c.alert;
                ends += c.end;
            }
        }
        eprintln!("{name:<20} {n:>5} {apply:>8} {changes:>8} {alerts:>7} {ends:>6}");
    }
}
