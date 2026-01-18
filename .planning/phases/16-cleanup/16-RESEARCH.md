# Phase 16: Cleanup - Research

**Researched:** 2026-01-18
**Domain:** Rust dependency management, crate removal
**Confidence:** HIGH

## Summary

Phase 16 is a straightforward dependency cleanup task following Phase 15's objc2 migration. The goal is to remove the deprecated `cocoa` and `objc` crates from `overlay/Cargo.toml` after Phase 15 has migrated all code to use the modern `objc2` ecosystem.

Key findings:
1. Both `cocoa` and `objc` crates are deprecated - `cocoa` recommends migration to objc2 ecosystem, `core-graphics-rs` from Servo is archived
2. The `core-graphics` crate is **independent** and does NOT depend on `objc` or `cocoa` - it can safely remain
3. Only `overlay/Cargo.toml` directly depends on `cocoa` and `objc` - no other crates in the workspace use them
4. CI already has macOS builds on `macos-15` runners that will verify the removal

**Primary recommendation:** After Phase 15 completes, remove `cocoa` and `objc` lines from Cargo.toml, keep `core-graphics`, verify with `cargo check --target aarch64-apple-darwin`.

## Standard Stack

After Phase 16, the macOS platform dependencies should be:

### Core (Retained)
| Library | Version | Purpose | Why Retained |
|---------|---------|---------|--------------|
| core-graphics | 0.24 | CGContext bitmap rendering | Independent of objc, provides CGContext API needed for drawing |
| objc2 | 0.6 | Modern Objective-C runtime bindings | Safe, maintained, replaces deprecated objc |
| objc2-foundation | 0.3 | Foundation framework (NSString, NSRect, etc.) | Modern Foundation bindings |
| objc2-app-kit | 0.3 | AppKit framework (NSWindow, NSView, etc.) | Modern AppKit bindings |

### Removed (Deprecated)
| Library | Version | Why Removed |
|---------|---------|-------------|
| cocoa | 0.26 | Deprecated, recommends objc2 ecosystem |
| objc | 0.2 | Deprecated, replaced by objc2 |

### Dependency Analysis

Current dependency tree shows:
```
cocoa v0.26.1
  -> cocoa-foundation v0.2.1
       -> objc v0.2.7
  -> objc v0.2.7
objc v0.2.7 (direct)
core-graphics v0.24.0 (independent - no objc dependency)
  -> core-foundation v0.10.1
  -> core-graphics-types v0.2.0
```

**Critical finding:** `core-graphics` does NOT depend on `objc` or `cocoa`. It only depends on:
- `core-foundation` (separate, maintained crate)
- `core-graphics-types`
- `foreign-types`
- `libc`

This means `core-graphics` can safely remain after removing `cocoa` and `objc`.

## Architecture Patterns

### Cargo.toml After Cleanup
```toml
# Platform: macOS
[target.'cfg(target_os = "macos")'.dependencies]
# CGContext operations (independent of objc ecosystem)
core-graphics = "0.24"

# Modern objc2 ecosystem (added in Phase 15)
objc2 = "0.6"
objc2-foundation = { version = "0.3", default-features = false, features = [
    "NSArray",
    "NSDate",
    "NSGeometry",
    "NSObject",
    "NSString",
] }
objc2-app-kit = { version = "0.3", default-features = false, features = [
    "NSApplication",
    "NSColor",
    "NSEvent",
    "NSGraphicsContext",
    "NSResponder",
    "NSRunningApplication",
    "NSScreen",
    "NSView",
    "NSWindow",
] }
```

### Removal Process
```bash
# Option 1: Manual edit (recommended for precision)
# Remove these two lines from overlay/Cargo.toml:
#   cocoa = "0.26"
#   objc = "0.2"

# Option 2: Using cargo remove
cd overlay && cargo remove cocoa objc
```

### Verification Steps
```bash
# 1. Check compilation for macOS target
cargo check -p baras-overlay --target aarch64-apple-darwin

# 2. Verify crates are gone from dependency tree
cargo tree -p baras-overlay --target aarch64-apple-darwin | grep -E "(cocoa|objc[^2])"
# Should return empty

# 3. Full build verification (if macOS available)
cargo build -p baras-overlay --target aarch64-apple-darwin --release
```

## Don't Hand-Roll

Problems that have existing solutions:

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Dependency removal verification | Manual grep of target/ | `cargo tree` with grep | Authoritative dependency analysis |
| Cross-compile checking | Build locally | `cargo check --target` | Faster, catches compilation errors |
| Unused dependency detection | Manual code review | `cargo-machete` or `cargo-udeps` | Automated, comprehensive |

## Common Pitfalls

### Pitfall 1: Premature Removal
**What goes wrong:** Removing cocoa/objc before Phase 15 completes all migrations
**Why it happens:** Eager cleanup without verifying all code paths migrated
**How to avoid:** Phase 16 must wait for Phase 15 completion and summary confirmation
**Warning signs:** Build errors mentioning `cocoa::` or old `objc::` imports

