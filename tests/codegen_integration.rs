use std::path::Path;

use aw_kit::{codegen, manifest::ManifestConfig, platform::resolve_platform, resolver};

fn fixture(name: &str) -> ManifestConfig {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    ManifestConfig::from_file(&path).unwrap()
}

#[test]
fn generate_all_minimal() {
    let dir = tempfile::tempdir().unwrap();
    let manifest = fixture("minimal.toml");
    let platform = resolve_platform(&manifest.platform).unwrap();
    let plan = resolver::resolve(&manifest, &platform).unwrap();

    codegen::generate_all(&manifest, &plan, &platform, dir.path()).unwrap();

    // Compose file should exist.
    let compose = dir.path().join(".aw-kit/compose/docker-compose.yml");
    assert!(compose.exists());
    let content = std::fs::read_to_string(&compose).unwrap();
    assert!(content.contains("services:"));
    assert!(content.contains("network_mode: host"));

    // No Dockerfiles for minimal (pull-only).
    let build_dir = dir.path().join(".aw-kit/build");
    let dockerfiles: Vec<_> = std::fs::read_dir(&build_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "Dockerfile"))
        .collect();
    assert!(dockerfiles.is_empty());

    // .gitignore should exist.
    assert!(dir.path().join(".aw-kit/.gitignore").exists());
}

#[test]
fn generate_all_patched() {
    let dir = tempfile::tempdir().unwrap();
    let manifest = fixture("patched.toml");
    let platform = resolve_platform(&manifest.platform).unwrap();
    let plan = resolver::resolve(&manifest, &platform).unwrap();

    codegen::generate_all(&manifest, &plan, &platform, dir.path()).unwrap();

    // Should have patch Dockerfiles.
    let loc_df = dir
        .path()
        .join(".aw-kit/build/localization-mapping.patch.Dockerfile");
    assert!(loc_df.exists());
    let content = std::fs::read_to_string(&loc_df).unwrap();
    assert!(content.contains("colcon build --packages-select"));

    let sp_df = dir
        .path()
        .join(".aw-kit/build/sensing-perception.patch.Dockerfile");
    assert!(sp_df.exists());
}

#[test]
fn generate_all_custom_pkg() {
    let dir = tempfile::tempdir().unwrap();
    let manifest = fixture("custom-pkg.toml");
    let platform = resolve_platform(&manifest.platform).unwrap();
    let plan = resolver::resolve(&manifest, &platform).unwrap();

    codegen::generate_all(&manifest, &plan, &platform, dir.path()).unwrap();

    // Extension Dockerfiles.
    let pc_df = dir
        .path()
        .join(".aw-kit/build/planning-control.extended.Dockerfile");
    assert!(pc_df.exists());

    let sp_df = dir
        .path()
        .join(".aw-kit/build/sensing-perception.extended.Dockerfile");
    assert!(sp_df.exists());

    // Devcontainer files.
    let dc1 = dir
        .path()
        .join(".devcontainer/autosdv_behavioral_planner.devcontainer.json");
    assert!(dc1.exists());
    let content = std::fs::read_to_string(&dc1).unwrap();
    assert!(content.contains("autosdv_behavioral_planner"));

    let dc2 = dir
        .path()
        .join(".devcontainer/autosdv_v2x_bridge.devcontainer.json");
    assert!(dc2.exists());
}

#[test]
fn generate_all_orin() {
    let dir = tempfile::tempdir().unwrap();
    let manifest = fixture("orin.toml");
    let platform = resolve_platform(&manifest.platform).unwrap();
    let plan = resolver::resolve(&manifest, &platform).unwrap();

    codegen::generate_all(&manifest, &plan, &platform, dir.path()).unwrap();

    let compose = dir.path().join(".aw-kit/compose/docker-compose.yml");
    let content = std::fs::read_to_string(&compose).unwrap();
    assert!(content.contains("runtime: nvidia"));
    assert!(content.contains("/dev/nvhost-ctrl"));
}
