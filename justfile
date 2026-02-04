alias r:= run-debug

alias p50:= parse-50mb

run-debug:
  ./target/debug/baras

parse-50mb:
  ./target/debug/baras parse-file --path './test-log-files/50mb/combat_2025-12-10_18_12_15_087604.txt'

# Tauri app commands
dev:
  cargo clean -p baras-parse-worker && \
  cargo build -p baras-parse-worker && \
  mkdir -p app/target/debug && \
  cp target/debug/baras-parse-worker app/target/debug/ && \
  cd app && DEBUG_LOGGING=1 cargo tauri dev 2>&1 | tee /tmp/baras.log

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

run-app-image:
  {{justfile_directory()}}/target/release/bundle/appimage/*.AppImage

update-version:
  cd app/src-tauri && \
  sed -i "s|\"version\": \"[^\"]*\"|\"version\": \"$(date +%Y.%-m.%-d)\"|" tauri.conf.json

validate-revan:
  cargo run --bin baras-validate -- --boss revan --log test-log-files/operations/hm_tos_revan.txt

validate-xr:
  cargo run --bin baras-validate -- --boss propagator_core_xr53 --log test-log-files/operations/hm_propagator.txt