### Pitfall 2: Forgetting Transitive Dependencies
**What goes wrong:** Assuming removing direct deps removes all traces
**Why it happens:** Not understanding Cargo dependency resolution
**How to avoid:** Use `cargo tree` to verify complete removal
**Warning signs:** `cocoa-foundation` or similar crates still appearing in tree

### Pitfall 3: Breaking core-graphics
**What goes wrong:** Accidentally removing core-graphics thinking it depends on cocoa
**Why it happens:** Assumption based on crate naming/ecosystem
**How to avoid:** Verify dependency tree shows core-graphics is independent
**Warning signs:** CGContext-related compilation errors

### Pitfall 4: Stale Cargo.lock
**What goes wrong:** Old dependencies linger in Cargo.lock after removal
**Why it happens:** Cargo.lock preserves dependency versions
**How to avoid:** Run `cargo update` or regenerate Cargo.lock if needed
**Warning signs:** Removed crates still appearing in `cargo tree` output

### Pitfall 5: CI Target Mismatch
**What goes wrong:** Local check passes but CI fails
**Why it happens:** Different macOS target triple or SDK version
**How to avoid:** Match CI runner (macos-15, aarch64-apple-darwin) in verification
**Warning signs:** Platform-specific compilation errors in CI only

## Code Examples

### Verifying Dependency Removal
```bash
# Before removal - expect to see cocoa and objc
cargo tree -p baras-overlay --target aarch64-apple-darwin 2>&1 | grep -E "(cocoa|objc[^2])"

# After removal - should be empty
cargo tree -p baras-overlay --target aarch64-apple-darwin 2>&1 | grep -E "(cocoa|objc[^2])"
# (no output)

# Verify core-graphics still present
cargo tree -p baras-overlay --target aarch64-apple-darwin 2>&1 | grep "core-graphics"
# Should show: core-graphics v0.24.0
```

### Using cargo remove
```bash
# From workspace root
cargo remove cocoa objc --manifest-path overlay/Cargo.toml
```

### CI Verification Command
```bash
# This is what CI effectively runs
cargo build -p baras-overlay --target aarch64-apple-darwin --release
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| cocoa + objc crates | objc2 + objc2-app-kit | 2023-2024 | Memory safety, maintenance |
| Manual ClassDecl | define_class! macro | objc2 0.5+ | Type safety, ergonomics |
| Raw id pointers | Retained<T> smart pointers | objc2 0.5+ | Memory safety |
| core-graphics-rs (Servo) | Still usable OR objc2-core-graphics | Archived July 2023 | core-graphics still works |

**Note on core-graphics:** While `core-graphics-rs` from Servo is deprecated/archived, the `core-graphics` crate itself is still functional and does not require migration to `objc2-core-graphics` for basic CGContext operations. The project decision (from context) is to keep `core-graphics` rather than migrate.

## Open Questions

No significant open questions. The cleanup is straightforward once Phase 15 completes.

Minor considerations:
1. **Cargo.lock regeneration:** May need `cargo update` after removal to clean lock file - verify in practice
2. **CI timing:** Consider whether to run a manual CI workflow dispatch after cleanup to verify before merge

## Sources

### Primary (HIGH confidence)
- Local analysis: `cargo tree -p baras-overlay --target aarch64-apple-darwin` output
- `overlay/Cargo.toml` - Current dependency declarations
- `overlay/src/platform/macos.rs` - Usage of cocoa/objc/core-graphics
- `.github/workflows/build.yml` - CI configuration showing macOS build process

### Secondary (MEDIUM confidence)
- [cargo remove documentation](https://doc.rust-lang.org/cargo/commands/cargo-remove.html) - Official Cargo book
- [cocoa crate on crates.io](https://crates.io/crates/cocoa) - Deprecation notice
- [objc2 repository](https://github.com/madsmtm/objc2) - Replacement ecosystem
- [core-graphics-rs repository](https://github.com/servo/core-graphics-rs) - Archived status

### Tertiary (LOW confidence)
- [cargo-machete](https://github.com/bnjbvr/cargo-machete) - Tool for finding unused dependencies
- [cargo-udeps](https://github.com/est31/cargo-udeps) - Alternative unused dependency finder

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - Direct cargo tree analysis, no ambiguity
- Architecture: HIGH - Simple removal, verified dependencies
- Pitfalls: HIGH - Common patterns, well-documented

**Research date:** 2026-01-18
**Valid until:** Indefinite (dependency management patterns are stable)

**Phase 15 dependency:** This phase MUST wait for Phase 15 completion. The research assumes:
- 15-01: msg_send! migration complete
- 15-02: define_class! migration complete
- 15-03: Full window/app management migration complete (planned but not yet documented)

After Phase 15 completes, all `cocoa::` and old `objc::` imports should be removed from `macos.rs`, making the cleanup in Phase 16 a simple Cargo.toml edit.
