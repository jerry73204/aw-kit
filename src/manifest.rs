use serde::Deserialize;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use crate::error::{Error, Result};

/// Known OpenADKit component names.
///
/// These match the actual image tags in `ghcr.io/autowarefoundation/openadkit:<component>`.
const KNOWN_COMPONENTS: &[&str] = &[
    "sensing-perception",
    "localization-mapping",
    "planning-control",
    "vehicle-system",
    "api",
    "simulator",
    "visualizer",
    "universe",
];

/// Top-level manifest parsed from `Autoware.toml`.
#[derive(Debug, Deserialize)]
pub struct ManifestConfig {
    pub workspace: Workspace,

    #[serde(default)]
    pub components: BTreeMap<String, bool>,

    pub platform: Platform,

    #[serde(default)]
    pub patch: BTreeMap<String, BTreeMap<String, PatchSource>>,

    /// Custom user packages (`[[package]]` array).
    #[serde(default)]
    pub package: Vec<Package>,

    pub registry: Option<Registry>,
}

#[derive(Debug, Deserialize)]
pub struct Workspace {
    /// Autoware version string, e.g. "0.45.1".
    pub autoware: String,
}

#[derive(Debug, Deserialize)]
pub struct Platform {
    pub arch: String,
    /// Use CUDA image variants where available (auto-enabled for Jetson devices).
    pub cuda: Option<bool>,
    pub device: Option<String>,
    pub jetpack: Option<String>,
}

/// A patch source — either a git remote or a local path.
///
/// Deserialized from TOML like:
///   `ndt_scan_matcher = { git = "https://...", branch = "fix" }`
///   `lidar_centerpoint = { path = "./patches/lidar_centerpoint" }`
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum PatchSource {
    Git {
        git: String,
        branch: Option<String>,
        tag: Option<String>,
    },
    Path {
        path: PathBuf,
    },
}

#[derive(Debug, Deserialize)]
pub struct Package {
    pub name: String,
    pub path: PathBuf,
    pub extends: String,
}

#[derive(Debug, Deserialize)]
pub struct Registry {
    pub url: String,
    pub prefix: String,
}

impl ManifestConfig {
    /// Read and parse an `Autoware.toml` file.
    pub fn from_file(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path).map_err(|source| Error::ManifestRead {
            path: path.to_path_buf(),
            source,
        })?;

        let manifest: Self = toml::from_str(&content).map_err(|source| Error::ManifestParse {
            path: path.to_path_buf(),
            source,
        })?;

