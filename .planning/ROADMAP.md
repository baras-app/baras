# Roadmap: BARAS

## Milestones

- v1.0 Tech Debt Cleanup - Phases 1-7 (shipped 2026-01-18)
- v1.1 UX Polish - Phases 8-13 (in progress)

## Overview

v1.1 polishes the user experience by eliminating confusion and friction. Work progresses from platform foundation through session display, navigation redesign, profile fixes, overlay improvements, and editor polish. Each phase delivers observable improvements to specific UI areas.

## Phases

<details>
<summary>v1.0 Tech Debt Cleanup (Phases 1-7) - SHIPPED 2026-01-18</summary>

See: `.planning/milestones/v1.0-ROADMAP.md`

- [x] Phase 1: Logging Foundation (2 plans)
- [x] Phase 2: Core Error Types (3 plans)
- [x] Phase 3: Core Error Handling (3 plans)
- [x] Phase 4: Backend Error Handling (3 plans)
- [x] Phase 5: Frontend Error Handling (4 plans)
- [x] Phase 6: Logging Migration (4 plans)
- [x] Phase 7: Clone Cleanup (3 plans)

</details>

### v1.1 UX Polish (In Progress)

**Milestone Goal:** Eliminate confusion and friction in the UI - empty states, unclear controls, buried features, and workflow inconsistencies.

- [x] **Phase 8: Platform Foundation** - Infrastructure fixes that enable cleaner UX
- [x] **Phase 9: Session Page Polish** - Empty states, historical display, Parsely access
- [ ] **Phase 10: Navigation Redesign** - Live/historical indicator with Resume Live action
- [ ] **Phase 11: Profile System Fixes** - Visibility toggle decoupling and re-render fixes
- [ ] **Phase 12: Overlay Improvements** - Move mode, save button, live preview, descriptions
- [ ] **Phase 13: Editor Polish** - Tooltips, hierarchy, drag-drop, scroll behavior

## Phase Details

### Phase 8: Platform Foundation
**Goal**: Infrastructure and platform-specific fixes that enable a cleaner user experience
**Depends on**: Nothing (first phase of v1.1)
**Requirements**: PLAT-01, PLAT-02, PLAT-03, PLAT-04
**Success Criteria** (what must be TRUE):
  1. User cannot launch second app instance (gets focus redirect or error)
  2. BARAS header renders correctly on Windows without missing glyphs
  3. Hotkey settings page explains Wayland/Linux limitations before user gets confused
  4. Alacrity/Latency fields have tooltips and are not buried at bottom of form
**Plans**: 2 plans

Plans:
- [x] 08-01-PLAN.md - Single instance enforcement + hotkey limitation docs
- [x] 08-02-PLAN.md - Windows font fix + Alacrity/Latency relocation

### Phase 9: Session Page Polish
**Goal**: Session page shows helpful states and relevant information without noise
**Depends on**: Phase 8
**Requirements**: EMPTY-01, SESS-01, SESS-02, SESS-03, NAV-04
**Success Criteria** (what must be TRUE):
  1. User sees "Waiting for combat data..." instead of "Unknown Session" when no data
  2. Historical session shows end time and total duration prominently
  3. Historical session hides Area, Class, and Discipline (reduces noise)
  4. User can distinguish live vs historical sessions via clear icon/badge
  5. User can upload to Parsely directly from session page (not hunting in file explorer)
**Plans**: 2 plans

Plans:
- [x] 09-01-PLAN.md - Backend enhancements (SessionInfo fields, watcher fix)
- [x] 09-02-PLAN.md - Frontend session page polish (empty states, display, indicators, upload)

### Phase 10: Navigation Redesign
**Goal**: User always knows current mode (live vs historical) and can resume live easily
**Depends on**: Phase 9
**Requirements**: NAV-01, NAV-02, NAV-03
**Success Criteria** (what must be TRUE):
  1. Live mode shows green arrow indicator in central top bar position
  2. Historical mode shows pause/stop icon with clear "viewing history" visual
  3. User can click prominent "Resume Live" button when in historical mode
**Plans**: TBD

Plans:
- [ ] 10-01: TBD

### Phase 11: Profile System Fixes
**Goal**: Profile switching works reliably without side effects
**Depends on**: Phase 10
**Requirements**: PROF-01, PROF-02, PROF-03
**Success Criteria** (what must be TRUE):
  1. Switching profile does not change overlay visibility state
  2. Raid frames render correctly after profile switch (no stale/broken state)
  3. Profile selector is visible by default in overlay settings (not collapsed)
**Plans**: TBD

Plans:
- [ ] 11-01: TBD

### Phase 12: Overlay Improvements
**Goal**: Overlay customization is intuitive with clear controls and immediate feedback
**Depends on**: Phase 11
**Requirements**: OVLY-01, OVLY-02, OVLY-03, OVLY-04, OVLY-05, OVLY-06, EMPTY-02
**Success Criteria** (what must be TRUE):
  1. Move mode is always off when app starts (never persists from previous session)
  2. Save button visible without scrolling (fixed header position)
  3. Customization changes preview live before user commits
  4. Overlay buttons have brief descriptions explaining what they do
  5. REARRANGE FRAMES and CLEAR FRAMES buttons grouped with Raid Frames section
  6. Overlays display last encounter data on startup (not blank)
**Plans**: TBD

Plans:
- [ ] 12-01: TBD

### Phase 13: Editor Polish
**Goal**: Effects and Encounter Builder are clear and efficient to use
**Depends on**: Phase 12
**Requirements**: EDIT-01, EDIT-02, EDIT-03, EDIT-04, EDIT-05, EDIT-06, DATA-01
**Success Criteria** (what must be TRUE):
  1. Form fields have tooltips explaining their purpose (Display Target, Trigger, etc.)
  2. Form sections have clear visual hierarchy and grouping
  3. Empty state shows guidance for creating first entry
  4. New effects/timers appear at top of list (most recent visible)
  5. Stats reordering uses drag-and-drop (not tedious up/down buttons)
  6. Raw file path removed from Encounter Builder (cleaner UI)
  7. Combat log table resets scroll position when new encounter selected
**Plans**: TBD

Plans:
- [ ] 13-01: TBD

## Progress

**Execution Order:** 8 -> 9 -> 10 -> 11 -> 12 -> 13

| Phase | Milestone | Plans Complete | Status | Completed |
|-------|-----------|----------------|--------|-----------|
| 1-7 | v1.0 | 23/23 | Complete | 2026-01-18 |
| 8. Platform Foundation | v1.1 | 2/2 | Complete | 2026-01-18 |
| 9. Session Page Polish | v1.1 | 2/2 | Complete | 2026-01-18 |
| 10. Navigation Redesign | v1.1 | 0/? | Not started | - |
| 11. Profile System Fixes | v1.1 | 0/? | Not started | - |
| 12. Overlay Improvements | v1.1 | 0/? | Not started | - |
| 13. Editor Polish | v1.1 | 0/? | Not started | - |

---
*Roadmap created: 2026-01-18*
*Last updated: 2026-01-18 - Phase 9 complete*
