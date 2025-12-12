# SWTOR Combat Log Format Reference

## Terminology

- **Event** - a state change logged by the game. Each log line represents one event.
- **Entity** - a player character or NPC who participates in combat
- **Combat Encounter** - a group of events demarcated by the `EnterCombat` and `ExitCombat` events where the primary player engages in combat with other entities
- **Action** - an output by an entity that changes the state of another entity.
- **Effect** - a state change
- **Segment** - an `[]` enclosed set of values within a log line, or the trailing data after the last bracket

## General Structure

Note: `%` delimited values denote optional elements

### Combat Events

```
[timestamp] [source_entity] [target_entity] [action] [effect] (details) <threat>
```

- `[]`, with optional `()` for damage/heal details and `<>` for threat.

- Timestamps are dateless in the format HH:MM:SS.mmm Example: `[15:18:38.253]`

### Meta Data

#### AreaEntered

```
[timestamp] [source] [] [] [AreaEntered {836045448953664}: AreaName {areaId} %difficulty {difficultyId}%] <version>

```

Example:

```
[18:28:08.183] [@Jerran Zeva#689501114780828|(-8.56,3.11,-0.98,358.89)|(426912/442951)] [] [] [AreaEntered {836045448953664}: The Dread Palace {137438993410} 8 Player Master {836045448953655}] (he3000) <v7.0.0b>
```

#### DisciplineChanged

```

[timestamp] [source] [] [] [DisciplineChanged {836045448953665}: Class {classId}/Discipline {disciplineId}]

```

```
[18:28:08.183] [@Jerran Zeva#689501114780828|(-8.56,3.11,-0.98,358.89)|(426912/442951)] [] [] [DisciplineChanged {836045448953665}: Commando {16141067504602942620}/Combat Medic {2031339142381637}]
```

## Entity Formats

### Player

