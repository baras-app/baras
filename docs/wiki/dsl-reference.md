# BARAS DSL Reference

Configuration reference for Boss Encounters and Effects definitions.

---

## Boss Encounter DSL

**Location:** `core/definitions/encounters/{type}/{area}.toml`

Files suffixed by `_custom.toml` contain user changes to packaged defaults.

### Root Structure

```toml
[area]
name = "Area Name"
area_id = 123456789           # SWTOR area ID
area_type = "operation"       # operation | flashpoint | lair_boss | training_dummy | other

[[boss]]
id = "boss_id"
name = "Boss Name"
difficulties = ["story", "veteran", "master"]
is_final_boss = true              # Auto-stop ops timer when this boss is killed
```

**`is_final_boss`** — Optional boolean (default `false`). When set to `true`, killing this
boss (encounter ends as a success) will automatically stop the operations timer. Use this
on the last boss of each operation.

### Entities

```toml
[[boss.entities]]
name = "Entity Name"          # Reference name for triggers
ids = [123, 456]              # NPC class IDs (all difficulty variants)
is_boss = true                # Track HP, show on health bar
is_kill_target = true         # Killing ends encounter
triggers_encounter = true     # Seeing NPC starts encounter
show_on_hp_overlay = true     # Show on boss HP overlay
pushes_at = 25.0              # Hide HP bar when entity HP drops to this %
```

**`pushes_at`** — Optional HP percentage threshold. When set, the entity's health bar
is removed from the Boss HP overlay once its HP drops to or below this value. Use this
for entities that get "pushed" out of combat (e.g., knocked off a platform) without
actually dying. The entity is not treated as dead — only its health bar is hidden.

### Phases

```toml
[[boss.phases]]
id = "phase_id"
name = "Phase Name"
trigger = { type = "..." }
end_trigger = { type = "..." }
preceded_by = "other_phase"           # Guard: only fires after this phase
counter_condition = { counter_id = "x", operator = "eq", value = 3 }
resets_counters = ["counter_id"]
```

### Counters

```toml
[[boss.counters]]
id = "counter_id"
name = "Counter Name"
increment_on = { type = "..." }
decrement_on = { type = "..." }
reset_on = { type = "..." }           # Default: combat_end
initial_value = 0
set_value = 5                         # Set instead of increment
```

### Timers

```toml
[[boss.timer]]
id = "timer_id"
name = "Timer Name"
trigger = { type = "..." }
duration_secs = 10.0                  # 0 = instant alert
is_alert = false
alert_text = "Custom alert"
color = [255, 100, 100, 255]          # RGBA
phases = ["phase_id"]                 # Only active in these phases
difficulties = ["master"]
enabled = true
can_be_refreshed = false
repeats = 0                           # Repeat count after initial
chains_to = "other_timer"             # Start on expiration
cancel_trigger = { type = "..." }
alert_at_secs = 5.0                   # Alert N seconds before expiration
show_on_raid_frames = false
show_at_secs = 0.0                    # Only show when remaining <= N (0 = always)

[boss.timer.audio]
enabled = true
file = "alert.wav"
offset = 0                            # Seconds before expiration
countdown_start = 5
countdown_voice = "Amy"
```

### Maps

