# Requirements: BARAS v1.2 macOS Support

**Defined:** 2026-01-18
**Core Value:** Fast, reliable combat analysis that doesn't crash when something unexpected happens.

## v1.2 Requirements

Requirements for macOS platform support. Each maps to roadmap phases.

### macOS Overlay

- [ ] **MAC-01**: Fix CGContext compilation errors (type mismatches, missing API)
- [ ] **MAC-02**: Migrate from deprecated `cocoa` crate to `objc2-app-kit`/`objc2-foundation`
- [ ] **MAC-03**: Migrate custom NSView subclass from `ClassDecl` to `define_class!` macro
- [ ] **MAC-04**: Add `setReleasedWhenClosed(false)` for proper window memory management
- [ ] **MAC-05**: Use `Retained<T>` smart pointers for Objective-C object ownership
- [ ] **MAC-06**: Remove deprecated `cocoa` and `objc` crate dependencies after migration

## v1.3 Requirements

Deferred to future release.

### Navigation

- **NAV-01**: Live mode shows pulsing indicator with "LIVE" label
- **NAV-02**: Historical mode shows static indicator with "VIEWING: [encounter name]"
- **NAV-03**: Resume Live action returns to real-time data

### Editor

- **EDIT-05**: Drag-drop reordering for stats lists in overlay editor

## Out of Scope

Explicitly excluded.

| Feature | Reason |
|---------|--------|
| Mobile app | Desktop focus |
| objc2-core-graphics migration | core-graphics crate works, only need minor fixes |

## Traceability

Which phases cover which requirements. Updated during roadmap creation.

| Requirement | Phase | Status |
|-------------|-------|--------|
| MAC-01 | TBD | Pending |
| MAC-02 | TBD | Pending |
| MAC-03 | TBD | Pending |
| MAC-04 | TBD | Pending |
| MAC-05 | TBD | Pending |
| MAC-06 | TBD | Pending |

**Coverage:**
- v1.2 requirements: 6 total
- Mapped to phases: 0
- Unmapped: 6

---
*Requirements defined: 2026-01-18*
*Last updated: 2026-01-18 after initial definition*
