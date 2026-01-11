//! Example showing the new overlay types with mock data and icons
//!
//! Run with: cargo run -p baras-overlay --example new_overlays

use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use baras_overlay::icons::IconCache;
use baras_overlay::overlays::{
    CooldownConfig, CooldownData, CooldownEntry, CooldownOverlay, DotEntry, DotTarget,
    DotTrackerConfig, DotTrackerData, DotTrackerOverlay, Overlay, OverlayData, PersonalBuff,
    PersonalBuffsConfig, PersonalBuffsData, PersonalBuffsOverlay, PersonalDebuff,
    PersonalDebuffsConfig, PersonalDebuffsData, PersonalDebuffsOverlay,
};
use baras_overlay::platform::OverlayConfig;

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
        namespace: "personal_buffs_example".to_string(),
        click_through: true,
        target_monitor_id: None,
    };

    // Second buffs overlay with stack_priority mode
    let buffs_stack_config = OverlayConfig {
        x: 460,
        y: 100,
        width: 200,
        height: 80,
        namespace: "personal_buffs_stack_example".to_string(),
        click_through: true,
        target_monitor_id: None,
    };

    let debuffs_config = OverlayConfig {
        x: 100,
        y: 200,
        width: 350,
        height: 80,
        namespace: "personal_debuffs_example".to_string(),
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

    // Create overlays with show_effect_names enabled
    let mut buffs_cfg = PersonalBuffsConfig::default();
    buffs_cfg.show_effect_names = true;
    buffs_cfg.icon_size = 40;

    // Stack priority config - big centered stacks, timer secondary
    let mut buffs_stack_cfg = PersonalBuffsConfig::default();
    buffs_stack_cfg.show_effect_names = true;
    buffs_stack_cfg.icon_size = 48;
    buffs_stack_cfg.stack_priority = true;

    let mut debuffs_cfg = PersonalDebuffsConfig::default();
    debuffs_cfg.show_effect_names = true;
    debuffs_cfg.icon_size = 40;

    let mut cooldowns_cfg = CooldownConfig::default();
    cooldowns_cfg.show_ability_names = true;
    cooldowns_cfg.icon_size = 36;

    let mut dots_cfg = DotTrackerConfig::default();
    dots_cfg.show_effect_names = false; // Names in label, not on icons
    dots_cfg.icon_size = 24;

    let mut buffs_overlay = PersonalBuffsOverlay::new(buffs_config, buffs_cfg, 180)
        .expect("Failed to create buffs overlay");

    let mut buffs_stack_overlay = PersonalBuffsOverlay::new(buffs_stack_config, buffs_stack_cfg, 180)
        .expect("Failed to create stack priority buffs overlay");

    let mut debuffs_overlay = PersonalDebuffsOverlay::new(debuffs_config, debuffs_cfg, 180)
        .expect("Failed to create debuffs overlay");

    let mut cooldowns_overlay = CooldownOverlay::new(cooldowns_config, cooldowns_cfg, 180)
        .expect("Failed to create cooldowns overlay");

    let mut dots_overlay = DotTrackerOverlay::new(dots_config, dots_cfg, 180)
        .expect("Failed to create DOT tracker overlay");

    // Pre-load icons once (avoid allocations every frame)
    let icons = CachedIcons::load(icon_cache.as_ref());
    let start_time = Instant::now();

    // Test one overlay at a time - comment/uncomment to test each
    const TEST_BUFFS: bool = true;
    const TEST_BUFFS_STACK: bool = true;
    const TEST_DEBUFFS: bool = true;
    const TEST_COOLDOWNS: bool = true;
    const TEST_DOTS: bool = true;

    // Debug: skip rendering to test data update overhead
    const SKIP_RENDER: bool = false;

    loop {
        let elapsed = start_time.elapsed().as_secs_f32();

        // Update and render only enabled overlays
        if TEST_BUFFS {
            let buffs_data = create_mock_buffs(elapsed, &icons);
            buffs_overlay.update_data(OverlayData::PersonalBuffs(buffs_data));
            if !SKIP_RENDER {
                buffs_overlay.render();
            }
            if !buffs_overlay.poll_events() {
                break;
            }
        }

        // Stack priority buffs (Supercharge-style)
        if TEST_BUFFS_STACK {
            let stack_buffs_data = create_mock_stack_buffs(elapsed, &icons);
            buffs_stack_overlay.update_data(OverlayData::PersonalBuffs(stack_buffs_data));
            if !SKIP_RENDER {
                buffs_stack_overlay.render();
            }
            if !buffs_stack_overlay.poll_events() {
                break;
            }
        }

        if TEST_DEBUFFS {
            let debuffs_data = create_mock_debuffs(elapsed, &icons);
            debuffs_overlay.update_data(OverlayData::PersonalDebuffs(debuffs_data));
            if !SKIP_RENDER {
                debuffs_overlay.render();
            }
            if !debuffs_overlay.poll_events() {
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
    shock: IconData,
    stunned: IconData,
    orbital: IconData,
    heroic: IconData,
    maul: IconData,
    cleave: IconData,
    alacrity: IconData,
    shatter: IconData,
    ap_cell: IconData,
    supercharge: IconData,
    tactical_surge: IconData,
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
            shock: load(3362023089897472),
            stunned: load(3362023089897763),
            orbital: load(3221088033046528),
            heroic: load(3659741632921600),
            maul: load(2022405610405888),
            cleave: load(2748083284738048),
            alacrity: load(3417509772394496),
            shatter: load(1460787096846336),
            ap_cell: load(828301622902784),
            supercharge: load(801671729430528),  // Supercharged Celerity / Combat Support Cell
            tactical_surge: load(3244358165856256), // Reuse power surge icon
        }
    }
}

fn create_mock_buffs(elapsed: f32, icons: &CachedIcons) -> PersonalBuffsData {
    PersonalBuffsData {
        buffs: vec![
            PersonalBuff {
                effect_id: 3244358165856256,
                icon_ability_id: 3244358165856256,
                name: "Power Surge".to_string(),
                remaining_secs: 15.0 - (elapsed % 15.0),
                total_secs: 15.0,
                color: [80, 200, 220, 200],
                stacks: ((elapsed / 2.0) as u8 % 3) + 1,
                source_name: "Player".to_string(),
                target_name: "Player".to_string(),
                icon: icons.power_surge.clone(),
            },
            PersonalBuff {
                effect_id: 3648192465862656,
                icon_ability_id: 3648192465862656,
                name: "Focused Def".to_string(),
                remaining_secs: 10.0 - (elapsed % 10.0),
                total_secs: 10.0,
                color: [220, 180, 50, 200],
                stacks: 0,
                source_name: "Player".to_string(),
                target_name: "Player".to_string(),
                icon: icons.focused_def.clone(),
            },
            PersonalBuff {
                effect_id: 2877168526819328,
                icon_ability_id: 2877168526819328,
                name: "Enrage".to_string(),
                remaining_secs: 20.0 - (elapsed % 20.0),
                total_secs: 20.0,
                color: [200, 80, 80, 200],
                stacks: 0,
                source_name: "Player".to_string(),
                target_name: "Player".to_string(),
                icon: icons.enrage.clone(),
            },
            PersonalBuff {
                effect_id: 2882571595677696,
                icon_ability_id: 2882571595677696,
                name: "Shield".to_string(),
                remaining_secs: 12.0 - ((elapsed + 3.0) % 12.0),
                total_secs: 12.0,
                color: [80, 140, 220, 200],
                stacks: 0,
                source_name: "Player".to_string(),
                target_name: "Player".to_string(),
                icon: icons.shield.clone(),
            },
        ],
    }
}

/// Create mock buffs for stack_priority mode (Supercharge-style)
fn create_mock_stack_buffs(elapsed: f32, icons: &CachedIcons) -> PersonalBuffsData {
    // Simulate building stacks over time (cycles 1-10 every 10 seconds)
    let supercharge_stacks = ((elapsed % 10.0) as u8) + 1;
    let tactical_stacks = ((elapsed / 1.5) as u8 % 5) + 1;

    PersonalBuffsData {
        buffs: vec![
            PersonalBuff {
                effect_id: 801671729430528,
                icon_ability_id: 801671729430528,
                name: "Supercharge".to_string(),
                remaining_secs: 45.0 - (elapsed % 45.0), // Long duration, not the focus
                total_secs: 45.0,
                color: [80, 200, 220, 200],
                stacks: supercharge_stacks.min(10), // Building to 10
                source_name: "Player".to_string(),
                target_name: "Player".to_string(),
                icon: icons.supercharge.clone(),
            },
            PersonalBuff {
                effect_id: 3244358165856257,
                icon_ability_id: 3244358165856256,
                name: "Tact Surge".to_string(),
                remaining_secs: 30.0 - (elapsed % 30.0),
                total_secs: 30.0,
                color: [220, 180, 50, 200],
                stacks: tactical_stacks, // Building to 5
                source_name: "Player".to_string(),
                target_name: "Player".to_string(),
                icon: icons.tactical_surge.clone(),
            },
        ],
    }
}

fn create_mock_debuffs(elapsed: f32, icons: &CachedIcons) -> PersonalDebuffsData {
    PersonalDebuffsData {
        debuffs: vec![
            PersonalDebuff {
                effect_id: 1460787096846593,
                icon_ability_id: 1460787096846593,
                name: "Bleeding".to_string(),
                remaining_secs: 6.0 - (elapsed % 6.0),
                total_secs: 6.0,
                color: [255, 100, 50, 200],
                stacks: 3,
                is_cleansable: true,
                source_name: "Dread Master Styrak".to_string(),
                target_name: "Player".to_string(),
                icon: icons.bleeding.clone(),
            },
            PersonalDebuff {
                effect_id: 3362023089897472,
                icon_ability_id: 3362023089897472,
                name: "Shock".to_string(),
                remaining_secs: 8.0 - (elapsed % 8.0),
                total_secs: 8.0,
                color: [180, 80, 200, 200],
                stacks: 0,
                is_cleansable: true,
                source_name: "Dread Guard".to_string(),
                target_name: "Player".to_string(),
                icon: icons.shock.clone(),
            },
            PersonalDebuff {
                effect_id: 3362023089897763,
                icon_ability_id: 3362023089897763,
                name: "Stunned".to_string(),
                remaining_secs: 4.0 - (elapsed % 4.0),
                total_secs: 4.0,
                color: [100, 100, 100, 200],
                stacks: 0,
                is_cleansable: false,
                source_name: "Brontes".to_string(),
                target_name: "Player".to_string(),
                icon: icons.stunned.clone(),
            },
        ],
    }
}

fn create_mock_cooldowns(elapsed: f32, icons: &CachedIcons) -> CooldownData {
    CooldownData {
        entries: vec![
            CooldownEntry {
                ability_id: 3221088033046528,
                name: "Orbital Strike".to_string(),
                remaining_secs: (60.0 - (elapsed % 60.0)).max(0.0),
                total_secs: 60.0,
                icon_ability_id: 3221088033046528,
                charges: 1,
                max_charges: 1,
                color: [200, 100, 50, 200],
                source_name: "Player".to_string(),
                target_name: "".to_string(),
                icon: icons.orbital.clone(),
            },
            CooldownEntry {
                ability_id: 3659741632921600,
                name: "Heroic Moment".to_string(),
                remaining_secs: (300.0 - (elapsed % 300.0)).max(0.0),
                total_secs: 300.0,
                icon_ability_id: 3659741632921600,
                charges: 1,
                max_charges: 1,
                color: [220, 180, 50, 200],
                source_name: "Player".to_string(),
                target_name: "".to_string(),
                icon: icons.heroic.clone(),
            },
            CooldownEntry {
                ability_id: 2022405610405888,
                name: "Maul".to_string(),
                remaining_secs: (9.0 - (elapsed % 9.0)).max(0.0),
                total_secs: 9.0,
                icon_ability_id: 2022405610405888,
                charges: 2,
                max_charges: 2,
                color: [180, 80, 200, 200],
                source_name: "Player".to_string(),
                target_name: "".to_string(),
                icon: icons.maul.clone(),
            },
            CooldownEntry {
                ability_id: 2748083284738048,
                name: "Cleave".to_string(),
                remaining_secs: 0.0,
                total_secs: 6.0,
                icon_ability_id: 2748083284738048,
                charges: 1,
                max_charges: 1,
                color: [80, 200, 80, 200],
                source_name: "Player".to_string(),
                target_name: "".to_string(),
                icon: icons.cleave.clone(),
            },
            CooldownEntry {
                ability_id: 3417509772394496,
                name: "Alacrity".to_string(),
                remaining_secs: (120.0 - (elapsed % 120.0)).max(0.0),
                total_secs: 120.0,
                icon_ability_id: 3417509772394496,
                charges: 1,
                max_charges: 1,
                color: [80, 180, 220, 200],
                source_name: "Player".to_string(),
                target_name: "".to_string(),
                icon: icons.alacrity.clone(),
            },
        ],
    }
}

fn create_mock_dots(elapsed: f32, icons: &CachedIcons) -> DotTrackerData {
    DotTrackerData {
        targets: vec![
            DotTarget {
                entity_id: 100,
                name: "Dread Master Styrak".to_string(),
                dots: vec![
                    DotEntry {
                        effect_id: 1460787096846336,
                        icon_ability_id: 1460787096846336,
                        name: "Shatter".to_string(),
                        remaining_secs: 18.0 - (elapsed % 18.0),
                        total_secs: 18.0,
                        color: [180, 80, 200, 200],
                        stacks: 0,
                        source_name: "Player".to_string(),
                        target_name: "Dread Master Styrak".to_string(),
                        icon: icons.shatter.clone(),
                    },
                    DotEntry {
                        effect_id: 1460787096846593,
                        icon_ability_id: 1460787096846593,
                        name: "Bleed".to_string(),
                        remaining_secs: 18.0 - ((elapsed + 6.0) % 18.0),
                        total_secs: 18.0,
                        color: [200, 50, 50, 200],
                        stacks: 0,
                        source_name: "Player".to_string(),
                        target_name: "Dread Master Styrak".to_string(),
                        icon: icons.bleeding.clone(),
                    },
                ],
                last_updated: Instant::now(),
            },
            DotTarget {
                entity_id: 101,
                name: "Dread Guard".to_string(),
                dots: vec![DotEntry {
                    effect_id: 1460787096846336,
                    icon_ability_id: 1460787096846336,
                    name: "Shatter".to_string(),
                    remaining_secs: 18.0 - ((elapsed + 3.0) % 18.0),
                    total_secs: 18.0,
                    color: [180, 80, 200, 200],
                    stacks: 0,
                    source_name: "Player".to_string(),
                    target_name: "Dread Guard".to_string(),
                    icon: icons.shatter.clone(),
                }],
                last_updated: Instant::now(),
            },
            DotTarget {
                entity_id: 102,
                name: "Kell Dragon".to_string(),
                dots: vec![
                    DotEntry {
                        effect_id: 1460787096846593,
                        icon_ability_id: 1460787096846593,
                        name: "Bleed".to_string(),
                        remaining_secs: 18.0 - ((elapsed + 9.0) % 18.0),
                        total_secs: 18.0,
                        color: [200, 50, 50, 200],
                        stacks: 0,
                        source_name: "Player".to_string(),
                        target_name: "Kell Dragon".to_string(),
                        icon: icons.bleeding.clone(),
                    },
                    DotEntry {
                        effect_id: 828301622902784,
                        icon_ability_id: 828301622902784,
                        name: "AP Cell".to_string(),
                        remaining_secs: 6.0 - (elapsed % 6.0),
                        total_secs: 6.0,
                        color: [220, 220, 80, 200],
                        stacks: 5,
                        source_name: "Player".to_string(),
                        target_name: "Kell Dragon".to_string(),
                        icon: icons.ap_cell.clone(),
                    },
                ],
                last_updated: Instant::now(),
            },
        ],
    }
}
