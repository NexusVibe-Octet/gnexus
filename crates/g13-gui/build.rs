use std::path::PathBuf;
use std::process::Command;

fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        build_audio_effect();
    }
    tauri_build::build()
}

/// G Nexus's audio effect (`g13-apo`, a DLL loaded by Windows' audio engine) is
/// built here and embedded in the app, which installs it when the user asks for the
/// equalizer. Its own target directory: the outer build holds the main one.
fn build_audio_effect() {
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let workspace = manifest.parent().and_then(|p| p.parent()).expect("workspace").to_path_buf();
    let target = out.join("apo-target");
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let status = Command::new(cargo)
        .current_dir(&workspace)
        .args(["build", "-p", "g13-apo", "--lib", "--release", "--target-dir"])
        .arg(&target)
        .env_remove("CARGO_TARGET_DIR")
        .env_remove("RUSTFLAGS")
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .status()
        .expect("cargo build g13-apo");
    assert!(status.success(), "g13-apo build failed");
    std::fs::copy(target.join("release").join("g13_apo.dll"), out.join("g13_apo.dll")).expect("copy g13_apo.dll");
    for dir in ["g13-apo/src", "g13-dsp/src"] {
        println!("cargo:rerun-if-changed={}", workspace.join("crates").join(dir).display());
    }
}
