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
```

### Entities

```toml
[[boss.entities]]
name = "Entity Name"          # Reference name for triggers
ids = [123, 456]              # NPC class IDs (all difficulty variants)
is_boss = true                # Track HP, show on health bar
is_kill_target = true         # Killing ends encounter
triggers_encounter = true     # Seeing NPC starts encounter
show_on_hp_overlay = true     # Show on boss HP overlay
```

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
| `ability_cast`     | `abilities`, `source?`                  |
| `effect_applied`   | `effects`, `source?`, `target?`         |
| `effect_removed`   | `effects`, `source?`, `target?`         |
| `damage_taken`     | `abilities`, `source?`, `target?`       |
| `boss_hp_below`    | `hp_percent`, `selector?`               |
| `boss_hp_above`    | `hp_percent`, `selector?` (phases only) |
| `npc_appears`      | `selector` (required)                   |
| `entity_death`     | `selector?`                             |
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