        manifest.validate()?;
        Ok(manifest)
    }

    /// Validate cross-field constraints.
    fn validate(&self) -> Result<()> {
        // Every enabled component must be a known name.
        for name in self.components.keys() {
            if !KNOWN_COMPONENTS.contains(&name.as_str()) {
                return Err(Error::Validation(format!(
                    "unknown component '{name}'. Known components: {}",
                    KNOWN_COMPONENTS.join(", "),
                )));
            }
        }

        // Every patch must reference a declared (and enabled) component.
        for component in self.patch.keys() {
            match self.components.get(component) {
                Some(true) => {}
                Some(false) => {
                    return Err(Error::Validation(format!(
                        "patch references component '{component}' which is disabled",
                    )));
                }
                None => {
                    return Err(Error::Validation(format!(
                        "patch references component '{component}' which is not listed in [components]",
                    )));
                }
            }
        }

        // Every package.extends must reference a known component.
        for pkg in &self.package {
            if !KNOWN_COMPONENTS.contains(&pkg.extends.as_str()) {
                return Err(Error::Validation(format!(
                    "package '{}' extends unknown component '{}'",
                    pkg.name, pkg.extends,
                )));
            }
        }

        Ok(())
    }

    /// Components that are enabled (value = true).
    pub fn enabled_components(&self) -> Vec<&str> {
        self.components
            .iter()
            .filter_map(|(k, v)| if *v { Some(k.as_str()) } else { None })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(toml: &str) -> Result<ManifestConfig> {
        let manifest: ManifestConfig =
            toml::from_str(toml).map_err(|source| Error::ManifestParse {
                path: PathBuf::from("<inline>"),
                source,
            })?;
        manifest.validate()?;
        Ok(manifest)
    }

    #[test]
    fn minimal() {
        let m = parse(
            r#"
            [workspace]
            autoware = "0.45.1"

            [platform]
            arch = "amd64"

            [components]
            sensing-perception   = true
            localization-mapping = true
            planning-control     = true
            vehicle-system       = true
            api                  = true
            "#,
        )
        .unwrap();

        assert_eq!(m.workspace.autoware, "0.45.1");
        assert_eq!(m.enabled_components().len(), 5);
        assert_eq!(m.platform.arch, "amd64");
        assert!(m.platform.device.is_none());
        assert!(m.patch.is_empty());
        assert!(m.package.is_empty());
        assert!(m.registry.is_none());
    }

    #[test]
    fn patched() {
        let m = parse(
            r#"
            [workspace]
            autoware = "0.45.1"

            [platform]
            arch = "amd64"

            [components]
            localization-mapping = true
            sensing-perception   = true

            [patch.localization-mapping]
            ndt_scan_matcher = { git = "https://github.com/autosdv/ndt_fix.git", branch = "orin-mem-fix" }

            [patch.sensing-perception]
            lidar_centerpoint = { path = "./patches/lidar_centerpoint" }
            "#,
        )
        .unwrap();

        assert_eq!(m.patch.len(), 2);
        let loc_patches = &m.patch["localization-mapping"];
        assert!(matches!(
            loc_patches["ndt_scan_matcher"],
            PatchSource::Git { .. }
        ));
        let perc_patches = &m.patch["sensing-perception"];
        assert!(matches!(
            perc_patches["lidar_centerpoint"],
            PatchSource::Path { .. }
        ));
    }

    #[test]
    fn orin_platform() {
        let m = parse(
            r#"
            [workspace]
            autoware = "0.45.1"

            [platform]
            arch    = "arm64"
            device  = "jetson-agx-orin"
            jetpack = "6.1"

            [components]
            sensing-perception = true
            planning-control   = true
            "#,
        )
        .unwrap();

        assert_eq!(m.platform.arch, "arm64");
        assert_eq!(m.platform.device.as_deref(), Some("jetson-agx-orin"));
        assert_eq!(m.platform.jetpack.as_deref(), Some("6.1"));
    }

    #[test]
    fn custom_packages() {
        let m = parse(
            r#"
            [workspace]
            autoware = "0.45.1"

            [platform]
            arch = "amd64"

            [components]
            planning-control   = true
            sensing-perception = true

            [[package]]
            name    = "autosdv_behavioral_planner"
            path    = "./src/autosdv_behavioral_planner"
            extends = "planning-control"

            [[package]]
            name    = "autosdv_v2x_bridge"
            path    = "./src/autosdv_v2x_bridge"
            extends = "sensing-perception"
            "#,
        )
        .unwrap();

        assert_eq!(m.package.len(), 2);
        assert_eq!(m.package[0].name, "autosdv_behavioral_planner");
        assert_eq!(m.package[0].extends, "planning-control");
        assert_eq!(m.package[1].extends, "sensing-perception");
    }

    #[test]
    fn full_manifest() {
        let m = parse(
            r#"
            [workspace]
            autoware = "0.45.1"

            [platform]
            arch    = "arm64"
            cuda    = true
            device  = "jetson-agx-orin"
            jetpack = "6.1"

            [components]
            sensing-perception   = true
            localization-mapping = true
            planning-control     = true
            vehicle-system       = true
            api                  = true

            [patch.localization-mapping]
            ndt_scan_matcher = { git = "https://github.com/autosdv/ndt_fix.git", branch = "orin-fix" }

            [[package]]
            name    = "autosdv_behavioral_planner"
            path    = "./src/autosdv_behavioral_planner"
            extends = "planning-control"

            [registry]
            url    = "harbor.autosdv.edu.tw"
            prefix = "autosdv/openadkit"
            "#,
        )
        .unwrap();

        assert_eq!(m.enabled_components().len(), 5);
        assert_eq!(m.platform.device.as_deref(), Some("jetson-agx-orin"));
        assert_eq!(m.platform.cuda, Some(true));
        assert_eq!(m.patch.len(), 1);
        assert_eq!(m.package.len(), 1);
        assert!(m.registry.is_some());
        assert_eq!(m.registry.as_ref().unwrap().url, "harbor.autosdv.edu.tw");
    }

    #[test]
    fn unknown_component_rejected() {
        let err = parse(
            r#"
            [workspace]
            autoware = "0.45.1"

            [platform]
            arch = "amd64"

            [components]
            teleportation = true
            "#,
        )
        .unwrap_err();

        let msg = err.to_string();
        assert!(msg.contains("unknown component 'teleportation'"), "{msg}");
    }

    #[test]
    fn patch_undeclared_component_rejected() {
        let err = parse(
            r#"
            [workspace]
            autoware = "0.45.1"

            [platform]
            arch = "amd64"

            [components]
            planning-control = true

            [patch.localization-mapping]
            ndt_scan_matcher = { git = "https://example.com/ndt.git" }
            "#,
        )
        .unwrap_err();

        let msg = err.to_string();
        assert!(msg.contains("not listed in [components]"), "{msg}",);
    }

    #[test]
    fn patch_disabled_component_rejected() {
        let err = parse(
            r#"
            [workspace]
            autoware = "0.45.1"

            [platform]
            arch = "amd64"

            [components]
            localization-mapping = false

            [patch.localization-mapping]
            ndt_scan_matcher = { path = "./ndt" }
            "#,
        )
        .unwrap_err();

        let msg = err.to_string();
        assert!(msg.contains("disabled"), "{msg}");
    }

    #[test]
    fn package_extends_unknown_component_rejected() {
        let err = parse(
            r#"
            [workspace]
            autoware = "0.45.1"

            [platform]
            arch = "amd64"

            [components]
            planning-control = true

            [[package]]
            name    = "foo"
            path    = "./src/foo"
            extends = "teleportation"
            "#,
        )
        .unwrap_err();

        let msg = err.to_string();
        assert!(msg.contains("unknown component 'teleportation'"), "{msg}");
    }

    #[test]
    fn missing_workspace_rejected() {
        let err = parse(
            r#"
            [platform]
            arch = "amd64"

            [components]
            planning = true
            "#,
        )
        .unwrap_err();

        let msg = err.to_string();
        assert!(msg.contains("workspace"), "{msg}");
    }

    #[test]
    fn missing_platform_rejected() {
        let err = parse(
            r#"
            [workspace]
            autoware = "0.45.1"

            [components]
            planning = true
            "#,
        )
        .unwrap_err();

        let msg = err.to_string();
        assert!(msg.contains("platform"), "{msg}");
    }

    #[test]
    fn missing_platform_arch_rejected() {
        let err = parse(
            r#"
            [workspace]
            autoware = "0.45.1"

            [platform]
            device = "jetson-agx-orin"

            [components]
            planning = true
            "#,
        )
        .unwrap_err();

        let msg = err.to_string();
        assert!(msg.contains("arch"), "{msg}");
    }
}
