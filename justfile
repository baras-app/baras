default:
  @just --list

# Development copies keep their config, history and caches here, apart from the installed app's
dev_config := env("BARAS_DEV_CONFIG_DIR", config_directory() / "baras-dev")

# No sidecar copy in dev: tauri-build repeats it on every definition edit, replacing the fresh parse-worker with the last bundled one
# A checkout of the maps repository: dev builds read replay maps from it instead of downloading (set it empty to download)
maps_repo := env("BARAS_MAPS_DIR", justfile_directory() / ".." / "baras-maps")

tauri_dev := """cargo tauri dev --config '{"bundle":{"externalBin":[]}}'"""

# Run the app with hot-reload against the dev config directory (the app finds the parse-worker next to its own exe in target/debug)
dev:
  cargo build -p baras-parse-worker && \
  cd app && BARAS_CONFIG_DIR="{{dev_config}}" BARAS_MAPS_DIR="{{maps_repo}}" DEBUG_LOGGING=1 {{tauri_dev}} 2>&1 | tee /tmp/baras.log

# Same as `dev`, but on the installed app's own config directory
dev-live:
  cargo build -p baras-parse-worker && \
  cd app && BARAS_MAPS_DIR="{{maps_repo}}" DEBUG_LOGGING=1 {{tauri_dev}} 2>&1 | tee /tmp/baras.log

# Copy the installed app's settings, definitions and characters into the dev config directory (not its history, parsed data or logs)
dev-seed:
  #!/usr/bin/env bash
  set -euo pipefail
  src="{{config_directory()}}/baras"
  mkdir -p "{{dev_config}}"
  for item in config.toml characters.json library.json timer_preferences.toml definitions sounds models; do
    [[ -e "$src/$item" ]] && cp -r "$src/$item" "{{dev_config}}/"
  done
  echo "✓ Seeded {{dev_config}} from $src"

# Type-check the backend and the WASM frontend
check: css-check
  cargo check -p app
  cd app && cargo check --target wasm32-unknown-unknown

# Fail on a CSS class name claimed by more than one feature (the stylesheets are one namespace)
css-check:
  python3 scripts/check_css_classes.py

# Everything the Check workflow gates on: formatting, clippy with warnings as errors, and tests
lint: css-check
  cargo fmt --all --check
  cargo clippy --workspace --exclude app-ui --all-targets -- -D warnings
  cd app && cargo clippy --target wasm32-unknown-unknown -- -D warnings

# Same tests the Check workflow runs on pull requests
test:
  cargo test --locked --workspace --exclude app-ui

# Delete build artifacts not touched in the last two weeks (target/ grows without bound otherwise)
sweep:
  cargo sweep --time 14

# Build parse-worker and copy to binaries dir with platform-specific name
build-parse-worker:
  #!/usr/bin/env bash
  set -euo pipefail
  cargo build --release -p baras-parse-worker
  TARGET=$(rustc -vV | grep host | cut -d' ' -f2)
  mkdir -p app/src-tauri/binaries
  EXE=""; [[ "$TARGET" == *windows* ]] && EXE=".exe"
  cp "target/release/baras-parse-worker${EXE}" \
     "app/src-tauri/binaries/baras-parse-worker-${TARGET}${EXE}"
  echo "✓ parse-worker binary updated for ${TARGET}"

# Build AppImage/deb (NO_STRIP needed on Arch due to linuxdeploy incompatibility)
bundle: build-parse-worker
  cd app && NO_STRIP=1 cargo tauri build

# Build release binary only (no bundle)
build-app:
  cd app && cargo tauri build --no-bundle

# Run the newest AppImage `just bundle` produced, passing any args through
run-app-image *args:
  #!/usr/bin/env bash
  set -euo pipefail
  img=$(ls -t "{{justfile_directory()}}"/target/release/bundle/appimage/*.AppImage 2>/dev/null | head -n 1 || true)
  [[ -n "$img" ]] || { echo "No AppImage found: run 'just bundle' first"; exit 1; }
  echo "▶ ${img##*/}"
  exec "$img" {{args}}

# Replay a log against a boss definition, e.g. `just validate revan test-log-files/operations/hm_tos_revan.txt`
validate boss log *args:
  cargo run --bin baras-validate -- --boss {{boss}} --log {{log}} {{args}}

# Record effect-tracker timelines for every test log as the baseline for `just effects-diff`
effects-golden defs="core/definitions":
  just _effects-timelines golden {{defs}}

# Replay every test log and diff the effect-tracker timelines against `just effects-golden`
effects-diff defs="core/definitions":
  #!/usr/bin/env bash
  set -euo pipefail
  just _effects-timelines current {{defs}}
  tag=$(basename {{defs}})
  diff -r target/effects-timeline/golden/$tag target/effects-timeline/current/$tag && echo "✓ timelines identical"

_effects-timelines kind defs:
  #!/usr/bin/env bash
  set -euo pipefail
  cargo build -q --release -p baras-validate
  out=target/effects-timeline/{{kind}}/$(basename {{defs}})
  rm -rf "$out" && mkdir -p "$out"
  find test-log-files -name '*.txt' -print0 | xargs -0 -P "$(nproc)" -I{} sh -c \
    'n=$(echo "{}" | sed "s|^test-log-files/||; s|/|__|g"); target/release/baras-validate -l "{}" -d "{{defs}}" --effects-timeline "'"$out"'/$n" 2>"'"$out"'/$n.coverage" </dev/null'
  echo "✓ $(ls "$out" | grep -vc coverage) timelines in $out"

