alias r:= run-debug
alias p50:= parse-50mb

run-debug:
  ./target/debug/baras

parse-50mb:
  ./target/debug/baras parse-file --path './test-log-files/50mb/combat_2025-12-10_18_12_15_087604.txt'

# Tauri app commands
dev:
  cd app && cargo tauri dev 2>&1 | tee /tmp/baras.log

# Build AppImage/deb (NO_STRIP needed on Arch due to linuxdeploy incompatibility)
bundle:
  cd app && NO_STRIP=1 cargo tauri build

# Build release binary only (no bundle)
build-app:
  cd app && cargo tauri build --no-bundle

run-app-image:
  {{justfile_directory()}}/target/release/bundle/appimage/*.AppImage

update-version:
  cd app/src-tauri && \
  sed -i "s|\"version\": \"[^\"]*\"|\"version\": \"$(date +%Y.%m.%d)\"|" tauri.conf.json

validate-revan:
  cargo run --bin baras-validate -- --boss revan --log test-log-files/operations/hm_tos_revan.txt
