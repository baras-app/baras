# Phase 11: Profile System Fixes - Research

**Researched:** 2026-01-18
**Domain:** Overlay state management, profile switching, Rust/Tauri architecture
**Confidence:** HIGH

## Summary

This research investigates the raid frames re-render bug (PROF-02) and profile selector placement (PROF-03). The visibility decoupling (PROF-01) has already been implemented and is out of scope.

The raid frames disappearing on profile switch and settings save is caused by a mismatch between the overlay recreation pattern used for raid overlays (to handle grid size changes) and the need to resend current frame data after recreation. The visibility toggle workaround works because `show_all` properly sends initial data to newly spawned overlays, but `refresh_settings` recreates the raid overlay without data resend.

**Primary recommendation:** After recreating the raid overlay in `refresh_settings`, explicitly send current raid frame data from the service's raid registry to the newly spawned overlay.

## Standard Stack

This is internal Rust/Tauri/Dioxus code - no external libraries needed.

### Core Components
| File | Purpose | Relevance |
|------|---------|-----------|
| `app/src-tauri/src/overlay/manager.rs` | Overlay lifecycle (spawn, shutdown, settings refresh) | Primary fix location for PROF-02 |
| `app/src-tauri/src/commands/service.rs` | Profile loading command | Calls `refresh_overlay_settings` after load |
| `app/src-tauri/src/router.rs` | Routes data updates to overlay threads | Shows how raid data flows |
| `overlay/src/overlays/raid.rs` | Raid overlay rendering | `update_data`, `update_config`, `needs_render` |
| `app/src/app.rs` | Frontend profile selector | Profile dropdown in header |

### Data Flow Architecture
```
Profile Load Flow:
1. Frontend: load_profile(name) -> refresh_overlay_settings()
2. Backend: load_profile updates config in memory + disk
3. Backend: refresh_overlay_settings recreates raid overlay
4. BUG: New raid overlay has empty frames (no data sent)

Visibility Toggle Flow (working):
1. hide_all() -> drain all overlays, shutdown
2. show_all() -> spawn enabled overlays + send_initial_data()
3. send_initial_data() for raid: sends current frames from registry
4. Result: Raid frames appear correctly
```

## Architecture Patterns

### Current Profile Switch Flow
```rust
// app/src-tauri/src/commands/service.rs - load_profile
config.load_profile(&name)?;           // Update config in memory
*handle.shared.config.write().await = config.clone();  // Propagate
config.save()?;                        // Persist to disk

// Reset move mode only (no data refresh)
for tx in txs {
    let _ = tx.send(OverlayCommand::SetMoveMode(false)).await;
}

// Frontend then calls refresh_overlay_settings
```

### Current refresh_settings (manager.rs) - Raid Special Case
```rust
// Lines 761-780: Raid overlay always recreates to handle grid size changes
let raid_enabled = settings.enabled.get("raid").copied().unwrap_or(false);
let raid_was_running = {
    if let Ok(mut s) = state.lock()
        && let Some(handle) = s.remove(OverlayType::Raid)
    {
        let _ = handle.tx.try_send(OverlayCommand::Shutdown);
        was_running = true;
    }
    was_running
};

// Respawn with new settings
if globally_visible && (raid_was_running || raid_enabled)
    && let Ok(result) = Self::spawn(OverlayType::Raid, settings)
{
    s.insert(result.handle);
}
// BUG: No data sent to newly spawned raid overlay!
```

### show_all Pattern (Working)
```rust
// Lines 504-508 in manager.rs
Self::send_initial_data(kind, &spawn_result.0, combat_data.as_ref()).await;
```

### Recommended Fix Pattern
```rust
// After raid respawn in refresh_settings:
if let OverlayType::Raid = kind {
    // Get current raid frame data from service
    if let Some(raid_data) = service.current_raid_data().await {
        let _ = tx.send(OverlayCommand::UpdateData(
            OverlayData::Raid(raid_data)
        )).await;
    }
}
```

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Raid data after recreate | Custom event system | Existing `send_initial_data` pattern | Already works for show_all |
| Frame state persistence | Manual state tracking | Service's raid registry | Already tracks current frames |
| Config update detection | Custom diff logic | Recreate pattern + data resend | Simple, already working for grid changes |

**Key insight:** The pattern for sending initial data exists and works in `show_all`. The bug is that `refresh_settings` doesn't use this pattern for raid overlays.

## Common Pitfalls

### Pitfall 1: Raid Data Source Location
**What goes wrong:** Assuming raid data is in combat_data cache
**Why it happens:** Metric/personal data comes from CombatData, raid frames come from RaidRegistry
**How to avoid:** Use `service.current_raid_frames()` or equivalent, not combat_data
**Warning signs:** Empty frames even though log is being tailed

### Pitfall 2: Async Lock Ordering
**What goes wrong:** Deadlock when holding overlay state lock while awaiting service calls
**Why it happens:** Both locks needed, but order matters
**How to avoid:** Get tx clone from overlay state, drop lock, then await service + send
**Warning signs:** Application hangs on profile switch

