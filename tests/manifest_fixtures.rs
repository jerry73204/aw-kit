use std::path::Path;

use aw_kit::manifest::ManifestConfig;

fn fixture(name: &str) -> ManifestConfig {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    ManifestConfig::from_file(&path).unwrap()
}

#[test]
fn minimal_fixture() {
    let m = fixture("minimal.toml");
    assert_eq!(m.workspace.autoware, "0.45.1");
    assert_eq!(m.enabled_components().len(), 6);
    assert!(m.platform.is_none());
    assert!(m.patch.is_empty());
    assert!(m.package.is_empty());
}

#[test]
fn patched_fixture() {
    let m = fixture("patched.toml");
    assert_eq!(m.patch.len(), 2);
    assert!(m.patch.contains_key("localization"));
    assert!(m.patch.contains_key("perception"));
    assert_eq!(m.patch["localization"].len(), 1);
}

#[test]
fn orin_fixture() {
    let m = fixture("orin.toml");
    let p = m.platform.as_ref().unwrap();
    assert_eq!(p.device.as_deref(), Some("jetson-agx-orin"));
    assert_eq!(p.jetpack.as_deref(), Some("6.1"));
    assert_eq!(p.arch.as_deref(), Some("arm64"));
}

#[test]
fn custom_pkg_fixture() {
    let m = fixture("custom-pkg.toml");
    assert_eq!(m.package.len(), 2);
    assert_eq!(m.package[0].name, "autosdv_behavioral_planner");
    assert_eq!(m.package[0].extends, "planning");
}

#[test]
fn full_fixture() {
    let m = fixture("full.toml");
    assert!(m.platform.is_some());
    assert_eq!(m.patch.len(), 1);
    assert_eq!(m.package.len(), 1);
    assert!(m.registry.is_some());
    assert_eq!(m.enabled_components().len(), 5);
}
