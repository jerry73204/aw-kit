use std::{collections::BTreeMap, fmt, path::PathBuf};

use crate::{
    error::Result,
    manifest::{ManifestConfig, PatchSource},
    platform::{ResolvedPlatform, needs_cuda_rebuild},
};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const UPSTREAM_REGISTRY: &str = "ghcr.io/autowarefoundation/openadkit";

// ---------------------------------------------------------------------------
// Build plan types
// ---------------------------------------------------------------------------

/// A layer type in the image stacking order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerType {
    PlatformRebuild,
    Patch,
    Extension,
}

impl fmt::Display for LayerType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PlatformRebuild => write!(f, "platform-rebuild"),
            Self::Patch => write!(f, "patch"),
            Self::Extension => write!(f, "extension"),
        }
    }
}

/// A source that needs to be fetched before building.
#[derive(Debug, Clone)]
pub enum SourceFetch {
    GitClone {
        url: String,
        refspec: Option<String>,
        dest: PathBuf,
    },
    LocalPath {
        path: PathBuf,
    },
}

/// A single step in the build plan.
#[derive(Debug, Clone)]
pub enum BuildStep {
    Pull {
        component: String,
        image: String,
    },
    BuildOverlay {
        component: String,
        base_image: String,
        dockerfile: PathBuf,
        context: PathBuf,
        tag: String,
        layer_type: LayerType,
        sources: Vec<SourceFetch>,
    },
}

impl BuildStep {
    pub fn component(&self) -> &str {
        match self {
            Self::Pull { component, .. } | Self::BuildOverlay { component, .. } => component,
        }
    }

    /// The image reference produced by this step.
    pub fn output_image(&self) -> &str {
        match self {
            Self::Pull { image, .. } => image,
            Self::BuildOverlay { tag, .. } => tag,
        }
    }
}

/// The complete build plan for all enabled components.
#[derive(Debug)]
pub struct BuildPlan {
    pub steps: Vec<BuildStep>,
}

impl BuildPlan {
    /// Print a human-readable summary to stderr.
    pub fn print_summary(&self) {
        eprintln!("Build plan ({} steps):", self.steps.len());
        for step in &self.steps {
            match step {
                BuildStep::Pull { component, image } => {
                    eprintln!("  pull  {component:<16} {image}");
                }
                BuildStep::BuildOverlay {
                    component,
                    layer_type,
                    tag,
                    ..
                } => {
                    eprintln!("  build {component:<16} [{layer_type}] -> {tag}");
                }
            }
        }
    }

