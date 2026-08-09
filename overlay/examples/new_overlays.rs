//! Example showing the overlay types with mock data and icons
//!
//! Run with: cargo run -p baras-overlay --example new_overlays

use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use baras_overlay::icons::IconCache;
use baras_overlay::overlays::{
    CooldownConfig, CooldownData, CooldownEntry, CooldownOverlay, DotEntry, DotTarget,
    DotTrackerConfig, DotTrackerData, DotTrackerOverlay, EffectABEntry, EffectsABConfig,
    EffectsABData, EffectsABOverlay, EffectsLayout, MapConfig, MapData, MapOverlay, Overlay,
    OverlayData,
};
use baras_overlay::platform::OverlayConfig;

/// Mock map SVG for the map overlay example (a bordered box with an ellipse and
/// a couple of clock-style markers).
const MAP_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="200" viewBox="0 0 200 200">
  <rect x="1.5" y="1.5" width="197" height="197" fill="none" stroke="#ffffff" stroke-width="2"/>
  <ellipse cx="100" cy="100" rx="55" ry="80" fill="#e63946" fill-opacity="0.25" stroke="#e63946" stroke-width="2"/>
  <circle cx="30" cy="100" r="13" fill="#e63946" fill-opacity="0.25"/>
  <text x="30" y="105" text-anchor="middle" font-family="sans-serif" font-size="16" font-weight="bold" fill="#ffffff">12</text>
  <circle cx="100" cy="16" r="13" fill="#e63946" fill-opacity="0.25"/>
  <text x="100" y="21" text-anchor="middle" font-family="sans-serif" font-size="16" font-weight="bold" fill="#ffffff">3</text>
  <circle cx="170" cy="100" r="13" fill="#e63946" fill-opacity="0.25"/>
  <text x="170" y="105" text-anchor="middle" font-family="sans-serif" font-size="16" font-weight="bold" fill="#ffffff">6</text>
  <circle cx="100" cy="184" r="13" fill="#e63946" fill-opacity="0.25"/>
  <text x="100" y="189" text-anchor="middle" font-family="sans-serif" font-size="16" font-weight="bold" fill="#ffffff">9</text>
</svg>"##;

