# Add Overlay Config Option

Guide for adding a new configuration option to any overlay in the BARAS project.

## Pattern Overview

Adding a config option touches 3 layers: **types** → **overlay** → **UI**. Each overlay follows the same pattern.

## Step 1: Add the field to the config struct (`types/src/lib.rs`)

Find the overlay's config struct (e.g., `BossHealthConfig`, `CombatTimeOverlayConfig`, `TimerOverlayConfig`).

1. Add the field with a `#[serde(default)]` attribute for backward compatibility with existing saved configs:
   ```rust
   /// Description of what this option does
   #[serde(default)]           // defaults to false
   pub my_new_option: bool,
   ```
   - Use `#[serde(default = "default_true")]` if the default should be `true`
   - Use `#[serde(default = "default_scaling_factor")]` for `f32` fields defaulting to `1.0`
   - Use `#[serde(default = "default_font_color")]` for `Color` fields

2. Add the field to the `Default` impl for the same struct, matching the serde default.

## Step 2: Wire the overlay rendering logic (`overlay/src/overlays/<overlay>.rs`)

Modify the overlay's behavior based on the new config field. Common patterns:

- **Behavioral toggle** (e.g., `clear_after_combat`): Guard in `update_data()`:
  ```rust
  fn update_data(&mut self, data: OverlayData) -> bool {
      if let OverlayData::MyOverlay(new_data) = data {
          if should_ignore(&new_data) && !self.config.my_new_option {
              return false;
          }
          self.set_data(new_data);
          true
      } else { false }
  }
  ```
- **Visual toggle** (e.g., `show_target`, `show_title`): Conditional in `render()`:
  ```rust
  if self.config.show_something {
      // draw the element
  }
  ```
- **Numeric value** (e.g., `font_scale`): Use in `render()` calculations:
  ```rust
  let font_size = base_size * self.config.font_scale.clamp(1.0, 2.0);
  ```

No changes needed to `set_config()`, `update_config()`, or `OverlayConfigUpdate` — the config struct flows through unchanged.

## Step 3: Add the UI control (`app/src/components/settings_panel.rs`)

Find the overlay's settings tab (search for the tab name string, e.g., `if tab == "boss_health"`). Add the control before the reset button.

**Checkbox (for bool fields):**
```rust
div { class: "setting-row",
    label { "My New Option" }
    input {
        r#type: "checkbox",
        checked: current_settings.my_overlay.my_new_option,
        onchange: move |e: Event<FormData>| {
            let mut new_settings = draft_settings();
            new_settings.my_overlay.my_new_option = e.checked();
            update_draft(new_settings);
        }
    }
}
```

**Range slider (for f32 fields):**
```rust
div { class: "setting-row",
    label { "My Scale" }
    input {
        r#type: "range",
        min: "100",
        max: "200",
        step: "10",
        value: "{(current_settings.my_overlay.my_scale * 100.0) as i32}",
        oninput: move |e| {
            if let Ok(val) = e.value().parse::<i32>() {
                let mut new_settings = draft_settings();
                new_settings.my_overlay.my_scale = (val as f32 / 100.0).clamp(1.0, 2.0);
                update_draft(new_settings);
            }
        }
    }
    span { class: "value", "{(current_settings.my_overlay.my_scale * 100.0) as i32}%" }
}
```

**Color picker (for Color fields):**
```rust
div { class: "setting-row",
    label { "My Color" }
    input {
        r#type: "color",
        value: "{my_color_hex}",
        class: "color-picker",
        oninput: move |e: Event<FormData>| {
            if let Some(color) = parse_hex_color(&e.value()) {
                let mut new_settings = draft_settings();
                new_settings.my_overlay.my_color = color;
                update_draft(new_settings);
            }
        }
    }
}
```

The reset button already resets to `Default::default()` — no changes needed there.

## Verification

```bash
cargo check -p baras-types                                    # types compile
cargo check -p baras-overlay                                  # overlay compiles
cd app && cargo check --target wasm32-unknown-unknown          # frontend compiles
```

## Key Points

- `#[serde(default)]` is **required** — existing user configs won't have the new field
- The config struct flows through `OverlayConfigUpdate` as-is — no enum changes needed
- `update_draft()` handles debounced preview via the existing 300ms timer
- Reset button uses `Default::default()` so it automatically picks up new defaults
- Config structs are defined in `types/` but re-exported through `core/src/context/`
