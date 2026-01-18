# Requirements: BARAS

**Defined:** 2026-01-18
**Core Value:** Fast, reliable combat analysis that doesn't crash when something unexpected happens.

## v1.1 Requirements

Requirements for UX Polish milestone. Eliminate confusion and friction in the UI.

### Empty States

- [ ] **EMPTY-01**: User sees helpful "Waiting for combat data..." message instead of "Unknown Session"
- [ ] **EMPTY-02**: Overlays display last encounter data on startup instead of appearing blank

### Navigation & Status

- [ ] **NAV-01**: Live mode shows green arrow indicator in central top bar position
- [ ] **NAV-02**: Historical mode shows pause/stop icon with history indicator
- [ ] **NAV-03**: "Resume Live" button prominently visible when in historical mode
- [ ] **NAV-04**: Parsely upload button on session page (not buried in file explorer)

### Profile System

- [ ] **PROF-01**: Profile switching does not toggle overlay visibility
- [ ] **PROF-02**: Raid frames re-render correctly after profile switch
- [ ] **PROF-03**: Profile selector more visible in overlay settings (not collapsed by default)

### Overlay System

- [ ] **OVLY-01**: Move mode always resets to false on application startup
- [ ] **OVLY-02**: Save button in fixed header position (visible without scrolling)
- [ ] **OVLY-03**: Customization changes preview live before saving
- [ ] **OVLY-04**: Overlay buttons have brief descriptions of what they do
- [ ] **OVLY-05**: REARRANGE FRAMES and CLEAR FRAMES grouped with Raid Frames section
- [ ] **OVLY-06**: CUSTOMIZE button clearly indicates it opens settings

### Effects & Encounter Builder

- [ ] **EDIT-01**: Form fields have tooltips explaining their purpose (Display Target, Trigger, etc.)
- [ ] **EDIT-02**: Better visual grouping/hierarchy separating form sections
- [ ] **EDIT-03**: Empty state shows guidance for creating first entry
- [ ] **EDIT-04**: New effects/timers appear at top of list (not bottom)
- [ ] **EDIT-05**: Stats reordering uses drag-and-drop instead of up/down buttons
- [ ] **EDIT-06**: Raw file path removed from Encounter Builder

### Session Display

- [ ] **SESS-01**: Historical session shows end time and total duration
- [ ] **SESS-02**: Historical session hides Area, Class, and Discipline (noisy/misleading)
- [ ] **SESS-03**: Session type icons distinguish live vs historical

### Platform & Infrastructure

- [ ] **PLAT-01**: Single instance enforcement prevents multiple app instances
- [ ] **PLAT-02**: SVG font fallback for Windows (BARAS header)
- [ ] **PLAT-03**: Hotkey settings explain Wayland/Linux limitations clearly
- [ ] **PLAT-04**: Alacrity/Latency fields have explanatory tooltips and more prominent placement

### Data Explorer

- [ ] **DATA-01**: Combat log table resets scroll position when new encounter selected

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
| EMPTY-01 | TBD | Pending |
| EMPTY-02 | TBD | Pending |
| NAV-01 | TBD | Pending |
| NAV-02 | TBD | Pending |
| NAV-03 | TBD | Pending |
| NAV-04 | TBD | Pending |
| PROF-01 | TBD | Pending |
| PROF-02 | TBD | Pending |
| PROF-03 | TBD | Pending |
| OVLY-01 | TBD | Pending |
| OVLY-02 | TBD | Pending |
| OVLY-03 | TBD | Pending |
| OVLY-04 | TBD | Pending |
| OVLY-05 | TBD | Pending |
| OVLY-06 | TBD | Pending |
| EDIT-01 | TBD | Pending |
| EDIT-02 | TBD | Pending |
| EDIT-03 | TBD | Pending |
| EDIT-04 | TBD | Pending |
| EDIT-05 | TBD | Pending |
| EDIT-06 | TBD | Pending |
| SESS-01 | TBD | Pending |
| SESS-02 | TBD | Pending |
| SESS-03 | TBD | Pending |
| PLAT-01 | TBD | Pending |
| PLAT-02 | TBD | Pending |
| PLAT-03 | TBD | Pending |
| PLAT-04 | TBD | Pending |
| DATA-01 | TBD | Pending |

**Coverage:**
- v1.1 requirements: 29 total
- Mapped to phases: 0
- Unmapped: 29

---
*Requirements defined: 2026-01-18*
*Last updated: 2026-01-18 after initial definition*