fn main() {
    println!("Starting new overlays example...");
    println!("Press Ctrl+C to exit\n");

    // Load icon cache
    let icon_cache = IconCache::new(
        Path::new("icons/icons.csv"),
        Path::new("icons/icons.zip"),
        200,
    );

    let icon_cache = match icon_cache {
        Ok(cache) => {
            println!("Loaded icon cache successfully");
            Some(Arc::new(cache))
        }
        Err(e) => {
            println!("Warning: Could not load icons: {}", e);
            println!("Running without icons (colored squares only)\n");
            None
        }
    };

    // Create overlay configs with different positions
    // Use click_through: true for normal rendering (not move mode)
    let buffs_config = OverlayConfig {
        x: 100,
        y: 100,
        width: 350,
        height: 80,
        namespace: "effects_a_example".to_string(),
        click_through: true,
        target_monitor_id: None,
    };

    // Effects in bar layout (timer-style stacked bars)
    let effects_bar_config = OverlayConfig {
        x: 460,
        y: 100,
        width: 300,
        height: 200,
        namespace: "effects_bar_example".to_string(),
        click_through: true,
        target_monitor_id: None,
    };

    let cooldowns_config = OverlayConfig {
        x: 100,
        y: 300,
        width: 220,
        height: 320,
        namespace: "cooldowns_example".to_string(),
        click_through: true,
        target_monitor_id: None,
    };

    let dots_config = OverlayConfig {
        x: 340,
        y: 300,
        width: 300,
        height: 200,
        namespace: "dot_tracker_example".to_string(),
        click_through: true,
        target_monitor_id: None,
    };

    // DOT tracker in bar mode (grouped bars under target names)
    let dots_bar_config = OverlayConfig {
        x: 660,
        y: 300,
        width: 320,
        height: 400,
        namespace: "dot_tracker_bar_example".to_string(),
        click_through: true,
        target_monitor_id: None,
    };

    // Map overlay (renders a static SVG stretched to fill the window)
    let map_config = OverlayConfig {
        x: 1000,
        y: 100,
        width: 240,
        height: 240,
        namespace: "map_example".to_string(),
        click_through: true,
        target_monitor_id: None,
    };

    // Create overlays with show_effect_names enabled
    let mut buffs_cfg = EffectsABConfig::default();
    buffs_cfg.show_effect_names = true;
    buffs_cfg.icon_size = 40;

    let mut effects_bar_cfg = EffectsABConfig::default();
    effects_bar_cfg.layout = EffectsLayout::Bar;
    effects_bar_cfg.show_effect_names = true;
    effects_bar_cfg.bar_gradient = true;

    let mut cooldowns_cfg = CooldownConfig::default();
    cooldowns_cfg.show_ability_names = true;
    cooldowns_cfg.icon_size = 36;

    let mut dots_cfg = DotTrackerConfig::default();
    dots_cfg.icon_size = 24;

    let mut dots_bar_cfg = DotTrackerConfig::default();
    dots_bar_cfg.layout_bar = true;
    dots_bar_cfg.bar_gradient = true;

    let mut buffs_overlay = EffectsABOverlay::new(buffs_config, buffs_cfg, 180, "Effects A")
        .expect("Failed to create effects overlay");

    let mut effects_bar_overlay =
        EffectsABOverlay::new(effects_bar_config, effects_bar_cfg, 180, "Effects Bar")
            .expect("Failed to create bar effects overlay");

    let mut cooldowns_overlay = CooldownOverlay::new(cooldowns_config, cooldowns_cfg, 180)
        .expect("Failed to create cooldowns overlay");

    let mut dots_overlay = DotTrackerOverlay::new(dots_config, dots_cfg, 180)
        .expect("Failed to create DOT tracker overlay");

    let mut dots_bar_overlay = DotTrackerOverlay::new(dots_bar_config, dots_bar_cfg, 180)
        .expect("Failed to create bar mode DOT tracker overlay");

    let mut map_overlay = MapOverlay::new(map_config, MapConfig::default(), 180)
        .expect("Failed to create map overlay");
    // The map SVG is static, so build it once and reuse it each frame.
    let map_svg = Arc::new(MAP_SVG.to_string());

    // Pre-load icons once (avoid allocations every frame)
    let icons = CachedIcons::load(icon_cache.as_ref());
    let start_time = Instant::now();

    // Test one overlay at a time - comment/uncomment to test each
    const TEST_BUFFS: bool = true;
    const TEST_EFFECTS_BAR: bool = true;
    const TEST_COOLDOWNS: bool = true;
    const TEST_DOTS: bool = true;
    const TEST_DOTS_BAR: bool = true;
    const TEST_MAP: bool = true;

    // Debug: skip rendering to test data update overhead
    const SKIP_RENDER: bool = false;

    loop {
        let elapsed = start_time.elapsed().as_secs_f32();

        // Update and render only enabled overlays
        if TEST_BUFFS {
            let buffs_data = create_mock_effects(elapsed, &icons);
            buffs_overlay.update_data(OverlayData::EffectsA(buffs_data));
            if !SKIP_RENDER {
                buffs_overlay.render();
            }
            if !buffs_overlay.poll_events() {
                break;
            }
        }

        if TEST_EFFECTS_BAR {
            let effects_data = create_mock_effects(elapsed, &icons);
            effects_bar_overlay.update_data(OverlayData::EffectsB(effects_data));
            if !SKIP_RENDER {
                effects_bar_overlay.render();
            }
            if !effects_bar_overlay.poll_events() {
                break;
            }
        }

        if TEST_COOLDOWNS {
            let cooldowns_data = create_mock_cooldowns(elapsed, &icons);
            cooldowns_overlay.update_data(OverlayData::Cooldowns(cooldowns_data));
            if !SKIP_RENDER {
                cooldowns_overlay.render();
            }
            if !cooldowns_overlay.poll_events() {
                break;
            }
        }

        if TEST_DOTS {
            let dots_data = create_mock_dots(elapsed, &icons);
            dots_overlay.update_data(OverlayData::DotTracker(dots_data));
            if !SKIP_RENDER {
                dots_overlay.render();
            }
            if !dots_overlay.poll_events() {
                break;
            }
        }

        if TEST_DOTS_BAR {
            let dots_data = create_mock_dots(elapsed, &icons);
            dots_bar_overlay.update_data(OverlayData::DotTracker(dots_data));
            if !SKIP_RENDER {
                dots_bar_overlay.render();
            }
            if !dots_bar_overlay.poll_events() {
                break;
            }
        }

        if TEST_MAP {
            map_overlay.update_data(OverlayData::Map(MapData {
                svg: Some(map_svg.clone()),
                placeholder: None,
            }));
            if !SKIP_RENDER {
                map_overlay.render();
            }
            if !map_overlay.poll_events() {
                break;
            }
        }

        // 100ms = 10 FPS
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    println!("Example finished.");
}

/// Icon data wrapped in Arc for zero-copy cloning
type IconData = Option<Arc<(u32, u32, Vec<u8>)>>;

/// Pre-loaded icon data (cached to avoid allocations every frame)
struct CachedIcons {
    power_surge: IconData,
    focused_def: IconData,
    enrage: IconData,
    shield: IconData,
    bleeding: IconData,
    orbital: IconData,
    heroic: IconData,
    maul: IconData,
    cleave: IconData,
    alacrity: IconData,
    shatter: IconData,
    ap_cell: IconData,
}

impl CachedIcons {
    fn load(cache: Option<&Arc<IconCache>>) -> Self {
        let load = |id: u64| -> IconData {
            cache.and_then(|c| {
                c.get_icon(id)
                    .map(|data| Arc::new((data.width, data.height, data.rgba)))
            })
        };

        Self {
            power_surge: load(3244358165856256),
            focused_def: load(3648192465862656),
            enrage: load(2877168526819328),
            shield: load(2882571595677696),
            bleeding: load(1460787096846593),
            orbital: load(3221088033046528),
            heroic: load(3659741632921600),
            maul: load(2022405610405888),
            cleave: load(2748083284738048),
            alacrity: load(3417509772394496),
            shatter: load(1460787096846336),
            ap_cell: load(828301622902784),
        }
    }
}

fn make_effect(
    effect_id: u64,
    name: &str,
    remaining_secs: f32,
    total_secs: f32,
    color: [u8; 4],
    stacks: u8,
    icon: &IconData,
) -> EffectABEntry {
    EffectABEntry {
        effect_id,
        icon_ability_id: effect_id,
        name: name.to_string(),
        display_text: String::new(),
        remaining_secs,
        total_secs,
        color,
        stacks,
        source_name: "Player".to_string(),
        target_name: "Player".to_string(),
        icon: icon.clone(),
        show_icon: true,
        display_source: false,
        max_total_secs: None,
        max_remaining_secs: None,
    }
}

fn create_mock_effects(elapsed: f32, icons: &CachedIcons) -> EffectsABData {
    EffectsABData {
        effects: vec![
            make_effect(
                3244358165856256,
                "Power Surge",
                15.0 - (elapsed % 15.0),
                15.0,
                [80, 200, 220, 200],
                ((elapsed / 2.0) as u8 % 3) + 1,
                &icons.power_surge,
            ),
            make_effect(
                3648192465862656,
                "Focused Def",
                10.0 - (elapsed % 10.0),
                10.0,
                [220, 180, 50, 200],
                0,
                &icons.focused_def,
            ),
            make_effect(
                2877168526819328,
                "Enrage",
                20.0 - (elapsed % 20.0),
                20.0,
                [200, 80, 80, 200],
                0,
                &icons.enrage,
            ),
            make_effect(
                2882571595677696,
                "Shield",
                12.0 - ((elapsed + 3.0) % 12.0),
                12.0,
                [80, 140, 220, 200],
                0,
                &icons.shield,
            ),
        ],
    }
}

fn create_mock_cooldowns(elapsed: f32, icons: &CachedIcons) -> CooldownData {
    let make = |ability_id: u64,
                name: &str,
                remaining_secs: f32,
                total_secs: f32,
                charges: u8,
                max_charges: u8,
                color: [u8; 4],
                icon: &IconData| CooldownEntry {
        ability_id,
        name: name.to_string(),
        remaining_secs,
        total_secs,
        icon_ability_id: ability_id,
        charges,
        max_charges,
        color,
        source_name: "Player".to_string(),
        target_name: String::new(),
        icon: icon.clone(),
        show_icon: true,
        display_source: false,
        is_in_ready_state: remaining_secs <= 0.0,
    };

    CooldownData {
        entries: vec![
            make(
                3221088033046528,
                "Orbital Strike",
                (60.0 - (elapsed % 60.0)).max(0.0),
                60.0,
                1,
                1,
                [200, 100, 50, 200],
                &icons.orbital,
            ),
            make(
                3659741632921600,
                "Heroic Moment",
                (300.0 - (elapsed % 300.0)).max(0.0),
                300.0,
                1,
                1,
                [220, 180, 50, 200],
                &icons.heroic,
            ),
            make(
                2022405610405888,
                "Maul",
                (9.0 - (elapsed % 9.0)).max(0.0),
                9.0,
                2,
                2,
                [180, 80, 200, 200],
                &icons.maul,
            ),
            make(
                2748083284738048,
                "Cleave",
                0.0,
                6.0,
                1,
                1,
                [80, 200, 80, 200],
                &icons.cleave,
            ),
            make(
                3417509772394496,
                "Alacrity",
                (120.0 - (elapsed % 120.0)).max(0.0),
                120.0,
                1,
                1,
                [80, 180, 220, 200],
                &icons.alacrity,
            ),
        ],
    }
}

fn make_dot(
    effect_id: u64,
    name: &str,
    remaining_secs: f32,
    total_secs: f32,
    color: [u8; 4],
    target_name: &str,
    icon: &IconData,
) -> DotEntry {
    DotEntry {
        effect_id,
        icon_ability_id: effect_id,
        name: name.to_string(),
        remaining_secs,
        total_secs,
        color,
        source_name: "Player".to_string(),
        target_name: target_name.to_string(),
        icon: icon.clone(),
        show_icon: true,
    }
}

fn create_mock_dots(elapsed: f32, icons: &CachedIcons) -> DotTrackerData {
    DotTrackerData {
        targets: vec![
            DotTarget {
                entity_id: 100,
                name: "Dread Master Styrak".to_string(),
                dots: vec![
                    make_dot(
                        1460787096846336,
                        "Shatter",
                        18.0 - (elapsed % 18.0),
                        18.0,
                        [180, 80, 200, 200],
                        "Dread Master Styrak",
                        &icons.shatter,
                    ),
                    make_dot(
                        1460787096846593,
                        "Bleed",
                        18.0 - ((elapsed + 6.0) % 18.0),
                        18.0,
                        [200, 50, 50, 200],
                        "Dread Master Styrak",
                        &icons.bleeding,
                    ),
                ],
            },
            DotTarget {
                entity_id: 101,
                name: "Dread Guard".to_string(),
                dots: vec![make_dot(
                    1460787096846336,
                    "Shatter",
                    18.0 - ((elapsed + 3.0) % 18.0),
                    18.0,
                    [180, 80, 200, 200],
                    "Dread Guard",
                    &icons.shatter,
                )],
            },
            DotTarget {
                entity_id: 102,
                name: "Kell Dragon".to_string(),
                dots: vec![
                    make_dot(
                        1460787096846593,
                        "Bleed",
                        18.0 - ((elapsed + 9.0) % 18.0),
                        18.0,
                        [200, 50, 50, 200],
                        "Kell Dragon",
                        &icons.bleeding,
                    ),
                    make_dot(
                        828301622902784,
                        "AP Cell",
                        6.0 - (elapsed % 6.0),
                        6.0,
                        [220, 220, 80, 200],
                        "Kell Dragon",
                        &icons.ap_cell,
                    ),
                ],
            },
        ],
    }
}
