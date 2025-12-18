//! Example overlay application demonstrating the DPS meter
//!
//! This is a standalone example that creates a DPS meter overlay
//! with sample data. In production, this would receive data from
//! the core combat log parser.

use baras_overlay::{MeterEntry, MeterOverlay, OverlayConfig, colors};
use std::time::{Duration, Instant};

fn main() {
    // Configure the overlay
    let config = OverlayConfig {
        x: 500,
        y: 500,
        width: 280,
        height: 200,
        namespace: "baras-dps-meter".to_string(),
        click_through: false,
    };

    // Create the meter overlay
    let mut meter = match MeterOverlay::new(config, "DPS Meter") {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Failed to create overlay: {}", e);
            return;
        }
    };

    // Sample DPS entries (in production, these come from baras-core)
    let entries = vec![
        MeterEntry {
            name: "Player 1".to_string(),
            value: 12500,
            max_value: 15000,
            color: colors::dps_bar_fill(),
        },
        MeterEntry {
            name: "Player 2".to_string(),
            value: 10200,
            max_value: 15000,
            color: colors::dps_bar_fill(),
        },
        MeterEntry {
            name: "Player 3".to_string(),
            value: 8700,
            max_value: 15000,
            color: colors::hps_bar_fill(),
        },
        MeterEntry {
            name: "Player 4".to_string(),
            value: 6100,
            max_value: 15000,
            color: colors::tank_bar_fill(),
        },
    ];

    meter.set_entries(entries);

    let start = Instant::now();
    let mut last_frame = Instant::now();
    let frame_duration = Duration::from_millis(16); // ~60 FPS

    println!("Overlay running. Press Ctrl+C to exit.");

    // Main loop
    loop {
        if !meter.poll_events() {
            break;
        }

        // Throttle to ~60 FPS
        let now = Instant::now();
        if now.duration_since(last_frame) >= frame_duration {
            // Update title with elapsed time (demo purposes)
            let elapsed = start.elapsed().as_secs();
            meter.set_title(&format!("DPS Meter - {}:{:02}", elapsed / 60, elapsed % 60));

            // Render the meter
            meter.render();

            last_frame = now;
        }

        // Small sleep to avoid busy-waiting
        std::thread::sleep(Duration::from_millis(1));
    }
}
