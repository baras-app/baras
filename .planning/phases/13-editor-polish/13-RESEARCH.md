# Phase 13: Editor Polish - Research

**Researched:** 2026-01-18
**Domain:** Dioxus UI polish - tooltips, visual hierarchy, list management, scroll behavior
**Confidence:** HIGH

## Summary

Phase 13 focuses on making the Effects Editor and Encounter Builder more intuitive through tooltips, visual hierarchy improvements, list ordering enhancements, and scroll behavior fixes. The existing codebase already uses `title` attributes for native browser tooltips extensively (40+ instances in `app.rs` alone), which is the established pattern.

Key findings:
- Native `title` attribute tooltips are the established pattern - used throughout the codebase
- Dioxus drag-and-drop was broken but has been fixed in recent versions (PR #3137); however, CONTEXT.md notes Dioxus lacks support, so keep up/down buttons as decided
- Card-based sections with distinct headers are the decided visual hierarchy pattern
- New effects already appear at top of list (draft effect rendered first in `effect_editor.rs`)
- Combat log scroll reset requires clearing `scroll_top` signal when encounter changes

**Primary recommendation:** Use native `title` attributes for tooltips, implement card-based sections with CSS, remove file path display, add scroll reset on encounter selection.

## Standard Stack

The established libraries/tools for this domain:

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| Dioxus | 0.6+ | Frontend framework | Already in use, provides all needed event handling |
| Native HTML `title` | N/A | Tooltips | Used 40+ times in codebase, zero dependencies |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| CSS Variables | N/A | Consistent styling | Already defined in `styles.css` |
| FontAwesome | N/A | Section header icons | Already in use for icons throughout |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Native `title` | Custom tooltip component | More complexity, but allows richer styling - not needed per CONTEXT.md |
| CSS cards | Component library | Adds dependency, existing CSS patterns sufficient |
| Drag-and-drop | Up/down buttons | CONTEXT.md explicitly states Dioxus lacks support, keep buttons |

**Installation:**
No new dependencies required - all implementation uses existing stack.

## Architecture Patterns

### Current Project Structure (Relevant)
```
app/src/
├── components/
│   ├── effect_editor.rs      # Effects Editor (main target)
│   ├── encounter_editor/     # Encounter Builder (secondary target)
│   │   ├── mod.rs            # Main panel, file path displayed here
│   │   ├── timers.rs         # Timer forms (tooltip targets)
│   │   ├── entities.rs       # Entity forms
│   │   ├── phases.rs         # Phase forms
│   │   └── ...
│   ├── combat_log.rs         # Scroll reset target
│   └── data_explorer.rs      # Encounter selection context
└── assets/
    └── styles.css            # CSS variables and patterns
```

### Pattern 1: Native Tooltip via `title` Attribute
**What:** Use HTML `title` attribute for hover tooltips
**When to use:** All form field labels needing explanation
**Example:**
```rust
// Source: Existing pattern from app/src/app.rs
span {
    title: "Shows boss health bars and cast timers",
    class: "btn-header-overlay",
    // content
}
```

### Pattern 2: Icon-Labeled Help Indicator
**What:** Small question mark icon indicating tooltip presence
**When to use:** Fields where the `title` tooltip needs a visual indicator
**Example:**
```rust
// Recommended pattern combining icon with title
label { class: "flex items-center gap-xs",
    "Display Target"
    span {
        class: "help-icon",
        title: "Which overlay shows this effect when triggered",
        "?"
    }
}
```

### Pattern 3: Card-Based Section Grouping
**What:** Visually distinct cards with headers for form sections
**When to use:** Grouping related form fields (Identity, Trigger, Options, etc.)
**Example:**
```rust
// Card section pattern
div { class: "form-card",
    div { class: "form-card-header",
        i { class: "fa-solid fa-tag" }
        span { "Identity" }
    }
    div { class: "form-card-content",
        // Form fields
    }
}
```

### Pattern 4: Empty State with Guidance
**What:** Helpful message when list is empty with action guidance
**When to use:** Effect list, timer list, when no items exist
**Example:**
```rust
// Source: Modified from existing pattern in effect_editor.rs:428
if effects().is_empty() && draft_effect().is_none() {
    div { class: "empty-state-guidance",
        div { class: "empty-state-icon", "+" }
        p { "No effects defined yet" }
        p { class: "hint", "Click \"+ New Effect\" above to create your first effect" }
    }
}
```

### Pattern 5: Scroll Position Reset
**What:** Reset scroll position when context changes
**When to use:** Combat log when encounter selection changes
**Example:**
```rust
// Source: Existing pattern from combat_log.rs:147-149
spawn(async move {
    // Reset scroll position when filters change
    scroll_top.set(0.0);
    loaded_offset.set(0);
    // ... load data
});
```

### Anti-Patterns to Avoid
- **Custom tooltip components:** Adds unnecessary complexity when `title` works
- **Collapsible sections:** CONTEXT.md explicitly states "No collapsible sections - all sections visible at once"
- **File path display:** CONTEXT.md states "Raw file path removed from Encounter Builder entirely"
- **Drag-and-drop for reordering:** CONTEXT.md notes Dioxus lacks support, keep up/down buttons

## Don't Hand-Roll

Problems that look simple but have existing solutions:

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Tooltips | Custom tooltip overlay component | Native `title` attribute | Built-in, accessible, zero dependencies |
| Visual cards | Custom box-shadow JS effects | CSS `.form-card` class | CSS is simpler, performant |
| Section icons | Custom icon components | FontAwesome `fa-solid fa-*` | Already integrated in project |
| Scroll management | Custom scroll controller | Dioxus signal + `scroll_top.set(0.0)` | Pattern already exists in codebase |

**Key insight:** The codebase already has all the patterns needed. This phase is about applying existing patterns consistently, not building new infrastructure.

## Common Pitfalls

### Pitfall 1: Tooltip Content in Wrong Attribute
**What goes wrong:** Using `tooltip`, `data-tooltip`, or custom attributes instead of `title`
**Why it happens:** Framework habit from other systems
**How to avoid:** Always use native HTML `title` attribute
**Warning signs:** Tooltips not appearing on hover

### Pitfall 2: Forgetting to Reset Scroll on Data Change
**What goes wrong:** Combat log shows old scroll position with new data
**Why it happens:** Scroll state persists across data reloads
**How to avoid:** Reset `scroll_top.set(0.0)` in the `use_effect` that handles encounter changes
**Warning signs:** User selecting new encounter but seeing middle of list

### Pitfall 3: Breaking Existing List Ordering
**What goes wrong:** Effects editor already shows drafts at top, changes break this
**Why it happens:** Not understanding existing draft rendering order
**How to avoid:** Check current implementation - draft is already rendered first in `effect_editor.rs:437-461`
**Warning signs:** New effects appearing at bottom

### Pitfall 4: Adding Collapsible Sections When Told Not To
**What goes wrong:** Implementing `<details>` or accordion patterns
**Why it happens:** Common UI pattern instinct
**How to avoid:** CONTEXT.md explicitly states "No collapsible sections"
**Warning signs:** Any `<details>` or collapse logic in form sections

### Pitfall 5: Over-Styling Cards
**What goes wrong:** Cards with heavy shadows, animations, or complex borders
**Why it happens:** Wanting visual impact
**How to avoid:** Keep styling minimal - subtle border, small header background, matching existing UI
**Warning signs:** Cards that stand out more than content

## Code Examples

Verified patterns from the codebase:

### Existing Tooltip Pattern
```rust
// Source: /home/prescott/baras/app/src/app.rs:972
span {
    title: "Shows boss health bars and cast timers",
    class: "btn-header-overlay",
    onclick: move |_| { /* ... */ },
    i { class: "fa-solid fa-heart-pulse" }
}
```

### Existing Scroll Reset Pattern
```rust
// Source: /home/prescott/baras/app/src/components/combat_log.rs:146-149
spawn(async move {
    // Reset scroll position
    scroll_top.set(0.0);
    loaded_offset.set(0);
    // ... rest of data loading
});
```

### Existing Draft-First Rendering Pattern
```rust
// Source: /home/prescott/baras/app/src/components/effect_editor.rs:436-461
div { class: "effect-list",
    // Draft effect at the top (if any)
    if let Some(draft) = draft_effect() {
        EffectRow {
            // ... draft row
        }
    }
    // Existing effects below
    for effect in filtered_effects() {
        EffectRow {
            // ... existing effect rows
        }
    }
}
```

### CSS Variables Available for Cards
```css
/* Source: /home/prescott/baras/app/assets/styles.css */
--bg-mid: #252530;
--bg-light: #2f2f38;
--border-light: rgba(255, 255, 255, 0.1);
--border-medium: rgba(255, 255, 255, 0.15);
--radius-md: 4px;
--space-md: 0.75em;
```

### Recommended Card CSS Pattern
```css
/* New CSS for form cards */
.form-card {
  background: var(--bg-mid);
  border: 1px solid var(--border-light);
  border-radius: var(--radius-md);
  margin-bottom: var(--space-md);
}

.form-card-header {
  display: flex;
  align-items: center;
  gap: var(--space-sm);
  padding: var(--space-sm) var(--space-md);
  background: rgba(255, 255, 255, 0.03);
  border-bottom: 1px solid var(--border-subtle);
  font-size: 0.85em;
  color: var(--text-secondary);
}

.form-card-header i {
  color: var(--swtor-blue-dim);
}

.form-card-content {
  padding: var(--space-md);
}
```

### Recommended Help Icon CSS
```css
/* Help icon for tooltip indication */
.help-icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 14px;
  height: 14px;
  font-size: 10px;
  background: var(--bg-light);
  border: 1px solid var(--border-light);
  border-radius: 50%;
  color: var(--text-muted);
  cursor: help;
}

.help-icon:hover {
  color: var(--swtor-blue);
  border-color: var(--border-accent);
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Custom tooltip libraries | Native `title` + CSS | Established | Zero dependencies, accessible |
| Dioxus DragEvent broken | DragEvent fixed (PR #3137) | Oct 2024 | Drag-drop now possible BUT CONTEXT.md says keep buttons |
| Flat forms | Card-based sections | Phase 13 | Better visual hierarchy |

**Deprecated/outdated:**
- None relevant to this phase

## Open Questions

Things that couldn't be fully resolved:

1. **Exact tooltip wording**
   - What we know: Functional tone decided ("Shows X", "Triggers when Y")
   - What's unclear: Exact wording for each field
   - Recommendation: Define during implementation, Claude's discretion per CONTEXT.md

2. **Which fields need tooltips**
   - What we know: Ambiguous fields only (Display Target, Trigger, Comparison)
   - What's unclear: Complete list of "ambiguous" fields
   - Recommendation: Start with Display Target, Trigger Type, Source/Target filters, Alert On

3. **Section header icons**
   - What we know: Icons + text for card headers, FontAwesome available
   - What's unclear: Which specific icons for each section
   - Recommendation: Claude's discretion - suggest: fa-tag (Identity), fa-bolt (Trigger), fa-cog (Options), fa-bell (Alerts), fa-volume-high (Audio)

## Sources

### Primary (HIGH confidence)
- `/home/prescott/baras/app/src/app.rs` - Existing tooltip patterns (40+ instances)
- `/home/prescott/baras/app/src/components/effect_editor.rs` - Current Effects Editor implementation
- `/home/prescott/baras/app/src/components/combat_log.rs` - Scroll reset pattern
- `/home/prescott/baras/app/assets/styles.css` - CSS variables and patterns
- `/home/prescott/baras/.planning/phases/13-editor-polish/13-CONTEXT.md` - User decisions

### Secondary (MEDIUM confidence)
- [GitHub - DioxusLabs/dioxus Issue #3133](https://github.com/DioxusLabs/dioxus/issues/3133) - Drag event fix status
- [dioxus-components crates.io](https://crates.io/crates/dioxus-components) - Available tooltip components (not recommended for this project)

### Tertiary (LOW confidence)
- WebSearch results on Dioxus patterns - General ecosystem information

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - Uses only existing dependencies and patterns
- Architecture: HIGH - All patterns verified in existing codebase
- Pitfalls: HIGH - Based on CONTEXT.md decisions and code analysis

**Research date:** 2026-01-18
**Valid until:** Indefinite - uses stable native HTML patterns

---

*Phase: 13-editor-polish*
*Research completed: 2026-01-18*
