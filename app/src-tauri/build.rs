use std::{env, fs, path::Path};

fn main() {
    embed_changelogs();
    tauri_build::build()
}

/// Emit `CHANGELOGS: &[(version, markdown)]` from `changelogs/v{version}.md`.
fn embed_changelogs() {
    let dir = Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap()).join("../../changelogs");
    println!("cargo:rerun-if-changed={}", dir.display());

    let mut entries = String::new();
    for entry in fs::read_dir(&dir).expect("changelogs directory") {
        let path = entry.unwrap().path().canonicalize().unwrap();
        if path.extension().is_some_and(|e| e == "md")
            && let Some(version) = path.file_stem().and_then(|s| s.to_str()).and_then(|s| s.strip_prefix('v'))
        {
            entries.push_str(&format!("    ({version:?}, include_str!({path:?})),\n"));
        }
    }

    let out = Path::new(&env::var("OUT_DIR").unwrap()).join("changelogs.rs");
    fs::write(out, format!("pub const CHANGELOGS: &[(&str, &str)] = &[\n{entries}];\n")).unwrap();
}