# ─────────────────────────────────────────────────────────────────────────
# Releasing
# ─────────────────────────────────────────────────────────────────────────

# Start the next version: bump tauri.conf.json to today's date and open its changelogs/ file
new-version:
  #!/usr/bin/env bash
  set -euo pipefail
  CONF=app/src-tauri/tauri.conf.json
  OLD=$(jq -r '.version' "$CONF")
  NEW=$(date +%Y.%-m.%-d)
  [[ "$OLD" != "$NEW" ]] || { echo "Version is already $NEW"; exit 1; }
  [[ ! -e "changelogs/v$NEW.md" ]] || { echo "changelogs/v$NEW.md already exists"; exit 1; }
  printf '# v%s\n\n' "$NEW" > "changelogs/v$NEW.md"
  sed -i "s|\"version\": \"$OLD\"|\"version\": \"$NEW\"|" "$CONF"
  echo "✓ $OLD → $NEW; write the notes in changelogs/v$NEW.md"

# Publish the version in tauri.conf.json from master via the Build workflow
release:
  #!/usr/bin/env bash
  set -euo pipefail
  VERSION=$(jq -r '.version' app/src-tauri/tauri.conf.json)
  [[ "$(git branch --show-current)" == master ]] || { echo "Releases are cut from master"; exit 1; }
  [[ -z "$(git status --porcelain)" ]] || { echo "Working tree is not clean"; exit 1; }
  git fetch -q origin master
  [[ "$(git rev-parse HEAD)" == "$(git rev-parse origin/master)" ]] || { echo "master differs from origin/master"; exit 1; }
  [[ "$(head -n 1 "changelogs/v$VERSION.md" 2> /dev/null)" == "# v$VERSION" ]] || { echo "changelogs/v$VERSION.md is missing or does not start with '# v$VERSION'"; exit 1; }
  gh workflow run build.yml --ref master -f release=true
  echo "✓ Release v$VERSION started; follow it with: gh run watch"

# Build the current branch in CI without releasing (args pass to gh, e.g. -f build_macos=false)
ci-build *args:
  gh workflow run build.yml --ref "$(git branch --show-current)" {{args}}

# Sync encounter definitions to build output (for dev without full rebuild)
sync-defs:
  #!/usr/bin/env bash
  set -euo pipefail
  DEST="target/debug/definitions/encounters"
  mkdir -p "$DEST"
  find core/definitions/encounters -name '*.toml' -exec cp -t "$DEST" {} +
  echo "✓ Synced $(ls "$DEST"/*.toml | wc -l) encounter definitions"

build-validate-windows:
  cross build --release -p baras-validate --target x86_64-pc-windows-gnu

# Extract every operation's and PvP zone's replay maps (images + world bounds) from the game client into the maps repository checkout (env: BARAS_MAPS_DIR, SWTOR_EXTRACTOR, SWTOR_ASSETS; needs ImageMagick)
extract-maps:
  #!/usr/bin/env bash
  set -euo pipefail
  EXTRACTOR="${SWTOR_EXTRACTOR:-$HOME/dedicatedabilitydataswtorsextractor}"
  ASSETS="${SWTOR_ASSETS:-$HOME/.local/share/Steam/steamapps/common/Star Wars - The Old Republic/Assets}"
  PY="$EXTRACTOR/.venv/bin/python"
  [[ -x "$PY" ]] || { echo "No venv at $EXTRACTOR/.venv (python -m venv .venv && .venv/bin/pip install zstandard)"; exit 1; }
  [[ -d "$ASSETS" ]] || { echo "Assets folder not found: $ASSETS"; exit 1; }
  command -v magick >/dev/null || { echo "ImageMagick (magick) not found"; exit 1; }
  [[ -f "{{maps_repo}}/extract_maps.py" ]] || { echo "No maps repository checkout at {{maps_repo}} (git clone git@github.com:baras-app/baras-maps.git)"; exit 1; }
  "$PY" "{{maps_repo}}/extract_maps.py" --assets "$ASSETS" --extractor "$EXTRACTOR" \
    --encounters "{{justfile_directory()}}/core/definitions/encounters/operations" \
    --areas "{{justfile_directory()}}/core/data/pvp_areas.csv"

# Run the SWTOR ability extractor and copy ability_data.csv + effect_data.csv into core/data (env: SWTOR_EXTRACTOR, SWTOR_ASSETS; args pass to main.py, e.g. --no-npc)
extract-abilities *args:
  #!/usr/bin/env bash
  set -euo pipefail
  EXTRACTOR="${SWTOR_EXTRACTOR:-$HOME/dedicatedabilitydataswtorsextractor}"
  ASSETS="${SWTOR_ASSETS:-$HOME/.local/share/Steam/steamapps/common/Star Wars - The Old Republic/Assets}"
  PY="$EXTRACTOR/.venv/bin/python"
  [[ -x "$PY" ]] || { echo "No venv at $EXTRACTOR/.venv (python -m venv .venv && .venv/bin/pip install zstandard)"; exit 1; }
  [[ -d "$ASSETS" ]] || { echo "Assets folder not found: $ASSETS"; exit 1; }
  (cd "$EXTRACTOR" && "$PY" main.py --assets "$ASSETS" {{args}})
  for f in ability_data effect_data; do
    cp "$EXTRACTOR/data/extracted/$f.csv" "{{justfile_directory()}}/core/data/$f.csv"
    echo "✓ core/data/$f.csv updated ($(($(wc -l < "{{justfile_directory()}}/core/data/$f.csv") - 1)) rows)"
  done
