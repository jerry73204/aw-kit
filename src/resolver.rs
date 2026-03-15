use std::{collections::BTreeMap, fmt, path::PathBuf};

use crate::{
    error::Result,
    images,
    manifest::{ManifestConfig, PatchSource},
    platform::{ResolvedPlatform, has_cuda_variant},
};

// ---------------------------------------------------------------------------
// Build plan types
// ---------------------------------------------------------------------------

/// A layer type in the image stacking order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerType {
    Patch,
    Extension,
}

impl fmt::Display for LayerType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
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
                    eprintln!("  pull  {component:<24} {image}");
                }
                BuildStep::BuildOverlay {
                    component,
                    layer_type,
                    tag,
                    ..
                } => {
                    eprintln!("  build {component:<24} [{layer_type}] -> {tag}");
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

/// Resolve the upstream image tag for a component.
///
/// Format: `<upstream_image>:<component>[-cuda]`
fn upstream_tag(upstream_image: &str, component: &str, use_cuda: bool) -> String {
    if use_cuda && has_cuda_variant(component) {
        format!("{upstream_image}:{component}-cuda")
    } else {
        format!("{upstream_image}:{component}")
    }
}

/// Resolve a manifest + platform into a concrete build plan.
pub fn resolve(manifest: &ManifestConfig, platform: &ResolvedPlatform) -> Result<BuildPlan> {
    let images = images::load();
    let upstream_image = &images.upstream.image;
    let version = &manifest.workspace.autoware;

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
        let has_extensions = extensions.contains_key(component);

        // 1. Always pull the upstream image.
        let pull_tag = upstream_tag(upstream_image, component, platform.use_cuda);
        steps.push(BuildStep::Pull {
            component: component.to_string(),
            image: pull_tag.clone(),
        });

        let mut current_base = pull_tag;

        // 2. Patch overlay.
        if has_patches {
            let patches = &manifest.patch[component];
            let sources = patch_sources(patches);
            let patch_hash = short_hash(patches);
            let tag = format!("{upstream_image}:{component}-{version}-p{patch_hash}");
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

        // 3. Extension (custom packages).
        if has_extensions {
            let pkg_names = &extensions[component];
            let ext_hash = short_hash_strs(pkg_names);
            let tag = format!("{upstream_image}:{component}-{version}-x{ext_hash}");
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
            planning-control = true
            vehicle-system   = true
            "#,
        );

        let plan = resolve(&m, &p).unwrap();
        assert_eq!(plan.steps.len(), 2);

        for step in &plan.steps {
            assert!(matches!(step, BuildStep::Pull { .. }));
        }

        let pc = plan.steps_for("planning-control");
        assert_eq!(pc.len(), 1);
        assert_eq!(
            pc[0].output_image(),
            "ghcr.io/autowarefoundation/openadkit:planning-control"
        );
    }

    // -- CUDA variant selection ---------------------------------------------

    #[test]
    fn cuda_pulls_cuda_variant_for_eligible_component() {
        let (m, p) = parse_and_resolve(
            r#"
            [workspace]
            autoware = "0.45.1"

            [platform]
            arch = "amd64"
            cuda = true

            [components]
            sensing-perception = true
            planning-control   = true
            "#,
        );

        let plan = resolve(&m, &p).unwrap();

        // sensing-perception has a CUDA variant — should pull -cuda
        let sp = plan.steps_for("sensing-perception");
        assert_eq!(
            sp[0].output_image(),
            "ghcr.io/autowarefoundation/openadkit:sensing-perception-cuda"
        );

        // planning-control has no CUDA variant — plain image
        let pc = plan.steps_for("planning-control");
        assert_eq!(
            pc[0].output_image(),
            "ghcr.io/autowarefoundation/openadkit:planning-control"
        );
    }

    #[test]
    fn no_cuda_pulls_plain_image() {
        let (m, p) = parse_and_resolve(
            r#"
            [workspace]
            autoware = "0.45.1"

            [platform]
            arch = "amd64"

            [components]
            sensing-perception = true
            "#,
        );

        let plan = resolve(&m, &p).unwrap();
        let sp = plan.steps_for("sensing-perception");
        assert_eq!(
            sp[0].output_image(),
            "ghcr.io/autowarefoundation/openadkit:sensing-perception"
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
            localization-mapping = true
            planning-control     = true

            [patch.localization-mapping]
            ndt_scan_matcher = { git = "https://github.com/autosdv/ndt_fix.git", branch = "fix" }
            "#,
        );

        let plan = resolve(&m, &p).unwrap();

        // planning-control: pull only
        let pc = plan.steps_for("planning-control");
        assert_eq!(pc.len(), 1);
        assert!(matches!(pc[0], BuildStep::Pull { .. }));

        // localization-mapping: pull + patch
        let loc = plan.steps_for("localization-mapping");
        assert_eq!(loc.len(), 2);
        assert!(matches!(loc[0], BuildStep::Pull { .. }));
        assert!(matches!(
            loc[1],
            BuildStep::BuildOverlay {
                layer_type: LayerType::Patch,
                ..
            }
        ));

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

    // -- Scenario C: Orin with CUDA variant ---------------------------------

    #[test]
    fn orin_pulls_cuda_variant() {
        let (m, p) = parse_and_resolve(
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
        );

        let plan = resolve(&m, &p).unwrap();

        // sensing-perception: CUDA variant pulled (Jetson auto-enables CUDA)
        let sp = plan.steps_for("sensing-perception");
        assert_eq!(sp.len(), 1);
        assert!(sp[0].output_image().ends_with("-cuda"));

        // planning-control: no CUDA variant, plain pull
        let pc = plan.steps_for("planning-control");
        assert_eq!(pc.len(), 1);
        assert!(!pc[0].output_image().contains("cuda"));
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
            planning-control = true

            [[package]]
            name    = "my_planner"
            path    = "./src/my_planner"
            extends = "planning-control"
            "#,
        );

        let plan = resolve(&m, &p).unwrap();
        let steps = plan.steps_for("planning-control");
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

    // -- Full stack: patch + extension on same component ---------------------

    #[test]
    fn full_stack_three_layers() {
        let (m, p) = parse_and_resolve(
            r#"
            [workspace]
            autoware = "0.45.1"

            [platform]
            arch = "amd64"

            [components]
            localization-mapping = true

            [patch.localization-mapping]
            ndt_scan_matcher = { git = "https://github.com/autosdv/ndt_fix.git", branch = "fix" }

            [[package]]
            name    = "my_loc_ext"
            path    = "./src/my_loc_ext"
            extends = "localization-mapping"
            "#,
        );

        let plan = resolve(&m, &p).unwrap();
        let steps = plan.steps_for("localization-mapping");

        // 3 steps: pull, patch, extension
        assert_eq!(steps.len(), 3);
        assert!(matches!(steps[0], BuildStep::Pull { .. }));
        assert!(matches!(
            steps[1],
            BuildStep::BuildOverlay {
                layer_type: LayerType::Patch,
                ..
            }
        ));
        assert!(matches!(
            steps[2],
            BuildStep::BuildOverlay {
                layer_type: LayerType::Extension,
                ..
            }
        ));

        // Verify chaining.
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
            localization-mapping = true
            sensing-perception   = true

            [patch.localization-mapping]
            ndt_scan_matcher = { path = "./patches/ndt" }

            [patch.sensing-perception]
            lidar_centerpoint = { path = "./patches/lidar" }
            "#,
        );

        let plan = resolve(&m, &p).unwrap();
        assert_eq!(plan.steps.len(), 4);

        let loc = plan.steps_for("localization-mapping");
        assert_eq!(loc.len(), 2);

        let sp = plan.steps_for("sensing-perception");
        assert_eq!(sp.len(), 2);

        assert_ne!(loc[1].output_image(), sp[1].output_image());
    }

    // -- Image tag format ---------------------------------------------------

    #[test]
    fn overlay_tags_include_version_and_hash() {
        let (m, p) = parse_and_resolve(
            r#"
            [workspace]
            autoware = "0.45.1"

            [platform]
            arch = "amd64"

            [components]
            localization-mapping = true

            [patch.localization-mapping]
            ndt_scan_matcher = { path = "./patches/ndt" }
            "#,
        );

        let plan = resolve(&m, &p).unwrap();
        let steps = plan.steps_for("localization-mapping");

        // Pull tag: plain component name
        assert_eq!(
            steps[0].output_image(),
            "ghcr.io/autowarefoundation/openadkit:localization-mapping"
        );

        // Patch tag: component-version-p<hash>
        let patch_tag = steps[1].output_image();
        assert!(
            patch_tag.contains("localization-mapping-0.45.1-p"),
            "{patch_tag}"
        );
    }

    // -- Dockerfile paths ---------------------------------------------------

    #[test]
    fn dockerfile_paths_follow_convention() {
        let (m, p) = parse_and_resolve(
            r#"
            [workspace]
            autoware = "0.45.1"

            [platform]
            arch = "amd64"

            [components]
            localization-mapping = true

            [patch.localization-mapping]
            ndt = { path = "./ndt" }

            [[package]]
            name    = "my_ext"
            path    = "./src/my_ext"
            extends = "localization-mapping"
            "#,
        );

        let plan = resolve(&m, &p).unwrap();
        let steps = plan.steps_for("localization-mapping");

        // steps: pull, patch, extension
        if let BuildStep::BuildOverlay { dockerfile, .. } = steps[1] {
            assert_eq!(
                dockerfile,
                &PathBuf::from(".aw-kit/build/localization-mapping.patch.Dockerfile")
            );
        }
        if let BuildStep::BuildOverlay { dockerfile, .. } = steps[2] {
            assert_eq!(
                dockerfile,
                &PathBuf::from(".aw-kit/build/localization-mapping.extended.Dockerfile")
            );
        }
    }
}