    /// Get all steps for a given component, in order.
    pub fn steps_for(&self, component: &str) -> Vec<&BuildStep> {
        self.steps
            .iter()
            .filter(|s| s.component() == component)
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Resolver
// ---------------------------------------------------------------------------

/// Resolve a manifest + platform into a concrete build plan.
pub fn resolve(manifest: &ManifestConfig, platform: &ResolvedPlatform) -> Result<BuildPlan> {
    let version = &manifest.workspace.autoware;
    let suffix = &platform.image_suffix;

    // Collect extensions per component.
    let mut extensions: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for pkg in &manifest.package {
        extensions
            .entry(pkg.extends.as_str())
            .or_default()
            .push(pkg.name.as_str());
    }

    let mut steps = Vec::new();

    for component in manifest.enabled_components() {
        let has_patches = manifest.patch.contains_key(component);
        let has_platform_rebuild = platform.device.is_some() && needs_cuda_rebuild(component);
        let has_extensions = extensions.contains_key(component);

        // 1. Always pull the upstream image.
        let upstream_image = format!("{UPSTREAM_REGISTRY}/{component}:{version}-{suffix}");
        steps.push(BuildStep::Pull {
            component: component.to_string(),
            image: upstream_image.clone(),
        });

        let mut current_base = upstream_image;

        // 2. Platform rebuild (if Jetson + CUDA component).
        if has_platform_rebuild {
            let tag = format!("{UPSTREAM_REGISTRY}/{component}:{version}-{suffix}-platform",);
            let dockerfile =
                PathBuf::from(format!(".aw-kit/build/{component}.platform.Dockerfile"));
            let context = PathBuf::from(".aw-kit/build");

            steps.push(BuildStep::BuildOverlay {
                component: component.to_string(),
                base_image: current_base,
                dockerfile,
                context,
                tag: tag.clone(),
                layer_type: LayerType::PlatformRebuild,
                sources: Vec::new(),
            });
            current_base = tag;
        }

        // 3. Patch overlay.
        if has_patches {
            let patches = &manifest.patch[component];
            let sources = patch_sources(patches);
            let patch_hash = short_hash(patches);
            let tag = format!("{UPSTREAM_REGISTRY}/{component}:{version}-p{patch_hash}-{suffix}",);
            let dockerfile = PathBuf::from(format!(".aw-kit/build/{component}.patch.Dockerfile"));
            let context = PathBuf::from(".");

            steps.push(BuildStep::BuildOverlay {
                component: component.to_string(),
                base_image: current_base,
                dockerfile,
                context,
                tag: tag.clone(),
                layer_type: LayerType::Patch,
                sources,
            });
            current_base = tag;
        }

        // 4. Extension (custom packages).
        if has_extensions {
            let pkg_names = &extensions[component];
            let ext_hash = short_hash_strs(pkg_names);
            let tag = format!("{UPSTREAM_REGISTRY}/{component}:{version}-x{ext_hash}-{suffix}",);
            let dockerfile =
                PathBuf::from(format!(".aw-kit/build/{component}.extended.Dockerfile"));
            let context = PathBuf::from(".");

            let sources = manifest
                .package
                .iter()
                .filter(|p| p.extends == component)
                .map(|p| SourceFetch::LocalPath {
                    path: p.path.clone(),
                })
                .collect();

            steps.push(BuildStep::BuildOverlay {
                component: component.to_string(),
                base_image: current_base,
                dockerfile,
                context,
                tag: tag.clone(),
                layer_type: LayerType::Extension,
                sources,
            });
        }
    }

    Ok(BuildPlan { steps })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn patch_sources(patches: &BTreeMap<String, PatchSource>) -> Vec<SourceFetch> {
    patches
        .iter()
        .map(|(name, source)| match source {
            PatchSource::Git { git, branch, tag } => {
                let refspec = branch.clone().or_else(|| tag.clone());
                SourceFetch::GitClone {
                    url: git.clone(),
                    refspec,
                    dest: PathBuf::from(format!(".aw-kit/src/{name}")),
                }
            }
            PatchSource::Path { path } => SourceFetch::LocalPath { path: path.clone() },
        })
        .collect()
}

/// Produce a short deterministic hash from patch entries (for image tagging).
fn short_hash<V: std::fmt::Debug>(map: &BTreeMap<String, V>) -> String {
    use std::hash::{DefaultHasher, Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    // BTreeMap iteration is sorted, so this is deterministic.
    for key in map.keys() {
        key.hash(&mut hasher);
    }
    let h = hasher.finish();
    format!("{h:08x}")
}

fn short_hash_strs(strs: &[&str]) -> String {
    use std::hash::{DefaultHasher, Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    for s in strs {
        s.hash(&mut hasher);
    }
    let h = hasher.finish();
    format!("{h:08x}")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        manifest::ManifestConfig,
        platform::{ResolvedPlatform, resolve_platform},
    };
    use std::path::PathBuf;

    fn parse_and_resolve(toml: &str) -> (ManifestConfig, ResolvedPlatform) {
        let manifest: ManifestConfig = toml::from_str(toml).unwrap();
        let platform = resolve_platform(&manifest.platform).unwrap();
        (manifest, platform)
    }

    // -- Scenario A: plain deployment (no customization) --------------------

    #[test]
    fn plain_deployment_pull_only() {
        let (m, p) = parse_and_resolve(
            r#"
            [workspace]
            autoware = "0.45.1"

            [platform]
            arch = "amd64"

            [components]
            planning = true
            control  = true
            "#,
        );

        let plan = resolve(&m, &p).unwrap();
        assert_eq!(plan.steps.len(), 2);

        // Both should be Pull steps.
        for step in &plan.steps {
            assert!(matches!(step, BuildStep::Pull { .. }));
        }

        let planning_steps = plan.steps_for("planning");
        assert_eq!(planning_steps.len(), 1);
        assert!(
            planning_steps[0]
                .output_image()
                .contains("planning:0.45.1-amd64")
        );
    }

    // -- Scenario B: patched package ----------------------------------------

    #[test]
    fn patched_component() {
        let (m, p) = parse_and_resolve(
            r#"
            [workspace]
            autoware = "0.45.1"

            [platform]
            arch = "amd64"

            [components]
            localization = true
            planning     = true

            [patch.localization]
            ndt_scan_matcher = { git = "https://github.com/autosdv/ndt_fix.git", branch = "fix" }
            "#,
        );

        let plan = resolve(&m, &p).unwrap();

        // planning: 1 pull
        let planning = plan.steps_for("planning");
        assert_eq!(planning.len(), 1);
        assert!(matches!(planning[0], BuildStep::Pull { .. }));

        // localization: 1 pull + 1 patch build
        let loc = plan.steps_for("localization");
        assert_eq!(loc.len(), 2);
        assert!(matches!(loc[0], BuildStep::Pull { .. }));
        assert!(matches!(
            loc[1],
            BuildStep::BuildOverlay {
                layer_type: LayerType::Patch,
                ..
            }
        ));

        // Patch build's base_image should be the pull image.
        if let BuildStep::BuildOverlay {
            base_image,
            sources,
            ..
        } = loc[1]
        {
            assert_eq!(base_image, loc[0].output_image());
            assert_eq!(sources.len(), 1);
            assert!(matches!(sources[0], SourceFetch::GitClone { .. }));
        }
    }

    // -- Scenario C: Orin platform rebuild ----------------------------------

    #[test]
    fn orin_cuda_component_gets_platform_rebuild() {
        let (m, p) = parse_and_resolve(
            r#"
            [workspace]
            autoware = "0.45.1"

            [platform]
            arch    = "arm64"
            device  = "jetson-agx-orin"
            jetpack = "6.1"

            [components]
            perception = true
            planning   = true
            "#,
        );

        let plan = resolve(&m, &p).unwrap();

        // perception is CUDA — gets pull + platform rebuild
        let perc = plan.steps_for("perception");
        assert_eq!(perc.len(), 2);
        assert!(matches!(perc[0], BuildStep::Pull { .. }));
        assert!(matches!(
            perc[1],
            BuildStep::BuildOverlay {
                layer_type: LayerType::PlatformRebuild,
                ..
            }
        ));

        // planning is not CUDA — pull only
        let plan_steps = plan.steps_for("planning");
        assert_eq!(plan_steps.len(), 1);
    }

    // -- Scenario D: custom package extension -------------------------------

    #[test]
    fn custom_package_extension() {
        let (m, p) = parse_and_resolve(
            r#"
            [workspace]
            autoware = "0.45.1"

            [platform]
            arch = "amd64"

            [components]
            planning = true

            [[package]]
            name    = "my_planner"
            path    = "./src/my_planner"
            extends = "planning"
            "#,
        );

        let plan = resolve(&m, &p).unwrap();
        let steps = plan.steps_for("planning");
        assert_eq!(steps.len(), 2);
        assert!(matches!(steps[0], BuildStep::Pull { .. }));
        assert!(matches!(
            steps[1],
            BuildStep::BuildOverlay {
                layer_type: LayerType::Extension,
                ..
            }
        ));

        if let BuildStep::BuildOverlay {
            base_image,
            sources,
            ..
        } = steps[1]
        {
            assert_eq!(base_image, steps[0].output_image());
            assert_eq!(sources.len(), 1);
            assert!(matches!(sources[0], SourceFetch::LocalPath { .. }));
        }
    }

    // -- Full stack: Orin + patch + extension --------------------------------

    #[test]
    fn full_stack_four_layers() {
        let (m, p) = parse_and_resolve(
            r#"
            [workspace]
            autoware = "0.45.1"

            [platform]
            arch    = "arm64"
            device  = "jetson-agx-orin"
            jetpack = "6.1"

            [components]
            localization = true

            [patch.localization]
            ndt_scan_matcher = { git = "https://github.com/autosdv/ndt_fix.git", branch = "fix" }

            [[package]]
            name    = "my_loc_ext"
            path    = "./src/my_loc_ext"
            extends = "localization"
            "#,
        );

        let plan = resolve(&m, &p).unwrap();
        let steps = plan.steps_for("localization");

        // 4 steps: pull, platform rebuild, patch, extension
        assert_eq!(steps.len(), 4);
        assert!(matches!(steps[0], BuildStep::Pull { .. }));
        assert!(matches!(
            steps[1],
            BuildStep::BuildOverlay {
                layer_type: LayerType::PlatformRebuild,
                ..
            }
        ));
        assert!(matches!(
            steps[2],
            BuildStep::BuildOverlay {
                layer_type: LayerType::Patch,
                ..
            }
        ));
        assert!(matches!(
            steps[3],
            BuildStep::BuildOverlay {
                layer_type: LayerType::Extension,
                ..
            }
        ));

        // Verify chaining: each step's base is the previous step's output.
        for i in 1..steps.len() {
            if let BuildStep::BuildOverlay { base_image, .. } = steps[i] {
                assert_eq!(
                    base_image,
                    steps[i - 1].output_image(),
                    "step {i} base_image should reference step {} output",
                    i - 1,
                );
            }
        }
    }

    // -- Two independent components -----------------------------------------

    #[test]
    fn independent_components_resolve_separately() {
        let (m, p) = parse_and_resolve(
            r#"
            [workspace]
            autoware = "0.45.1"

            [platform]
            arch = "amd64"

            [components]
            localization = true
            perception   = true

            [patch.localization]
            ndt_scan_matcher = { path = "./patches/ndt" }

            [patch.perception]
            lidar_centerpoint = { path = "./patches/lidar" }
            "#,
        );

        let plan = resolve(&m, &p).unwrap();

        // Each gets pull + patch = 2 steps, total 4.
        assert_eq!(plan.steps.len(), 4);

        let loc = plan.steps_for("localization");
        assert_eq!(loc.len(), 2);

        let perc = plan.steps_for("perception");
        assert_eq!(perc.len(), 2);

        // Their images should not reference each other.
        assert_ne!(loc[1].output_image(), perc[1].output_image());
    }

    // -- Image tag format ---------------------------------------------------

    #[test]
    fn image_tags_contain_version_and_suffix() {
        let (m, p) = parse_and_resolve(
            r#"
            [workspace]
            autoware = "0.45.1"

            [platform]
            arch    = "arm64"
            device  = "jetson-agx-orin"
            jetpack = "6.1"

            [components]
            perception = true

            [patch.perception]
            lidar_centerpoint = { path = "./patches/lidar" }
            "#,
        );

        let plan = resolve(&m, &p).unwrap();
        let steps = plan.steps_for("perception");

        // Pull image tag
        assert!(steps[0].output_image().contains("0.45.1"));
        assert!(steps[0].output_image().contains("agx-orin"));

        // Platform rebuild tag
        assert!(steps[1].output_image().contains("platform"));

        // Patch tag contains -p<hash>-
        let patch_tag = steps[2].output_image();
        assert!(
            patch_tag.contains("-p"),
            "tag should contain patch hash: {patch_tag}"
        );
        assert!(patch_tag.contains("agx-orin"));
    }

    // -- Dockerfile paths ---------------------------------------------------

    #[test]
    fn dockerfile_paths_follow_convention() {
        let (m, p) = parse_and_resolve(
            r#"
            [workspace]
            autoware = "0.45.1"

            [platform]
            arch    = "arm64"
            device  = "jetson-agx-orin"
            jetpack = "6.1"

            [components]
            localization = true

            [patch.localization]
            ndt = { path = "./ndt" }

            [[package]]
            name    = "my_ext"
            path    = "./src/my_ext"
            extends = "localization"
            "#,
        );

        let plan = resolve(&m, &p).unwrap();
        let steps = plan.steps_for("localization");

        // steps: pull, platform, patch, extension
        if let BuildStep::BuildOverlay { dockerfile, .. } = steps[1] {
            assert_eq!(
                dockerfile,
                &PathBuf::from(".aw-kit/build/localization.platform.Dockerfile")
            );
        }
        if let BuildStep::BuildOverlay { dockerfile, .. } = steps[2] {
            assert_eq!(
                dockerfile,
                &PathBuf::from(".aw-kit/build/localization.patch.Dockerfile")
            );
        }
        if let BuildStep::BuildOverlay { dockerfile, .. } = steps[3] {
            assert_eq!(
                dockerfile,
                &PathBuf::from(".aw-kit/build/localization.extended.Dockerfile")
            );
        }
    }
}
