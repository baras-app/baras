//! Example overlay application demonstrating the DPS metric
//!
//! This is a standalone example that creates a DPS metric overlay
//! with sample data. In production, this would receive data from
//! the core combat log parser.

use baras_core::context::OverlayAppearanceConfig;
use baras_overlay::{MeterEntry, MetricOverlay, OverlayConfig, colors};
use std::time::{Duration, Instant};

fn main() {
    // Configure the overlay
    let config = OverlayConfig {
        x: 500,
        y: 500,
        width: 280,
        height: 200,
        namespace: "baras-dps-metric".to_string(),
        click_through: false,
    };

    // Create the metric overlay with default appearance
    let appearance = OverlayAppearanceConfig::default();
    let mut metric = match MetricOverlay::new(config, "DPS Meter", appearance, 180) {
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

    metric.set_entries(entries);

    let start = Instant::now();
    let mut last_frame = Instant::now();
    let frame_duration = Duration::from_millis(16); // ~60 FPS

    println!("Overlay running. Press Ctrl+C to exit.");

    // Main loop
    loop {
        if !metric.poll_events() {
            break;
        }

        // Throttle to ~60 FPS
        let now = Instant::now();
        if now.duration_since(last_frame) >= frame_duration {
            // Update title with elapsed time (demo purposes)
            let elapsed = start.elapsed().as_secs();
            metric.set_title(&format!("DPS Meter - {}:{:02}", elapsed / 60, elapsed % 60));

            // Render the metric
            metric.render();

            last_frame = now;
        }

        // Small sleep to avoid busy-waiting
        std::thread::sleep(Duration::from_millis(1));
    }
}