[@PlayerName#PlayerId|(x,y,z,facing)|(health/maxHealth)]

Example:

```

[@Galen Ayder#690129185314118|(-4700.43,-4750.48,710.03,-0.71)|(1/414851)]

```

### Self-Reference

```

[=]

```

### NPC

```

[NpcName {npcId}:instanceId|(x,y,z,facing)|(health/maxHealth)]

```

Example:

```
[Dread Master Bestia {3273941900591104}:5320000112163|(137.28,-120.98,-8.85,81.28)|(0/19129210)]

```

### Companion

```

[@PlayerName#PlayerId/CompanionName {companionId}:instanceId|(coords)|(health)]

```

Example:

```
[@Jerran Zeva#689501114780828/Raina Temple {493328533553152}:87481369009487|(4749.87,4694.53,710.05,0.00)|(288866/288866)]
```

### Empty Target

```

[]

```

Used when there is no target (area effects, some buffs).

---

## Action/Ability Format

```

[AbilityName {abilityId}]

```

Example:

```

[Hold the Line {801303458480128}]

```

## Effect Format

```

[EffectType {effectTypeId}: EffectName {effectId}]

```

## Damage/Heal Segment Format

### Damage

```

(dmg%*% %~effectiveDmg% %-avoidtype% elementType {elementId} %(absorbedval abosrbed {absorbedId})%) <threat>

```

- `*` indicates critical hit
- `~` indicates effective value

### Heal Format

`(heal%*% %~effectiveheal%) %<threat>%`

- Only effective heals have a threat element

---

### Damage

#### Damage Dealt (full)

```
434:[13:33:01.056] [Dread Host Soldier {3266932513964032}:109455004221120|(-599.07,227.37,13.53,-212.85)|(7950/7950)] [@Jerran Zeva#689501114780828|(-605.11,236.17,10.96,0.49)|(45358/48062)] [Full Auto {811851898159104}] [ApplyEffect {836045448945477}: Damage {836045448945501}] (538 energy {836045448940874}) <538.0>
```

#### Damage Dealt (effective)

```
440:[13:33:01.268] [@Jerran Zeva#689501114780828|(-605.98,235.31,10.98,0.17)|(45358/48062)] [Dread Host Soldier {3266932513964032}:109455004221076|(-597.56,227.46,13.79,-169.16)|(0/7950)] [Boltstorm {3393522380046336}] [ApplyEffect {836045448945477}: Damage {836045448945501}] (4372* ~2641 energy {836045448940874}) <4372.0>
```

#### (shielded)

```
[13:33:03.378] [@Jerran Zeva#689501114780828|(-610.71,230.17,11.63,-75.74)|(44381/48062)] [Dread Host Soldier {3266932513964032}:109455004221164|(-599.67,227.36,13.49,104.36)|(1732/7950)] [Boltstorm {3393522380046336}] [ApplyEffect {836045448945477}: Damage {836045448945501}] (2583* energy {836045448940874} -shield {836045448945509} (1150 absorbed {836045448945511})) <2583.0>
```

#### avoided

```
[20:42:40.405] [@Squiikyy Cliin#690033109549043|(166.32,-228.92,-8.97,-176.96)|(443752/443752)] [Dread Master Calphayus {3273946195558400}:5320001530704|(164.79,-216.22,-7.96,-97.29)|(13147325/16878712)] [Corrosive Dart {3465720780292096}] [ApplyEffect {836045448945477}: Damage {836045448945501}] (0 -immune {836045448945506}) <8487.0>
```

- valid values: missed, dodge, parry, immune, resist, deflect

#### absorbed no shield

```
[20:40:49.575] [Dark Growth {3290812532129792}:5320001525667|(519.54,-176.95,-6.90,105.00)|(22504952/22504952)] [@Malenia#690112319693956|(464.56,-222.27,-8.97,-51.78)|(15592/433166)] [Inevitable Decay {3289425257693184}] [ApplyEffect {836045448945477}: Damage {836045448945501}] (21300 ~19807 internal {836045448940876} (1493 absorbed {836045448945511})) <21300.0>
```

#### reflected

```
[18:45:36.250] [@Jabba#689479364490083|(297.34,817.69,0.44,-8.80)|(503022/503022)] [Tunneling Tentacle {3025271884087296}:105466000024295|(287.97,799.82,0.15,-153.60)|(264
0035/4384811)] [Saber Reflect {3126177845739520}] [ApplyEffect {836045448945477}: Damage {836045448945501}] (116010 kinetic {836045448940873}(reflected {836045448953649}))
[18:45:36.250] [Tunneling Tentacle {3025271884087296}:105466000024295|(287.97,799.82,0.15,-153.60)|(2640035/4384811)] [@Jabba#689479364490083|(297.34,817.69,0.44,-8.80)|(5
03022/503022)] [Slam {3025877474476032}] [ApplyEffect {836045448945477}: Damage {836045448945501}] (0 -) <144394.0>
```

### Healing

#### Full

```
[22:18:23.116] [@Jerran Zeva#689501114780828|(-391.15,-955.35,-108.10,0.00)|(442951/442951)] [@Sh'dw#690124308215230|(-122.27,-32.95,-5.08,145.67)|(245836/442193)] [Preventative Medicine {2040994228863541}] [ApplyEffect {836045448945477}: Heal {836045448945500}] (11405*)
```

#### Effective

```
[22:07:13.261] [@Thalesa Sevaw#689497598898972|(-166.55,-220.72,-4.05,84.40)|(436908/436908)] [=] [Salvation {812990064492544}] [ApplyEffect {836045448945477}: Heal {836045448945500}] (2972* ~1882) <611.0>
```

#### No threat generated

```
[22:07:17.871] [@Thalesa Sevaw#689497598898972|(-166.55,-220.72,-4.05,105.09)|(436908/436908)] [=] [Salvation {812990064492544}] [ApplyEffect {836045448945477}: Heal {836045448945500}] (1754 ~0)
```

## Other Events

#### Enter Combat

```
[19:39:26.767] [@Jerran Zeva#689501114780828|(-129.34,-90.55,-8.82,142.94)|(442951/442951)] [] [] [Event {836045448945472}: EnterCombat {836045448945489}]
```

#### Exit Combat

```
[18:50:53.004] [@Jerran Zeva#689501114780828|(-26.15,-165.81,0.02,137.75)|(440098/442951)] [] [] [Event {836045448945472}: ExitCombat {836045448945490}]
```

#### Target Set

```
[20:42:50.344] [@Jerran Zeva#689501114780828|(466.61,-214.37,-8.97,-12.76)|(395069/442951)] [Dread Master Calphayus {3284954196738048}:5320001545076|(470.38,-217.20,-8.98,72.44)|(5470712/5626238)] [] [Event {836045448945472}: TargetSet {836045448953668}]
```

#### Target CLeared

```
[@Nir'eva#690546587511852|(-1003.82,368.82,-35.04,34.13)|(433048/433048)] [] [] [Event {836045448945472}: TargetCleared {836045448953669}]
```

#### Spend Energy (Merc/PT)

```
19:03:51.074] [@Jerran Zeva#689501114780828|(359.10,865.76,-169.10,-11.29)|(220057/442951)] [=] [] [Spend {836045448945473}: energy {836045448938503}] (7.0)
```

#### Buff Applied (self)

```
[15:18:38.959] [@Galen Ayder#690129185314118|(-4700.43,-4750.48,710.03,-0.71)|(414851/430286)] [=] [Fortification {4503204490379264}] [ApplyEffect {836045448945477}: Force Valor {4503204490379535}]
```

#### Buff Removed (self)

```
[15:18:56.417] [@Galen Ayder#690129185314118|(-4700.43,-4750.48,710.03,-0.71)|(430286/430286)] [=] [Safe Login {973870949466112}] [RemoveEffect {836045448945478}: Safe Login Immunity {973870949466372}]
```

#### Ability Activated

```
[15:20:52.554] [@Galen Ayder#690129185314118|(-4699.31,-4771.96,708.12,-0.23)|(430286/430286)] [=] [Hold the Line {801303458480128}] [Event {836045448945472}: AbilityActivate {836045448945479}]

```

#### ModifyCharges

```
[22:18:12.116] [@Jerran Zeva#689501114780828|(-142.80,-42.06,-5.17,-171.92)|(52045/442951)] [@Sh'dw#690124308215230|(-140.22,-33.76,-5.10,-164.40)|(274716/442193)] [Trauma Probe {999516199190528}] [ModifyCharges {836045448953666}: Trauma Probe {999516199190528}] (3 charges {836045448953667})
```

#### ApplyEffect w/ Charges

```
[@Jerran Zeva#689501114780828|(4700.32,4754.16,710.02,-0.94)|(442951/442951)] [=] [Supercharge {3408198283296768}] [ApplyEffect {836045448945477}: Supercharge {3408198283296768}] (1 charges {836045448953667})
```

## Notes & Edge Cases

- Timestamps can cross midnight (need to handle day rollover)
- Coordinates can be negative
- Some abilities have empty names: `[ {4813584596992000}]`
- Health values can be 0 for dead entities