The Replay tab draws a fight on the game's own map pages. Every operation's pages and the world area each one
covers are extracted from the game client into the [baras-maps](https://github.com/baras-app/baras-maps)
repository (`pages.toml` plus one image per page). The app downloads an operation's pages the first time one of
its fights is replayed, so a boss normally needs nothing here.

A boss can list its own maps instead, which then replace the area's pages for that boss:

```toml
[[boss.maps]]
image = "the_dread_fortress__map_brontes.webp"   # a page from baras-maps, or your own file under definitions/maps/
label = "brontes"         # optional: name on the map tabs
min_x = -86.0             # world area the whole image covers
max_x = 51.9
min_y = -857.8
max_y = -719.9
rotation = 0.0            # optional: degrees the image is turned
view = { min_x = -60.0, max_x = 30.0, min_y = -840.0, max_y = -750.0 }  # optional: open zoomed to this area
min_z = 3.0               # optional: floor, for stacked layouts
max_z = 62.0
phases = ["p2"]           # optional: limit the entry to these phases
```

`min_x` is the image's left edge and `min_y` its top edge; give a `min` above its `max` when the game axis runs
the other way. Entries may overlap and are listed smallest first: the replay shows the one holding most of the
raid and follows the raid when it moves. A `maps` list in a `_custom.toml` replaces the bundled list whole.

### Challenges

```toml
[[boss.challenge]]
id = "challenge_id"
name = "Challenge Name"
metric = "damage"                     # See metrics below
enabled = true
color = [255, 100, 100, 255]
columns = "total_percent"             # See column modes below

[[boss.challenge.conditions]]
type = "phase"
phase_ids = ["phase_id"]

[[boss.challenge.conditions]]
type = "source"
match = "boss"                        # EntityFilter

[[boss.challenge.conditions]]
type = "target"
match = "any_player"

[[boss.challenge.conditions]]
type = "ability"
ability_ids = [123456789]

[[boss.challenge.conditions]]
type = "effect"
effect_ids = [123456789]

[[boss.challenge.conditions]]
type = "counter"
counter_id = "x"
operator = "gte"
value = 3

[[boss.challenge.conditions]]
type = "boss_hp_range"
min_hp = 0.0
max_hp = 50.0
npc_id = 123456789                    # Optional: specific NPC
```

### Trigger Types

| Type               | Fields                                  |
| ------------------ | --------------------------------------- |
| `combat_start`     | —                                       |
| `combat_end`       | — (counter reset_on only)               |
| `ability_cast`     | `abilities`, `source?`, `position?`     |
| `effect_applied`   | `effects`, `source?`, `target?`, `position?` |
| `effect_removed`   | `effects`, `source?`, `target?`, `position?` |
| `damage_taken`     | `abilities`, `source?`, `target?`, `position?` |
| `boss_hp_below`    | `hp_percent`, `selector?`               |
| `boss_hp_above`    | `hp_percent`, `selector?` (phases only) |
| `npc_appears`      | `selector` (required), `position?`      |
| `entity_death`     | `selector?`, `position?`                |
| `target_set`       | `selector`, `target`                    |
| `phase_entered`    | `phase_id`                              |
| `phase_ended`      | `phase_id`                              |
| `any_phase_change` | — (counters only)                       |
| `counter_reaches`  | `counter_id`, `value`                   |
| `timer_expires`    | `timer_id`                              |
| `timer_started`    | `timer_id`                              |
| `timer_canceled`   | `timer_id`                              |
| `time_elapsed`     | `secs`                                  |
| `any_of`           | `conditions` (array of triggers)        |
| `manual`           | — (debug)                               |
| `never`            | — (disable reset)                       |

### Position Constraints

Event-driven triggers (`ability_cast`, `effect_applied`, `effect_removed`,
`damage_taken`, `damage_dealt`, `healing_taken`, `npc_appears`, `entity_death`)
accept an optional `position` array gating the trigger on the world coordinates
of the event's entities. All constraints must pass (AND); an entity without
coordinate data never matches. For `npc_appears` and `entity_death` the
constraints apply to the subject NPC itself (whether it appeared as the source
or target of the log event), so the `entity` field is effectively ignored.

```toml
# Only fire when the caster is inside the north-west quadrant
position = [
  { entity = "source", axis = "x", op = "between", min = 100.0, max = 150.0 },
  { entity = "source", axis = "y", op = "gte", value = -220.0 },
]
```

| Field    | Values                                              |
| -------- | --------------------------------------------------- |
| `entity` | `source` · `target`                                 |
| `axis`   | `x` · `y` · `z` · `facing`                          |
| `op`     | `gt` · `gte` · `lt` · `lte` · `between`             |
| values   | `value` for comparisons; `min` + `max` for `between` (inclusive) |

### Entity Filters

| Filter                        | Description           |
| ----------------------------- | --------------------- |
| `local_player`                | Local player          |
| `other_players`               | Other players         |
| `any_player`                  | Any player            |
| `any_companion`               | Any companion         |
| `any_player_or_companion`     | Players or companions |
| `group_members`               | Local player's group  |
| `group_members_except_local`  | Group excluding local |
| `boss`                        | Boss NPCs             |
| `npc_except_boss`             | Non-boss NPCs         |
| `any_npc`                     | Any NPC               |
| `any`                         | Any entity            |
| `{ selector = [id, "name"] }` | Specific entities     |

### Challenge Metrics

`damage` · `healing` · `effective_healing` · `damage_taken` · `healing_taken` · `ability_count` · `effect_count` · `deaths` · `threat`

### Challenge Columns

`total_percent` · `total_per_second` · `per_second_percent` · `total_only` · `per_second_only` · `percent_only`

### Conditions

State-based guards that gate when timers, phases, and victory triggers are active.
Conditions are implicitly AND'd — all must be true for the gated item to activate.

| Type                    | Fields                                      | Description                              |
| ----------------------- | ------------------------------------------- | ---------------------------------------- |
| `phase_active`          | `phase_ids` (array of strings)              | True when in any of the listed phases    |
| `counter_compare`       | `counter_id`, `operator`, `value` (u32)     | True when counter satisfies comparison   |
| `timer_time_remaining`  | `timer_id`, `operator`, `value` (f32 secs)  | True when timer remaining time matches   |
| `all_of`                | `conditions` (array of conditions)          | AND: all sub-conditions must be true     |
| `any_of`                | `conditions` (array of conditions)          | OR: any sub-condition must be true       |
| `not`                   | `condition` (single condition)              | Negation: true when inner is false       |

#### Timer Time Remaining

Checks remaining seconds on an active timer. Inactive timers are treated as 0.0 seconds.

```toml
# Only activate when enrage timer is running (any time remaining)
[[boss.timer.conditions]]
type = "timer_time_remaining"
timer_id = "enrage_timer"
operator = "gte"
value = 0.01

# Only activate when a timer has 10 seconds or less remaining
[[boss.timer.conditions]]
type = "timer_time_remaining"
timer_id = "some_mechanic"
operator = "lte"
value = 10.0
```

### Comparison Operators

`eq` · `ne` · `lt` · `lte` · `gt` · `gte`

---

## Effects DSL

**Location:** `~/.config/baras/effects/*.toml`
**Bundled:** `core/definitions/effects/`

### Structure

```toml
[[effect]]
# Identity
id = "effect_id"                      # Unique identifier
name = "Effect Name"
display_text = "Short"                # Optional override

# Matching
effects = [814832605462528, "Name"]   # Effect IDs or names
trigger = "effect_applied"            # effect_applied | effect_removed
refresh_abilities = [123, "Ability"]  # Abilities that refresh this effect
source = "local_player"               # EntityFilter
target = "group_members"              # EntityFilter

# Duration & Stacks
duration_secs = 21.0                  # None = indefinite
can_be_refreshed = true
max_stacks = 0                        # 0 = don't show stacks

# Display
category = "hot"                      # See categories below
color = [80, 200, 80, 255]            # Override RGBA (None = use category)
show_on_raid_frames = false
show_on_effects_overlay = false
show_at_secs = 0                      # Only show when remaining <= N

# Behavior
enabled = true
persist_past_death = false
track_outside_combat = true

# Alerts
alert_near_expiration = false
alert_threshold_secs = 3.0

# Timer Integration
on_apply_trigger_timer = "timer_id"
on_expire_trigger_timer = "timer_id"

# Context
encounters = ["Encounter Name"]       # Empty = all encounters

# Audio
[effect.audio]
enabled = false
file = "sound.wav"
offset = 0                            # Seconds before expiration
countdown_start = 0                   # 0 = disabled
countdown_voice = "Amy"
```

### Effect Categories

| Category     | Color  | Use Case           |
| ------------ | ------ | ------------------ |
| `hot`        | Green  | Heal-over-time     |
| `shield`     | Gold   | Absorb shields     |
| `buff`       | Blue   | Beneficial effects |
| `debuff`     | Red    | Harmful effects    |
| `cleansable` | Purple | Dispellable        |
| `proc`       | Cyan   | Temporary procs    |
| `mechanic`   | Orange | Boss mechanics     |

### Entity Filters

Same as Boss Encounter DSL (see above).

---

## Common Patterns

### Selector Format

IDs and names can be mixed in arrays:

```toml
abilities = [123456789, "Ability Name"]
selector = [123, "Entity Name"]
```

### Color Format

RGBA as 4-element array, values 0-255:

```toml
color = [255, 100, 100, 255]
```

### Audio Config

Shared structure for timers and effects:

```toml
[*.audio]
enabled = true
file = "path/to/sound.wav"
offset = 3                  # Seconds before event
countdown_start = 5         # Start countdown at N seconds
countdown_voice = "Amy"     # Voice pack
```
