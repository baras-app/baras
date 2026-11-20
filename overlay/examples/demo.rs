//! Standalone overlay demos, for working on rendering without the app.
//!
//! Run with: cargo run -p baras-overlay --example demo -- [OPTION]
//!
//!   --metric [N]  DPS meter with N players (default 4)
//!   --raid        8 raid frames in move mode
//!   --timers      Boss timer bars counting down

use std::time::{Duration, Instant};

use baras_core::context::{OverlayAppearanceConfig, TimerOverlayConfig};
use baras_overlay::{
    InteractionMode, MetricEntry, MetricOverlay, Overlay, OverlayConfig, PlayerRole, RaidEffect, RaidFrame,
    RaidGridLayout, RaidOverlay, RaidOverlayConfig, TimerData, TimerEntry, TimerOverlay, colors,
};
use baras_types::ClassIconMode;

fn window(namespace: &str, width: u32, height: u32) -> OverlayConfig {
    OverlayConfig {
        x: 300,
        y: 200,
        width,
        height,
        namespace: namespace.to_string(),
        snap_to_grid: true,
        click_through: false,
        target_monitor_id: None,
    }
}

/// Render at `fps` until the window closes; `tick` gets the seconds since start.
fn run<O: Overlay>(mut overlay: O, fps: u64, mut tick: impl FnMut(&mut O, f32)) {
    let start = Instant::now();
    let frame = Duration::from_millis(1000 / fps);
    let mut last_frame = start - frame;
    println!("Overlay running. Press Ctrl+C to exit.");
    while overlay.poll_events() {
        if last_frame.elapsed() >= frame {
            tick(&mut overlay, start.elapsed().as_secs_f32());
            overlay.render();
            last_frame = Instant::now();
        }
        std::thread::sleep(Duration::from_millis(if overlay.is_interactive() { 4 } else { 16 }));
    }
}

fn metric(players: u32) -> Result<(), baras_overlay::PlatformError> {
    let appearance = OverlayAppearanceConfig {
        bar_gradient: true,
        ..Default::default()
    };
    let mut overlay = MetricOverlay::new(
        window("baras-dps-metric", 280, 60 + 22 * players),
        "DPS Meter",
        appearance,
        true,
        false,
        1.0,
        ClassIconMode::Class,
        1.0,
        true,
        false,
        0.32,
    )?;
    let entries = (0..players)
        .map(|i| {
            let value = 15_000 - 800 * i as i64;
            MetricEntry {
                name: format!("Player {}", i + 1),
                value,
                max_value: 15_000,
                total_value: value * 180,
                color: colors::dps_bar_fill(),
                split_value: (i % 2 == 0).then_some(value * 7 / 10),
                total_split_value: (i % 2 == 0).then_some(value * 126),
                split_color: None,
                class_icon: None,
                role: None,
                discipline_icon: None,
                class_name: None,
                display_value: None,
                display_total: None,
                player: None,
                is_local: i == 0,
                is_enemy: false,
            }
        })
        .collect();
    overlay.set_entries(entries);
    run(overlay, 10, |overlay, elapsed| {
        let secs = elapsed as u64;
        overlay.set_title(&format!("DPS Meter - {}:{:02}", secs / 60, secs % 60));
    });
    Ok(())
}

fn raid() -> Result<(), baras_overlay::PlatformError> {
    let players = [
        (
            "Tanky McTank",
            PlayerRole::Tank,
            "juggernaut.png",
            Some(("Guard", [100, 150, 220])),
        ),
        (
            "Healz4Days",
            PlayerRole::Healer,
            "sorcerer.png",
            Some(("Resurgence", [100, 220, 100])),
        ),
        (
            "PewPewLazors",
            PlayerRole::Dps,
            "sniper.png",
            Some(("Kolto Probe", [150, 255, 150])),
        ),
        ("StabbySith", PlayerRole::Dps, "assassin.png", None),
        (
            "OffTankOT",
            PlayerRole::Tank,
            "powertech.png",
            Some(("Saber Ward", [255, 200, 100])),
        ),
        ("HoTsOnYou", PlayerRole::Healer, "operative.png", None),
        (
            "StandInFire",
            PlayerRole::Dps,
            "marauder.png",
            Some(("Burning", [255, 100, 50])),
        ),
    ];
    let mut frames: Vec<RaidFrame> = players
        .into_iter()
        .enumerate()
        .map(|(slot, (name, role, icon, effect))| RaidFrame {
            slot: slot as u8,
            player_id: Some(1001 + slot as i64),
            name: name.to_string(),
            hp_percent: 0.0,
            role,
            class_icon: Some(icon.to_string()),
            effects: effect
                .into_iter()
                .map(|(effect_name, [r, g, b])| {
                    RaidEffect::new(100 * (slot as u64 + 1), effect_name)
                        .with_color(tiny_skia::Color::from_rgba8(r, g, b, 255))
                })
                .collect(),
            is_self: slot == 0,
            ambiguous: false,
        })
        .collect();
    frames.push(RaidFrame::empty(7));

    let layout = RaidGridLayout { columns: 2, rows: 4 };
    let mut overlay = RaidOverlay::new(window("baras-raid", 220, 200), layout, RaidOverlayConfig::default())?;
    overlay.set_frames(frames);
    overlay.set_interaction_mode(InteractionMode::Move);
    run(overlay, 60, |_, _| {});
    Ok(())
}

fn timers() -> Result<(), baras_overlay::PlatformError> {
    let config = TimerOverlayConfig {
        bar_gradient: true,
        ..Default::default()
    };
    let overlay = TimerOverlay::new(window("baras-timers", 240, 180), config, "Timers")?;
    let mechanics: [(&str, f32, [u8; 4]); 5] = [
        ("Doom", 30.0, [200, 50, 50, 255]),
        ("Lightning Storm", 20.0, [100, 150, 255, 255]),
        ("Adds Spawn", 45.0, [180, 100, 220, 255]),
        ("Enrage Check", 60.0, [255, 180, 50, 255]),
        ("A VERY LONG Tank Swap", 15.0, [100, 220, 100, 255]),
    ];
    run(overlay, 10, |overlay, elapsed| {
        let entries = mechanics
            .iter()
            .map(|&(name, cycle, color)| TimerEntry {
                name: name.to_string(),
                remaining_secs: cycle - elapsed % cycle,
                total_secs: cycle,
                color,
                icon_ability_id: None,
                icon: None,
            })
            .collect();
        overlay.set_data(TimerData { entries });
    });
    Ok(())
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env().add_directive(tracing::Level::INFO.into()))
        .init();

    let args: Vec<String> = std::env::args().collect();
    let result = match args.get(1).map(String::as_str).unwrap_or("--metric") {
        "--metric" => metric(args.get(2).and_then(|n| n.parse().ok()).unwrap_or(4)),
        "--raid" => raid(),
        "--timers" => timers(),
        _ => {
            println!("Usage: cargo run -p baras-overlay --example demo -- [--metric [N] | --raid | --timers]");
            Ok(())
        }
    };
    if let Err(e) = result {
        tracing::error!(error = %e, "Failed to create overlay");
    }
}
