//! Example overlay application demonstrating the overlays
//!
//! Run with: cargo run -p baras-overlay
//!
//! Use command line args to select overlay type:
//!   --metric      (default) - DPS meter overlay with 4 players
//!   --metric-8    - Moveable DPS meter with 8 players
//!   --metric-16   - Moveable DPS meter with 16 players
//!   --raid        - Three raid overlays side-by-side showing all interaction modes
//!   --raid-timers - 16 frames with ticking effect timers
//!   --timers       - Boss timer overlay with sample countdown bars
//!   --challenges   - Challenge overlay (vertical, 2-col format)
//!   --challenges-h - Challenge overlay (horizontal, 3-col with per-second)

use std::env;
use std::time::{Duration, Instant};

mod examples {
    use super::*;
    use baras_core::context::{
        ChallengeColumns, ChallengeLayout, ChallengeOverlayConfig, OverlayAppearanceConfig,
        TimerOverlayConfig,
    };
    use baras_overlay::{
        ChallengeData, ChallengeEntry, ChallengeOverlay, Color, InteractionMode, MetricEntry,
        MetricOverlay, Overlay, OverlayConfig, PlayerContribution, PlayerRole, RaidEffect,
        RaidFrame, RaidGridLayout, RaidOverlay, RaidOverlayConfig, TimerData, TimerEntry,
        TimerOverlay, colors,
    };

