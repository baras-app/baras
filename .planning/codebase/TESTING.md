# Testing Patterns

**Analysis Date:** 2026-01-17

## Test Framework

**Runner:**
- Rust's built-in test framework (`cargo test`)
- No external test runner configured

**Assertion Library:**
- Standard `assert!`, `assert_eq!`, `assert!(condition, "message")`

**Run Commands:**
```bash
cargo test                    # Run all tests
cargo test -p baras-core      # Run core crate tests only
cargo test -- --nocapture     # Show println output
cargo test test_name          # Run specific test
```

## Test File Organization

**Location:**
- Primary: Co-located in same file with `#[cfg(test)] mod tests;`
- Secondary: Separate `*_tests.rs` file in same directory (for large test suites)

**Naming:**
- Test modules: `mod tests` or `filename_tests.rs`
- Test functions: `test_<feature>_<scenario>`

**Structure:**
```
core/src/
├── combat_log/
│   ├── parser.rs           # Contains `mod tests;` at bottom
│   └── parser/
│       └── tests.rs        # Separate file for extensive tests
├── signal_processor/
│   ├── processor.rs
│   └── processor_tests.rs  # Separate test file
└── timers/
    ├── manager.rs
    └── manager_tests.rs    # Separate test file
```

## Test Structure

**Suite Organization:**
```rust
#[cfg(test)]
mod tests;  // External file reference

// OR inline:
#[cfg(test)]
mod tests {
    use super::*;

    // Helper functions at top
    fn test_parser() -> LogParser {
        let date = NaiveDateTime::parse_from_str("2024-01-01 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap();
        LogParser::new(date)
    }

    // Tests grouped by feature
    // parse_entity
    #[test]
    fn test_parse_entity_npc() { ... }

    #[test]
    fn test_parse_entity_player() { ... }
}
```

**Patterns:**
- Group related tests with comment headers
- Provide helper functions for test data setup
- Use `eprintln!` for debug output in tests

**Test Naming Convention:**
```rust
#[test]
fn test_<unit>_<scenario>() { ... }

// Examples:
fn test_parse_entity_npc()
fn test_parse_entity_player()
fn test_parse_details_damage_crit()
fn test_combat_start_triggers_timer()
fn test_timer_expires_triggers_chain()
```

## Mocking

**Framework:** None - uses real implementations with controlled inputs

**Patterns:**
- Create minimal test doubles inline
- Use fixture files for realistic test data
- Initialize test state directly without mocks

```rust
// Test helper creates minimal parser
fn test_parser() -> LogParser {
    let date = NaiveDateTime::parse_from_str("2024-01-01 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap();
    LogParser::new(date)
}

// Create test timer definitions inline
fn make_timer(id: &str, name: &str, trigger: TimerTrigger, duration: f32) -> TimerDefinition {
    TimerDefinition {
        id: id.to_string(),
        name: name.to_string(),
        trigger,
        duration_secs: duration,
        // ... defaults for other fields
    }
}
```

**What to Mock:**
- Nothing explicitly mocked; tests use real implementations
- Time is controlled via fixture timestamps, not mocked clocks

**What NOT to Mock:**
- File I/O for fixtures (use actual files)
- Parsing logic (test with real log lines)
- Signal processing (test full pipeline)

## Fixtures and Factories

**Test Data:**
```rust
// Inline fixture data for parser tests
let input = "Dread Master Bestia {3273941900591104}:5320000112163|(137.28,-120.98,-8.85,81.28)|(0/19129210)";
let result = parser.parse_entity(input);

// Load from file for integration tests
let fixture_path = Path::new("../test-log-files/fixtures/bestia_pull.txt");
if !fixture_path.exists() {
    eprintln!("Skipping test: fixture file not found at {:?}", fixture_path);
    return;
}
```

**Location:**
- `test-log-files/fixtures/` - Combat log snippets for signal tests
- `test-log-files/definitions/` - TOML boss definitions for integration tests
- `test-log-files/small/` - Small log files for quick tests
- `test-log-files/large/` - Full combat logs for performance testing

**Fixture Pattern:**
```rust
/// Parse a fixture file and collect all emitted signals
fn collect_signals_from_fixture(fixture_path: &Path) -> Vec<GameSignal> {
    let mut file = File::open(fixture_path).expect("Failed to open fixture file");
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).expect("Failed to read file");
    let content = String::from_utf8_lossy(&bytes);

    let parser = LogParser::new(chrono::Local::now().naive_local());
    let mut processor = EventProcessor::new();
    let mut cache = SessionCache::default();

    let mut all_signals = Vec::new();
    for (line_num, line) in content.lines().enumerate() {
        if let Some(event) = parser.parse_line(line_num as u64, line) {
            let (signals, _event) = processor.process_event(event, &mut cache);
            all_signals.extend(signals);
        }
    }
    all_signals
}
```