### Pitfall 3: UpdateConfig vs UpdateData Ordering
**What goes wrong:** Config arrives before data, overlay renders empty then doesn't re-render
**Why it happens:** Config update sets needs_render, data update checks for changes
**How to avoid:** Send data AFTER config update, or ensure data update always triggers render
**Warning signs:** First render shows old config with empty data

### Pitfall 4: Grid Layout Changes
**What goes wrong:** Grid size change doesn't take effect
**Why it happens:** RaidOverlay layout is set at construction time
**How to avoid:** Always recreate raid overlay on settings refresh (current pattern is correct)
**Warning signs:** Grid stays 2x4 when changed to 4x4

## Code Examples

### Current UpdateData Handler (raid.rs line 1145-1157)
```rust
impl Overlay for RaidOverlay {
    fn update_data(&mut self, data: OverlayData) -> bool {
        if let OverlayData::Raid(raid_data) = data {
            // Skip render if both old and new have no players with effects
            let old_has_effects = self.frames.iter().any(|f| !f.effects.is_empty());
            let new_has_effects = raid_data.frames.iter().any(|f| !f.effects.is_empty());
            let skip_render =
                !old_has_effects && !new_has_effects && self.frames.len() == raid_data.frames.len();
            self.set_frames(raid_data.frames);  // Always updates internal state
            !skip_render  // Returns whether render needed
        } else {
            false
        }
    }
}
```

### Current UpdateConfig Handler (raid.rs line 1159-1165)
```rust
fn update_config(&mut self, config: OverlayConfigUpdate) {
    if let OverlayConfigUpdate::Raid(raid_config, alpha) = config {
        self.config = raid_config;
        self.frame.set_background_alpha(alpha);
        self.needs_render = true;  // Always triggers render
    }
}
```

### send_initial_data Pattern (manager.rs line 169-180)
```rust
match kind {
    OverlayType::Raid
    | OverlayType::BossHealth
    | OverlayType::Timers
    | OverlayType::Challenges
    | OverlayType::Alerts
    | OverlayType::EffectsA
    | OverlayType::EffectsB
    | OverlayType::Cooldowns
    | OverlayType::DotTracker => {
        // These get data via separate update channels (bridge)
        // NOTE: This comment is misleading - the bridge only works for NEW data
        // Initial/current data must be sent explicitly
    }
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Send config only on refresh | Recreate raid overlay + send config | Unknown | Grid changes work, but data lost |

**Current gap:** Raid overlay recreation doesn't include data resend step.

## Open Questions

1. **Where is current raid frame data stored?**
   - **What we know:** RaidRegistry in service tracks slot assignments
   - **What's unclear:** Exact method to retrieve current frames as OverlayData::Raid
   - **Recommendation:** Add `current_raid_data()` method to ServiceHandle if not exists

2. **Should settings save also refresh raid data?**
   - **What we know:** Settings save calls same refresh_settings path
   - **What's unclear:** Whether user expectation differs for save vs profile switch
   - **Recommendation:** Same behavior - both should preserve current raid state

## PROF-03: Profile Selector Placement

### Current Implementation
Profile selector appears in two locations:
1. **Header bar** (line 473): Quick dropdown, only shows if profiles exist
2. **Overlay settings panel** (line 814): Within customization menu

### User Request
- Keep current placement in overlay settings
- Always visible even when no profiles exist
- Empty state: Show "Default" label + create button
- Selector only switches profiles - rename/delete handled in existing management area

### Implementation Approach
1. Modify overlay settings panel to always show profile section
2. Add empty state with "Default" label and "Save as Profile" button
3. Keep existing profile management (rename/delete) in current location
4. Selector dropdown shows existing profiles for switching

### Relevant Code Locations
| File | Lines | Purpose |
|------|-------|---------|
| `app/src/app.rs` | 813-860 | Current profile selector in overlay settings |
| `app/src/components/settings_panel.rs` | 196-215 | Profile management in settings |

## Sources

### Primary (HIGH confidence)
- Direct code analysis of:
  - `/home/prescott/baras/app/src-tauri/src/overlay/manager.rs` (refresh_settings logic)
  - `/home/prescott/baras/app/src-tauri/src/commands/service.rs` (load_profile flow)
  - `/home/prescott/baras/app/src-tauri/src/router.rs` (data routing)
  - `/home/prescott/baras/overlay/src/overlays/raid.rs` (raid overlay state)

### Secondary (MEDIUM confidence)
- `/home/prescott/baras/.planning/phases/11-profile-system-fixes/11-CONTEXT.md` (user decisions)

## Metadata

**Confidence breakdown:**
- Root cause analysis: HIGH - Code paths traced completely
- Fix approach: HIGH - Pattern exists in show_all, just needs reuse
- Profile selector: HIGH - Clear user requirements, existing code structure understood

**Research date:** 2026-01-18
**Valid until:** 60 days (stable internal code, unlikely to change significantly)