    pub fn run_metric_overlay() {
        let config = OverlayConfig {
            x: 500,
            y: 500,
            width: 280,
            height: 200,
            namespace: "baras-dps-metric".to_string(),
            click_through: false,
            target_monitor_id: None,
        };

        let appearance = OverlayAppearanceConfig::default();
        let mut metric = match MetricOverlay::new(config, "DPS Meter", appearance, 180) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("Failed to create overlay: {}", e);
                return;
            }
        };

        let entries = vec![
            MetricEntry {
                name: "Player 1".to_string(),
                value: 12500,
                max_value: 15000,
                total_value: 2_500_000,
                color: colors::dps_bar_fill(),
            },
            MetricEntry {
                name: "Player 2".to_string(),
                value: 10200,
                max_value: 15000,
                total_value: 1_800_000,
                color: colors::dps_bar_fill(),
            },
            MetricEntry {
                name: "Player 3".to_string(),
                value: 8700,
                max_value: 15000,
                total_value: 1_200_000,
                color: colors::hps_bar_fill(),
            },
            MetricEntry {
                name: "Player 4".to_string(),
                value: 6100,
                max_value: 15000,
                total_value: 800_000,
                color: colors::tank_bar_fill(),
            },
        ];

        metric.set_entries(entries);

        let start = Instant::now();
        let mut last_frame = Instant::now();
        let frame_duration = Duration::from_millis(16);

        println!("Metric overlay running. Press Ctrl+C to exit.");

        loop {
            if !metric.poll_events() {
                break;
            }

            let now = Instant::now();
            if now.duration_since(last_frame) >= frame_duration {
                let elapsed = start.elapsed().as_secs();
                metric.set_title(&format!("DPS Meter - {}:{:02}", elapsed / 60, elapsed % 60));
                metric.render();
                last_frame = now;
            }

            // Sleep based on interactive state for CPU efficiency
            let sleep_ms = if metric.is_interactive() { 1 } else { 16 };
            std::thread::sleep(Duration::from_millis(sleep_ms));
        }
    }

    /// Run a moveable metric overlay with 8 players (standard ops group)
    pub fn run_metric_overlay_8() {
        let config = OverlayConfig {
            x: 200,
            y: 100,
            width: 300,
            height: 280,
            namespace: "baras-dps-metric-8".to_string(),
            click_through: false,
            target_monitor_id: None,
        };

        let appearance = OverlayAppearanceConfig {
            max_entries: 8,
            ..Default::default()
        };

        let mut metric = match MetricOverlay::new(config, "DPS Meter (8 Players)", appearance, 180)
        {
            Ok(m) => m,
            Err(e) => {
                eprintln!("Failed to create overlay: {}", e);
                return;
            }
        };

        metric.set_move_mode(true);

        let player_data = [
            ("Fury Mara", 26500, colors::dps_bar_fill()),
            ("Carnage Mara", 24800, colors::dps_bar_fill()),
            ("Lightning Sorc", 23200, colors::dps_bar_fill()),
            ("Virulence Sniper", 21500, colors::dps_bar_fill()),
            ("Corruption Sorc", 8200, colors::hps_bar_fill()),
            ("Medicine Op", 7400, colors::hps_bar_fill()),
            ("Immortal Jugg", 4200, colors::tank_bar_fill()),
            ("Shield Tech PT", 3800, colors::tank_bar_fill()),
        ];

        let max_value = player_data[0].1;

        let entries: Vec<MetricEntry> = player_data
            .iter()
            .map(|(name, value, color)| MetricEntry {
                name: name.to_string(),
                value: *value,
                max_value,
                total_value: value * 180,
                color: *color,
            })
            .collect();

        metric.set_entries(entries);

        let start = Instant::now();
        let mut last_frame = Instant::now();
        let frame_duration = Duration::from_millis(16);

        println!("┌─────────────────────────────────────────────────────────────┐");
        println!("│        8-Player Metric Overlay - Move Mode Enabled          │");
        println!("├─────────────────────────────────────────────────────────────┤");
        println!("│  Drag anywhere to move the overlay                          │");
        println!("│  Drag bottom-right corner to resize                         │");
        println!("│  Shows 4 DPS + 2 Healers + 2 Tanks                          │");
        println!("├─────────────────────────────────────────────────────────────┤");
        println!("│  Press Ctrl+C to exit                                       │");
        println!("└─────────────────────────────────────────────────────────────┘");

        loop {
            if !metric.poll_events() {
                break;
            }

            let now = Instant::now();
            if now.duration_since(last_frame) >= frame_duration {
                let elapsed = start.elapsed().as_secs();
                metric.set_title(&format!("DPS Meter - {}:{:02}", elapsed / 60, elapsed % 60));
                metric.render();
                last_frame = now;
            }

            let sleep_ms = if metric.is_interactive() { 4 } else { 16 };
            std::thread::sleep(Duration::from_millis(sleep_ms));
        }
    }

    /// Run a moveable metric overlay with 16 players
    /// Demonstrates the full raid-size DPS meter in move mode
    pub fn run_metric_overlay_16() {
        let config = OverlayConfig {
            x: 200,
            y: 100,
            width: 320,
            height: 450,
            namespace: "baras-dps-metric-16".to_string(),
            click_through: false, // Moveable
            target_monitor_id: None,
        };

        let appearance = OverlayAppearanceConfig {
            max_entries: 16,
            ..Default::default()
        };

        let mut metric = match MetricOverlay::new(config, "DPS Meter (16 Players)", appearance, 180)
        {
            Ok(m) => m,
            Err(e) => {
                eprintln!("Failed to create overlay: {}", e);
                return;
            }
        };

        // Enable move mode so it's draggable
        metric.set_move_mode(true);

        // Create 16 players with varied DPS values
        let player_data = [
            ("Fury Mara", 28500, colors::dps_bar_fill()),
            ("Carnage Mara", 26800, colors::dps_bar_fill()),
            ("Annihilation", 25200, colors::dps_bar_fill()),
            ("Lightning Sorc", 24100, colors::dps_bar_fill()),
            ("Virulence Sniper", 23500, colors::dps_bar_fill()),
            ("Arsenal Merc", 22800, colors::dps_bar_fill()),
            ("IO Merc", 21200, colors::dps_bar_fill()),
            ("Hatred Assassin", 20500, colors::dps_bar_fill()),
            ("Corruption Sorc", 8200, colors::hps_bar_fill()),
            ("Medicine Op", 7800, colors::hps_bar_fill()),
            ("Bodyguard Merc", 7100, colors::hps_bar_fill()),
            ("Seer Sage", 6500, colors::hps_bar_fill()),
            ("Immortal Jugg", 4200, colors::tank_bar_fill()),
            ("Shield Tech PT", 3800, colors::tank_bar_fill()),
            ("Darkness Sin", 3500, colors::tank_bar_fill()),
            ("Defense Guard", 3100, colors::tank_bar_fill()),
        ];

        let max_value = player_data[0].1; // Highest DPS for bar scaling

        let entries: Vec<MetricEntry> = player_data
            .iter()
            .map(|(name, value, color)| MetricEntry {
                name: name.to_string(),
                value: *value,
                max_value,
                total_value: *value * 180, // ~3 min encounter
                color: *color,
            })
            .collect();

        metric.set_entries(entries);

        let start = Instant::now();
        let mut last_frame = Instant::now();
        let frame_duration = Duration::from_millis(16);

        println!("┌─────────────────────────────────────────────────────────────┐");
        println!("│        16-Player Metric Overlay - Move Mode Enabled         │");
        println!("├─────────────────────────────────────────────────────────────┤");
        println!("│  Drag anywhere to move the overlay                          │");
        println!("│  Drag bottom-right corner to resize                         │");
        println!("│  Shows 8 DPS + 4 Healers + 4 Tanks                          │");
        println!("├─────────────────────────────────────────────────────────────┤");
        println!("│  Press Ctrl+C to exit                                       │");
        println!("└─────────────────────────────────────────────────────────────┘");

        loop {
            if !metric.poll_events() {
                break;
            }

            let now = Instant::now();
            if now.duration_since(last_frame) >= frame_duration {
                let elapsed = start.elapsed().as_secs();
                metric.set_title(&format!("DPS Meter - {}:{:02}", elapsed / 60, elapsed % 60));
                metric.render();
                last_frame = now;
            }

            // Faster polling when interactive (dragging/resizing)
            let sleep_ms = if metric.is_interactive() { 4 } else { 16 };
            std::thread::sleep(Duration::from_millis(sleep_ms));
        }
    }

    /// Run three raid overlays side-by-side demonstrating each interaction mode
    pub fn run_raid_overlay() {
        let layout = RaidGridLayout {
            columns: 2,
            rows: 4,
        };
        let raid_config = RaidOverlayConfig::default();
        let test_frames = create_test_frames();

        // Create three overlays in a row, each in a different mode
        // Column 1: Normal (click-through)
        let config_normal = OverlayConfig {
            x: 50,
            y: 100,
            width: 220,
            height: 200,
            namespace: "baras-raid-normal".to_string(),
            click_through: true, // Will be set by InteractionMode::Normal
            target_monitor_id: None,
        };

        // Column 2: Move mode
        let config_move = OverlayConfig {
            x: 290,
            y: 100,
            width: 220,
            height: 200,
            namespace: "baras-raid-move".to_string(),
            click_through: false,
            target_monitor_id: None,
        };

        // Column 3: Rearrange mode
        let config_rearrange = OverlayConfig {
            x: 530,
            y: 100,
            width: 220,
            height: 200,
            namespace: "baras-raid-rearrange".to_string(),
            click_through: false,
            target_monitor_id: None,
        };

        let mut overlay_normal =
            match RaidOverlay::new(config_normal, layout, raid_config.clone(), 180) {
                Ok(o) => o,
                Err(e) => {
                    eprintln!("Failed to create normal overlay: {}", e);
                    return;
                }
            };

        let mut overlay_move = match RaidOverlay::new(config_move, layout, raid_config.clone(), 180)
        {
            Ok(o) => o,
            Err(e) => {
                eprintln!("Failed to create move overlay: {}", e);
                return;
            }
        };

        let mut overlay_rearrange =
            match RaidOverlay::new(config_rearrange, layout, raid_config, 180) {
                Ok(o) => o,
                Err(e) => {
                    eprintln!("Failed to create rearrange overlay: {}", e);
                    return;
                }
            };

        // Set up each overlay with test data and its interaction mode
        overlay_normal.set_frames(test_frames.clone());
        overlay_normal.set_interaction_mode(InteractionMode::Normal);

        overlay_move.set_frames(test_frames.clone());
        overlay_move.set_interaction_mode(InteractionMode::Move);

        overlay_rearrange.set_frames(test_frames);
        overlay_rearrange.set_interaction_mode(InteractionMode::Rearrange);

        let mut last_frame = Instant::now();
        let frame_duration = Duration::from_millis(16); // ~60fps

        println!("┌─────────────────────────────────────────────────────────────┐");
        println!("│          Raid Overlay Demo - Three Interaction Modes        │");
        println!("├─────────────────────────────────────────────────────────────┤");
        println!("│  LEFT:   Normal Mode   - Clicks pass through to game        │");
        println!("│  MIDDLE: Move Mode     - Drag to move, resize corner works  │");
        println!("│  RIGHT:  Rearrange Mode- Click frames to swap positions     │");
        println!("├─────────────────────────────────────────────────────────────┤");
        println!("│  Press Ctrl+C to exit                                       │");
        println!("└─────────────────────────────────────────────────────────────┘");

        loop {
            // Poll all overlays - if any closes, exit
            if !overlay_normal.poll_events()
             //   || !overlay_move.poll_events()
                || !overlay_rearrange.poll_events()
            {
                break;
            }

            let now = Instant::now();
            if now.duration_since(last_frame) >= frame_duration {
                overlay_normal.render();
                overlay_move.render();
                overlay_rearrange.render();
                last_frame = now;
            }

            // Adaptive sleep: faster polling when any overlay is interactive
            // In production, you'd also check for data changes (dirty flag)
            let any_interactive =
                overlay_move.is_interactive() || overlay_rearrange.is_interactive();
            let sleep_ms = if any_interactive { 4 } else { 16 };
            std::thread::sleep(Duration::from_millis(sleep_ms));
        }
    }

    fn create_test_frames() -> Vec<RaidFrame> {
        vec![
            // Slot 0: Self (Tank)
            RaidFrame {
                slot: 0,
                player_id: Some(1001),
                name: "Tanky McTank".to_string(),
                hp_percent: 0.0,
                role: PlayerRole::Tank,
                effects: vec![
                    RaidEffect::new(100, "Guard")
                        .with_color(tiny_skia::Color::from_rgba8(100, 150, 220, 255)),
                ],
                is_self: true,
            },
            // Slot 1: Healer
            RaidFrame {
                slot: 1,
                player_id: Some(1002),
                name: "Healz4Days".to_string(),
                hp_percent: 0.0,
                role: PlayerRole::Healer,
                effects: vec![
                    RaidEffect::new(200, "Resurgence")
                        .with_color(tiny_skia::Color::from_rgba8(100, 220, 100, 255))
                        .with_charges(2),
                ],
                is_self: false,
            },
            // Slot 2: DPS
            RaidFrame {
                slot: 2,
                player_id: Some(1003),
                name: "PewPewLazors".to_string(),
                hp_percent: 0.0,
                role: PlayerRole::Dps,
                effects: vec![
                    RaidEffect::new(300, "Kolto Probe")
                        .with_color(tiny_skia::Color::from_rgba8(150, 255, 150, 255)),
                    RaidEffect::new(301, "Force Armor")
                        .with_color(tiny_skia::Color::from_rgba8(200, 200, 100, 255)),
                ],
                is_self: false,
            },
            // Slot 3: DPS (no effects)
            RaidFrame {
                slot: 3,
                player_id: Some(1004),
                name: "StabbySith".to_string(),
                hp_percent: 0.0,
                role: PlayerRole::Dps,
                effects: vec![],
                is_self: false,
            },
            // Slot 4: Off-tank
            RaidFrame {
                slot: 4,
                player_id: Some(1005),
                name: "OffTankOT".to_string(),
                hp_percent: 0.0,
                role: PlayerRole::Tank,
                effects: vec![
                    RaidEffect::new(400, "Saber Ward")
                        .with_color(tiny_skia::Color::from_rgba8(255, 200, 100, 255)),
                ],
                is_self: false,
            },
            // Slot 5: Healer (no effects)
            RaidFrame {
                slot: 5,
                player_id: Some(1006),
                name: "HoTsOnYou".to_string(),
                hp_percent: 0.0,
                role: PlayerRole::Healer,
                effects: vec![],
                is_self: false,
            },
            // Slot 6: DPS with debuff
            RaidFrame {
                slot: 6,
                player_id: Some(1007),
                name: "StandInFire".to_string(),
                hp_percent: 0.0,
                role: PlayerRole::Dps,
                effects: vec![
                    RaidEffect::new(500, "Burning")
                        .with_color(tiny_skia::Color::from_rgba8(255, 100, 50, 255))
                        .with_is_buff(false),
                ],
                is_self: false,
            },
            // Slot 7: Empty slot
            RaidFrame::empty(7),
        ]
    }

    /// Run a single raid overlay in Normal mode with 16 frames × 2 effects = 32 ticking timers
    /// This demonstrates the 10 FPS frame rate limiting for effect countdown rendering
    ///
    /// Layout shows 4 different effect styles (one per row):
    /// - Row 0: Non-opaque, no text
    /// - Row 1: Opaque, no text
    /// - Row 2: Non-opaque, with text
    /// - Row 3: Opaque, with text
    pub fn run_raid_timer_stress_test() {
        // 4x4 grid = 16 frames
        let layout = RaidGridLayout {
            columns: 4,
            rows: 4,
        };
        let raid_config = RaidOverlayConfig {
            max_effects_per_frame: 4,
            effect_size: 20.0, // 1.4x scale (default 14 * 1.4 ≈ 20)
            ..Default::default()
        };

        let config = OverlayConfig {
            x: 100,
            y: 100,
            width: 500, // Wider to accommodate larger effects
            height: 450,
            namespace: "baras-raid-timer-test".to_string(),
            click_through: true,
            target_monitor_id: None,
        };

        let mut overlay = match RaidOverlay::new(config, layout, raid_config, 180) {
            Ok(o) => o,
            Err(e) => {
                eprintln!("Failed to create overlay: {}", e);
                return;
            }
        };

        // Create 16 frames with 2 effects each (32 total effects with duration timers)
        let frames = create_timer_stress_frames();
        overlay.set_frames(frames);
        overlay.set_interaction_mode(InteractionMode::Normal);

        let mut last_frame = Instant::now();
        let frame_duration = Duration::from_millis(16);

        println!("┌─────────────────────────────────────────────────────────────┐");
        println!("│       Raid Timer Stress Test - 32 Ticking Effect Timers     │");
        println!("├─────────────────────────────────────────────────────────────┤");
        println!("│  16 frames × 2 effects = 32 duration countdown bars         │");
        println!("│  Render rate: 10 FPS (capped for CPU efficiency)            │");
        println!("│  Effect size: 20px (1.4x scale)                             │");
        println!("├─────────────────────────────────────────────────────────────┤");
        println!("│  Row 0: Non-opaque, no text                                 │");
        println!("│  Row 1: Opaque, no text                                     │");
        println!("│  Row 2: Non-opaque, with text (stack count)                 │");
        println!("│  Row 3: Opaque, with text (stack count)                     │");
        println!("├─────────────────────────────────────────────────────────────┤");
        println!("│  Press Ctrl+C to exit                                       │");
        println!("└─────────────────────────────────────────────────────────────┘");

        loop {
            if !overlay.poll_events() {
                break;
            }

            let now = Instant::now();
            if now.duration_since(last_frame) >= frame_duration {
                overlay.render();
                last_frame = now;
            }

            std::thread::sleep(Duration::from_millis(16));
        }
    }

    /// Create 16 frames with 2 ticking effects each for the stress test
    /// Demonstrates 4 different effect visual styles across the 4 rows
    fn create_timer_stress_frames() -> Vec<RaidFrame> {
        let player_names = [
            // Row 0: Non-opaque, no text
            "TankOne",
            "TankTwo",
            "HealerA",
            "HealerB",
            // Row 1: Opaque, no text
            "DpsAlpha",
            "DpsBeta",
            "DpsGamma",
            "DpsDelta",
            // Row 2: Non-opaque, with text
            "RangedOne",
            "RangedTwo",
            "MeleeOne",
            "MeleeTwo",
            // Row 3: Opaque, with text
            "SupportA",
            "SupportB",
            "OffTank",
            "FlexDps",
        ];

        let roles = [
            PlayerRole::Tank,
            PlayerRole::Tank,
            PlayerRole::Healer,
            PlayerRole::Healer,
            PlayerRole::Dps,
            PlayerRole::Dps,
            PlayerRole::Dps,
            PlayerRole::Dps,
            PlayerRole::Dps,
            PlayerRole::Dps,
            PlayerRole::Dps,
            PlayerRole::Dps,
            PlayerRole::Healer,
            PlayerRole::Healer,
            PlayerRole::Tank,
            PlayerRole::Dps,
        ];

        // Base colors (will be modified with alpha for opacity variations)
        let base_colors = [
            (100, 220, 100), // Green (HoT)
            (100, 150, 220), // Blue (Shield)
            (220, 180, 50),  // Yellow (Buff)
            (180, 100, 220), // Purple (Debuff)
        ];

        (0..16)
            .map(|slot| {
                let row = slot / 4; // 0, 1, 2, or 3

                // Determine opacity based on row (0,2 = non-opaque, 1,3 = opaque)
                // Non-opaque at 100 alpha allows icons to show through clearly
                let is_opaque = row == 1 || row == 3;
                let alpha: u8 = if is_opaque { 255 } else { 100 };

                // Determine if text should show (row 2,3 = with text via charges)
                // Vary digit counts: 1-digit, 2-digit, and 3-digit examples
                let has_text = row >= 2;
                let charges: u8 = if has_text {
                    match slot % 4 {
                        0 => 3,   // 1 digit
                        1 => 42,  // 2 digits
                        2 => 127, // 3 digits
                        _ => 8,   // 1 digit
                    }
                } else {
                    0
                };

                // Stagger durations so effects expire at different times
                let base_duration_1 = Duration::from_secs(15 + (slot as u64 * 2));
                let base_duration_2 = Duration::from_secs(20 + (slot as u64 * 2));

                let (r1, g1, b1) = base_colors[slot % 4];
                let (r2, g2, b2) = base_colors[(slot + 1) % 4];

                let effect1 = RaidEffect::new(slot as u64 * 10, format!("Effect{}", slot * 2))
                    .with_duration_from_now(base_duration_1)
                    .with_color(tiny_skia::Color::from_rgba8(r1, g1, b1, alpha))
                    .with_charges(charges);

                // Second effect gets different digit count
                let charges2: u8 = if has_text {
                    match slot % 4 {
                        0 => 15,  // 2 digits
                        1 => 99,  // 2 digits
                        2 => 5,   // 1 digit
                        _ => 255, // 3 digits (max u8)
                    }
                } else {
                    0
                };

                let effect2 =
                    RaidEffect::new(slot as u64 * 10 + 1, format!("Effect{}", slot * 2 + 1))
                        .with_duration_from_now(base_duration_2)
                        .with_color(tiny_skia::Color::from_rgba8(r2, g2, b2, alpha))
                        .with_charges(charges2);

                RaidFrame {
                    slot: slot as u8,
                    player_id: Some(2000 + slot as i64),
                    name: player_names[slot].to_string(),
                    hp_percent: 1.0,
                    role: roles[slot],
                    effects: vec![effect1, effect2],
                    is_self: slot == 0,
                }
            })
            .collect()
    }

    /// Run the timer overlay with sample boss mechanic timers
    /// Timers tick down in real-time and refresh when expired
    pub fn run_timer_overlay() {
        let config = OverlayConfig {
            x: 300,
            y: 200,
            width: 240,
            height: 180,
            namespace: "baras-timers".to_string(),
            click_through: false,
            target_monitor_id: None,
        };

        let timer_config = TimerOverlayConfig::default();

        let mut overlay = match TimerOverlay::new(config, timer_config, 180) {
            Ok(o) => o,
            Err(e) => {
                eprintln!("Failed to create timer overlay: {}", e);
                return;
            }
        };

        // Enable move mode for repositioning
        overlay.set_move_mode(true);

        let start = Instant::now();
        let mut last_frame = Instant::now();
        let frame_duration = Duration::from_millis(100); // 10 FPS for timers

        println!("┌─────────────────────────────────────────────────────────────┐");
        println!("│           Timer Overlay - Boss Mechanic Countdown           │");
        println!("├─────────────────────────────────────────────────────────────┤");
        println!("│  Timers tick down in real-time                              │");
        println!("│  Drag anywhere to move the overlay                          │");
        println!("│  Drag bottom-right corner to resize                         │");
        println!("├─────────────────────────────────────────────────────────────┤");
        println!("│  Press Ctrl+C to exit                                       │");
        println!("└─────────────────────────────────────────────────────────────┘");

        loop {
            if !overlay.poll_events() {
                break;
            }

            let now = Instant::now();
            if now.duration_since(last_frame) >= frame_duration {
                let elapsed = start.elapsed().as_secs_f32();

                // Create sample timer entries with staggered durations
                let entries = create_sample_timers(elapsed);
                overlay.set_data(TimerData { entries });
                overlay.render();
                last_frame = now;
            }

            let sleep_ms = if overlay.is_interactive() { 4 } else { 50 };
            std::thread::sleep(Duration::from_millis(sleep_ms));
        }
    }

    /// Create sample boss timers that tick down based on elapsed time
    fn create_sample_timers(elapsed: f32) -> Vec<TimerEntry> {
        // Define sample boss mechanics with their cycle times
        let mechanics = [
            ("Doom", 30.0, [200, 50, 50, 255]), // Red - big mechanic
            ("Lightning Storm", 20.0, [100, 150, 255, 255]), // Blue
            ("Adds Spawn", 45.0, [180, 100, 220, 255]), // Purple
            ("Enrage Check", 60.0, [255, 180, 50, 255]), // Orange
            ("Tank Swap", 15.0, [100, 220, 100, 255]), // Green
        ];

        mechanics
            .iter()
            .map(|(name, cycle, color)| {
                // Calculate remaining time in the current cycle
                let remaining = cycle - (elapsed % cycle);

                TimerEntry {
                    name: name.to_string(),
                    remaining_secs: remaining,
                    total_secs: *cycle,
                    color: *color,
                }
            })
            .collect()
    }

    /// Run the challenge overlay with 8 players and 3 challenges
    /// Demonstrates a typical raid challenge tracking scenario
    pub fn run_challenge_overlay() {
        let overlay_config = OverlayConfig {
            x: 200,
            y: 100,
            width: 340,
            height: 400,
            namespace: "baras-challenges".to_string(),
            click_through: false,
            target_monitor_id: None,
        };

        let challenge_config = ChallengeOverlayConfig {
            show_footer: true,
            show_duration: true,
            layout: ChallengeLayout::Vertical,
            ..Default::default()
        };

        let mut overlay = match ChallengeOverlay::new(overlay_config, challenge_config, 180) {
            Ok(o) => o,
            Err(e) => {
                eprintln!("Failed to create challenge overlay: {}", e);
                return;
            }
        };

        overlay.set_move_mode(true);

        let data = create_sample_challenges();
        overlay.set_data(data);

        let start = Instant::now();
        let mut last_frame = Instant::now();
        let frame_duration = Duration::from_millis(250); // 4 FPS like metric overlays

        println!("┌─────────────────────────────────────────────────────────────┐");
        println!("│       Challenge Overlay - 8 Players × 3 Challenges          │");
        println!("├─────────────────────────────────────────────────────────────┤");
        println!("│  Shows per-player contribution to boss mechanics            │");
        println!("│  Drag anywhere to move the overlay                          │");
        println!("│  Drag bottom-right corner to resize                         │");
        println!("├─────────────────────────────────────────────────────────────┤");
        println!("│  Challenges:                                                │");
        println!("│    • Doom Cleanse - who's handling cleanses                 │");
        println!("│    • Orb Catches  - crystal/orb mechanic participation      │");
        println!("│    • Add Damage   - contribution to priority targets        │");
        println!("├─────────────────────────────────────────────────────────────┤");
        println!("│  Press Ctrl+C to exit                                       │");
        println!("└─────────────────────────────────────────────────────────────┘");

        loop {
            if !overlay.poll_events() {
                break;
            }

            let now = Instant::now();
            if now.duration_since(last_frame) >= frame_duration {
                // Simulate dynamic updates: increment values over time
                let elapsed = start.elapsed().as_secs_f32();
                let mut data = create_sample_challenges();

                // Add some dynamic variation based on elapsed time
                for challenge in &mut data.entries {
                    for player in &mut challenge.by_player {
                        // Simulate ongoing contributions
                        let bonus = (elapsed * player.value as f32 * 0.01) as i64;
                        player.value += bonus;
                    }
                    // Recalculate total and percentages
                    let total: i64 = challenge.by_player.iter().map(|p| p.value).sum();
                    challenge.value = total;
                    for player in &mut challenge.by_player {
                        player.percent = if total > 0 {
                            (player.value as f32 / total as f32) * 100.0
                        } else {
                            0.0
                        };
                    }
                }
                data.duration_secs = elapsed;

                overlay.set_data(data);
                overlay.render();
                last_frame = now;
            }

            let sleep_ms = if overlay.frame().is_interactive() {
                4
            } else {
                50
            };
            std::thread::sleep(Duration::from_millis(sleep_ms));
        }
    }

    /// Create sample challenge data with 8 players and 3 challenges
    fn create_sample_challenges() -> ChallengeData {
        // 8 player raid composition: 2 tanks, 2 healers, 4 DPS
        // Player IDs: 1001-1002 tanks, 1003-1004 healers, 1005-1008 DPS

        // Challenge 1: Doom Cleanse (healers and tanks typically handle this) - Green
        let doom_cleanse = ChallengeEntry {
            name: "Doom Cleanse".to_string(),
            value: 24,
            event_count: 24,
            per_second: Some(0.4),
            duration_secs: 60.0,
            enabled: true,
            color: Some(Color::from_rgba8(80, 200, 120, 255)), // Green for cleanse
            columns: ChallengeColumns::TotalPercent,           // Show total and percent
            by_player: vec![
                PlayerContribution {
                    entity_id: 1003,
                    name: "Healz4Days".to_string(),
                    value: 8,
                    percent: 33.3,
                    per_second: Some(0.13),
                },
                PlayerContribution {
                    entity_id: 1004,
                    name: "HoTsOnYou".to_string(),
                    value: 7,
                    percent: 29.2,
                    per_second: Some(0.12),
                },
                PlayerContribution {
                    entity_id: 1001,
                    name: "Tanky McTank".to_string(),
                    value: 5,
                    percent: 20.8,
                    per_second: Some(0.08),
                },
                PlayerContribution {
                    entity_id: 1002,
                    name: "Shield Wall".to_string(),
                    value: 4,
                    percent: 16.7,
                    per_second: Some(0.07),
                },
                PlayerContribution {
                    entity_id: 1005,
                    name: "PewPewLazors".to_string(),
                    value: 0,
                    percent: 0.0,
                    per_second: Some(0.0),
                },
                PlayerContribution {
                    entity_id: 1006,
                    name: "StabbySith".to_string(),
                    value: 0,
                    percent: 0.0,
                    per_second: Some(0.0),
                },
                PlayerContribution {
                    entity_id: 1007,
                    name: "LightningLord".to_string(),
                    value: 0,
                    percent: 0.0,
                    per_second: Some(0.0),
                },
                PlayerContribution {
                    entity_id: 1008,
                    name: "ArsenalMerc".to_string(),
                    value: 0,
                    percent: 0.0,
                    per_second: Some(0.0),
                },
            ],
        };

        // Challenge 2: Orb Catches (everyone participates, ranged usually better) - Blue
        let orb_catches = ChallengeEntry {
            name: "Orb Catches".to_string(),
            value: 48,
            event_count: 48,
            per_second: Some(0.8),
            duration_secs: 60.0,
            enabled: true,
            color: Some(Color::from_rgba8(100, 150, 220, 255)), // Blue for orbs
            columns: ChallengeColumns::TotalPercent,            // Show total and percent
            by_player: vec![
                PlayerContribution {
                    entity_id: 1007,
                    name: "LightningLord".to_string(),
                    value: 12,
                    percent: 25.0,
                    per_second: Some(0.2),
                },
                PlayerContribution {
                    entity_id: 1008,
                    name: "ArsenalMerc".to_string(),
                    value: 10,
                    percent: 20.8,
                    per_second: Some(0.17),
                },
                PlayerContribution {
                    entity_id: 1005,
                    name: "PewPewLazors".to_string(),
                    value: 9,
                    percent: 18.8,
                    per_second: Some(0.15),
                },
                PlayerContribution {
                    entity_id: 1003,
                    name: "Healz4Days".to_string(),
                    value: 6,
                    percent: 12.5,
                    per_second: Some(0.1),
                },
                PlayerContribution {
                    entity_id: 1004,
                    name: "HoTsOnYou".to_string(),
                    value: 5,
                    percent: 10.4,
                    per_second: Some(0.08),
                },
                PlayerContribution {
                    entity_id: 1006,
                    name: "StabbySith".to_string(),
                    value: 4,
                    percent: 8.3,
                    per_second: Some(0.07),
                },
                PlayerContribution {
                    entity_id: 1001,
                    name: "Tanky McTank".to_string(),
                    value: 1,
                    percent: 2.1,
                    per_second: Some(0.02),
                },
                PlayerContribution {
                    entity_id: 1002,
                    name: "Shield Wall".to_string(),
                    value: 1,
                    percent: 2.1,
                    per_second: Some(0.02),
                },
            ],
        };

        // Challenge 3: Add Damage (DPS contribution to priority adds) - Red/Orange
        let add_damage = ChallengeEntry {
            name: "Add Damage".to_string(),
            value: 2_850_000,
            event_count: 1200,
            per_second: Some(47500.0),
            duration_secs: 60.0,
            enabled: true,
            color: Some(Color::from_rgba8(220, 100, 80, 255)), // Red/Orange for damage
            columns: ChallengeColumns::TotalPerSecond,         // Show total and DPS
            by_player: vec![
                PlayerContribution {
                    entity_id: 1005,
                    name: "PewPewLazors".to_string(),
                    value: 720_000,
                    percent: 25.3,
                    per_second: Some(12000.0),
                },
                PlayerContribution {
                    entity_id: 1007,
                    name: "LightningLord".to_string(),
                    value: 680_000,
                    percent: 23.9,
                    per_second: Some(11333.0),
                },
                PlayerContribution {
                    entity_id: 1006,
                    name: "StabbySith".to_string(),
                    value: 650_000,
                    percent: 22.8,
                    per_second: Some(10833.0),
                },
                PlayerContribution {
                    entity_id: 1008,
                    name: "ArsenalMerc".to_string(),
                    value: 580_000,
                    percent: 20.4,
                    per_second: Some(9667.0),
                },
                PlayerContribution {
                    entity_id: 1001,
                    name: "Tanky McTank".to_string(),
                    value: 95_000,
                    percent: 3.3,
                    per_second: Some(1583.0),
                },
                PlayerContribution {
                    entity_id: 1002,
                    name: "Shield Wall".to_string(),
                    value: 85_000,
                    percent: 3.0,
                    per_second: Some(1417.0),
                },
                PlayerContribution {
                    entity_id: 1003,
                    name: "Healz4Days".to_string(),
                    value: 25_000,
                    percent: 0.9,
                    per_second: Some(417.0),
                },
                PlayerContribution {
                    entity_id: 1004,
                    name: "HoTsOnYou".to_string(),
                    value: 15_000,
                    percent: 0.5,
                    per_second: Some(250.0),
                },
            ],
        };

        ChallengeData {
            entries: vec![doom_cleanse, orb_catches, add_damage],
            boss_name: Some("Dread Master Brontes".to_string()),
            duration_secs: 60.0,
            phase_durations: std::collections::HashMap::new(),
        }
    }

    /// Run the challenge overlay in horizontal layout with per-second columns
    pub fn run_challenge_overlay_horizontal() {
        let overlay_config = OverlayConfig {
            x: 100,
            y: 100,
            width: 900,
            height: 280,
            namespace: "baras-challenges-horiz".to_string(),
            click_through: false,
            target_monitor_id: None,
        };

        // Horizontal layout (per-challenge columns setting determines what's shown)
        let challenge_config = ChallengeOverlayConfig {
            show_footer: true,
            show_duration: true,
            layout: ChallengeLayout::Horizontal,
            ..Default::default()
        };

        let mut overlay = match ChallengeOverlay::new(overlay_config, challenge_config, 180) {
            Ok(o) => o,
            Err(e) => {
                eprintln!("Failed to create challenge overlay: {}", e);
                return;
            }
        };

        overlay.set_move_mode(true);

        let data = create_sample_challenges();
        overlay.set_data(data);

        let start = Instant::now();
        let mut last_frame = Instant::now();
        let frame_duration = Duration::from_millis(250);

        println!("┌─────────────────────────────────────────────────────────────┐");
        println!("│   Challenge Overlay - Horizontal Layout + Per-Second        │");
        println!("├─────────────────────────────────────────────────────────────┤");
        println!("│  3 challenges side-by-side with value/s columns             │");
        println!("│  Drag anywhere to move, corner to resize                    │");
        println!("├─────────────────────────────────────────────────────────────┤");
        println!("│  Press Ctrl+C to exit                                       │");
        println!("└─────────────────────────────────────────────────────────────┘");

        loop {
            if !overlay.poll_events() {
                break;
            }

            let now = Instant::now();
            if now.duration_since(last_frame) >= frame_duration {
                let elapsed = start.elapsed().as_secs_f32();
                let mut data = create_sample_challenges();

                for challenge in &mut data.entries {
                    for player in &mut challenge.by_player {
                        let bonus = (elapsed * player.value as f32 * 0.01) as i64;
                        player.value += bonus;
                    }
                    let total: i64 = challenge.by_player.iter().map(|p| p.value).sum();
                    challenge.value = total;
                    for player in &mut challenge.by_player {
                        player.percent = if total > 0 {
                            (player.value as f32 / total as f32) * 100.0
                        } else {
                            0.0
                        };
                    }
                    challenge.duration_secs = elapsed;
                }
                data.duration_secs = elapsed;

                overlay.set_data(data);
                overlay.render();
                last_frame = now;
            }

            let sleep_ms = if overlay.frame().is_interactive() {
                4
            } else {
                50
            };
            std::thread::sleep(Duration::from_millis(sleep_ms));
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();

    let overlay_type = args.get(1).map(|s| s.as_str()).unwrap_or("--metric");

    match overlay_type {
        "--raid" => examples::run_raid_overlay(),
        "--raid-timers" => examples::run_raid_timer_stress_test(),
        "--metric" => examples::run_metric_overlay(),
        "--metric-8" => examples::run_metric_overlay_8(),
        "--metric-16" => examples::run_metric_overlay_16(),
        "--timers" => examples::run_timer_overlay(),
        "--challenges" => examples::run_challenge_overlay(),
        "--challenges-h" => examples::run_challenge_overlay_horizontal(),
        _ => {
            println!("Usage: cargo run -p baras-overlay -- [OPTION]");
            println!();
            println!("Options:");
            println!("  --metric       DPS meter with 4 players (default)");
            println!("  --metric-8     Moveable DPS meter with 8 players");
            println!("  --metric-16    Moveable DPS meter with 16 players");
            println!("  --raid         Three raid overlays showing interaction modes");
            println!("  --raid-timers  16-frame stress test with ticking timers");
            println!("  --timers       Boss timer overlay with countdown bars");
            println!("  --challenges   Challenge overlay (vertical, 2-col: value + percent)");
            println!(
                "  --challenges-h Challenge overlay (horizontal, 3-col: value + /s + percent)"
            );
        }
    }
}