## Coverage

**Requirements:** None enforced

**View Coverage:**
```bash
# Using cargo-llvm-cov (if installed)
cargo llvm-cov --html
```

## Test Types

**Unit Tests:**
- Parser tests: Verify individual parsing functions with raw input strings
- Location: `core/src/combat_log/parser/tests.rs`
- Pattern: One test per edge case, isolated inputs
```rust
#[test]
fn test_parse_entity_npc() {
    let parser = test_parser();
    let input = "Dread Master Bestia {3273941900591104}:5320000112163|...";
    let result = parser.parse_entity(input);
    assert!(result.is_some());
    // Verify individual fields
}
```

**Integration Tests:**
- Signal processor tests: Parse full fixture files and verify signals
- Timer manager tests: Verify timer activation/chaining with signals
- Location: `core/src/signal_processor/processor_tests.rs`, `core/src/timers/manager_tests.rs`
```rust
#[test]
fn test_bestia_pull_emits_expected_signals() {
    let fixture_path = Path::new("../test-log-files/fixtures/bestia_pull.txt");
    if !fixture_path.exists() {
        eprintln!("Skipping test: fixture file not found");
        return;
    }
    let signals = collect_signals_from_fixture(fixture_path);
    // Verify signal types and counts
}
```

**E2E Tests:**
- Not present in codebase
- Manual testing via running application

## Common Patterns

**Async Testing:**
- Not heavily used; most tests are synchronous
- Async functions tested via `tokio::test` when needed

**Error Testing:**
```rust
#[test]
fn test_parse_entity_empty() {
    let parser = test_parser();
    let input = "";
    let result = parser.parse_entity(input);
    assert!(result.is_some());

    let entity = result.unwrap();
    assert_eq!(entity.entity_type, EntityType::Empty);
}
```

**Skip Tests Gracefully:**
```rust
#[test]
fn test_integration_with_fixture() {
    let fixture_path = Path::new("../test-log-files/fixtures/file.txt");
    if !fixture_path.exists() {
        eprintln!("Skipping test: fixture file not found at {:?}", fixture_path);
        return;  // Early return instead of panic
    }
    // Test logic
}
```

**Debug Output:**
```rust
#[test]
fn test_with_debug_output() {
    let signals = collect_signals();

    // Print for debugging during test development
    eprintln!("Collected {} signals of {} unique types:", signals.len(), signal_types.len());
    for signal_type in &signal_types {
        let count = signals.iter().filter(|s| signal_type_name(s) == *signal_type).count();
        eprintln!("  - {}: {}", signal_type, count);
    }

    // Assertions
    assert!(signal_types.contains("CombatStarted"), "Missing CombatStarted signal");
}
```

**Test State Reset:**
- Each test creates fresh instances of parsers, processors, and caches
- No shared mutable state between tests
```rust
#[test]
fn test_combat_start_triggers_timer() {
    let mut manager = TimerManager::new();  // Fresh instance
    // ... test logic
}
```

## Test Categories in Codebase

**Parser Tests** (`core/src/combat_log/parser/tests.rs`):
- Entity parsing (NPC, Player, Companion, SelfReference, Empty)
- Damage detail parsing (basic, crit, effective, absorbed, miss, shield, reflect)
- Heal detail parsing
- Charge parsing

**Signal Processor Tests** (`core/src/signal_processor/processor_tests.rs`):
- Combat lifecycle signals (CombatStarted, CombatEnded)
- Effect signals (EffectApplied with source info)
- Target signals (TargetChanged, TargetCleared)
- NPC signals (NpcFirstSeen for all NPC types)
- Entity lifecycle (EntityDeath, EntityRevived)
- Boss signals (BossEncounterDetected, BossHpChanged, PhaseChanged)
- Challenge tracking (boss damage, add damage, burn phase DPS)

**Timer Manager Tests** (`core/src/timers/manager_tests.rs`):
- Trigger types (CombatStart, AbilityCast, EffectApplied, NpcAppears, PhaseEnded)
- Timer chaining (TimerExpires triggers)
- Timer cancellation (cancel_trigger, CombatEnded clears)
- Timer refresh behavior
- AnyOf composite triggers
- Integration tests with real log fixtures

---

*Testing analysis: 2026-01-17*
