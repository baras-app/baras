# Requirements: BARAS

**Defined:** 2026-01-18
**Core Value:** Fast, reliable combat analysis that doesn't crash when something unexpected happens.

## v1.1 Requirements

Requirements for UX Polish milestone. Eliminate confusion and friction in the UI.

### Empty States

- [x] **EMPTY-01**: User sees helpful "Waiting for combat data..." message instead of "Unknown Session"
- [x] **EMPTY-02**: Overlays display last encounter data on startup instead of appearing blank

### Navigation & Status

- [ ] **NAV-01**: Live mode shows green arrow indicator in central top bar position
- [ ] **NAV-02**: Historical mode shows pause/stop icon with history indicator
- [ ] **NAV-03**: "Resume Live" button prominently visible when in historical mode
- [x] **NAV-04**: Parsely upload button on session page (not buried in file explorer)

### Profile System

- [x] **PROF-01**: Profile switching does not toggle overlay visibility
- [x] **PROF-02**: Raid frames re-render correctly after profile switch
- [x] **PROF-03**: Profile selector more visible in overlay settings (not collapsed by default)

### Overlay System

- [x] **OVLY-01**: Move mode always resets to false on application startup
- [x] **OVLY-02**: Save button in fixed header position (visible without scrolling)
- [x] **OVLY-03**: Customization changes preview live before saving
- [x] **OVLY-04**: Overlay buttons have brief descriptions of what they do
- [ ] **OVLY-05**: REARRANGE FRAMES and CLEAR FRAMES grouped with Raid Frames section (descoped)
- [x] **OVLY-06**: CUSTOMIZE button clearly indicates it opens settings

### Effects & Encounter Builder

- [x] **EDIT-01**: Form fields have tooltips explaining their purpose (Display Target, Trigger, etc.)
- [x] **EDIT-02**: Better visual grouping/hierarchy separating form sections
- [x] **EDIT-03**: Empty state shows guidance for creating first entry
- [x] **EDIT-04**: New effects/timers appear at top of list (not bottom)
- [ ] **EDIT-05**: Stats reordering uses drag-and-drop instead of up/down buttons (descoped)
- [x] **EDIT-06**: Raw file path removed from Encounter Builder

### Session Display

- [x] **SESS-01**: Historical session shows end time and total duration
- [x] **SESS-02**: Historical session hides Area, Class, and Discipline (noisy/misleading)
- [x] **SESS-03**: Session type icons distinguish live vs historical

### Platform & Infrastructure

- [x] **PLAT-01**: Single instance enforcement prevents multiple app instances
- [x] **PLAT-02**: SVG font fallback for Windows (BARAS header)
- [x] **PLAT-03**: Hotkey settings explain Wayland/Linux limitations clearly
- [x] **PLAT-04**: Alacrity/Latency fields have explanatory tooltips and more prominent placement

### Data Explorer

- [x] **DATA-01**: Combat log table resets scroll position when new encounter selected

## v2 Requirements

Deferred to future release.

- **OVLY-07**: Overlay rendering in parallel (eliminate serial pop-in effect)
- **EDIT-07**: Overlay preview thumbnails in button grid

## Out of Scope

Explicitly excluded from v1.1.

| Feature | Reason |
|---------|--------|
| Data explorer table density reduction | Necessary information, users need all columns |
| MacOS support | Platform complexity, low demand |
| Mobile app | Desktop focus |

## Traceability

Which phases cover which requirements. Updated during roadmap creation.

| Requirement | Phase | Status |
|-------------|-------|--------|
| EMPTY-01 | Phase 9 | Complete |
| EMPTY-02 | Phase 12 | Complete |
| NAV-01 | Phase 10 | Pending |
| NAV-02 | Phase 10 | Pending |
| NAV-03 | Phase 10 | Pending |
| NAV-04 | Phase 9 | Complete |
| PROF-01 | Phase 11 | Complete |
| PROF-02 | Phase 11 | Complete |
| PROF-03 | Phase 11 | Complete |
| OVLY-01 | Phase 12 | Complete |
| OVLY-02 | Phase 12 | Complete |
| OVLY-03 | Phase 12 | Complete |
| OVLY-04 | Phase 12 | Complete |
| OVLY-05 | Phase 12 | Descoped |
| OVLY-06 | Phase 12 | Complete |
| EDIT-01 | Phase 13 | Complete |
| EDIT-02 | Phase 13 | Complete |
| EDIT-03 | Phase 13 | Complete |
| EDIT-04 | Phase 13 | Complete |
| EDIT-05 | Phase 13 | Descoped |
| EDIT-06 | Phase 13 | Complete |
| SESS-01 | Phase 9 | Complete |
| SESS-02 | Phase 9 | Complete |
| SESS-03 | Phase 9 | Complete |
| PLAT-01 | Phase 8 | Complete |
| PLAT-02 | Phase 8 | Complete |
| PLAT-03 | Phase 8 | Complete |
| PLAT-04 | Phase 8 | Complete |
| DATA-01 | Phase 13 | Complete |

**Coverage:**
- v1.1 requirements: 29 total
- Mapped to phases: 29
- Unmapped: 0

---
*Requirements defined: 2026-01-18*
*Last updated: 2026-01-18 - Phase 13 requirements complete*
