use std::{collections::BTreeMap, path::Path, process::Command, time::Instant};

use tracing::{info, warn};

use crate::{
    codegen,
    error::{Error, Result},
    lockfile::{LockFile, LockWorkspace, LockedComponent, LockedPackage},
    manifest::ManifestConfig,
    platform::ResolvedPlatform,
    resolver::{BuildPlan, BuildStep, LayerType, SourceFetch},
};

// ---------------------------------------------------------------------------
// Build result
// ---------------------------------------------------------------------------

/// Result of a single build step with captured digest.
#[derive(Debug, Clone)]
pub struct StepResult {
    pub component: String,
    pub image: String,
    pub digest: String,
    pub layer_type: Option<LayerType>,
}

/// Aggregate result of executing the full build plan.
#[derive(Debug)]
pub struct BuildResult {
    pub step_results: Vec<StepResult>,
}

// ---------------------------------------------------------------------------
// Docker engine abstraction
// ---------------------------------------------------------------------------

/// Abstraction over Docker CLI commands for testability.
pub trait DockerEngine {
    /// Pull an image and return its digest.
    fn pull(&self, image: &str, platform: &str) -> Result<String>;

    /// Build a Dockerfile and return the built image's digest.
    fn build(&self, dockerfile: &Path, context: &Path, tag: &str, platform: &str)
    -> Result<String>;

    /// Inspect an image and return its digest.
    fn inspect_digest(&self, image: &str) -> Result<String>;
}

/// Default engine that shells out to `docker`.
pub struct ShellDockerEngine;

impl DockerEngine for ShellDockerEngine {
    fn pull(&self, image: &str, platform: &str) -> Result<String> {
        info!("pulling {image}");
        let output = Command::new("docker")
            .args(["pull", "--platform", platform, image])
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .output()
            .map_err(|source| Error::Io {
                path: "docker".into(),
                source,
            })?;

        if !output.status.success() {
            return Err(Error::Build(format!("docker pull failed for {image}")));
        }

        self.inspect_digest(image)
    }

    fn build(
        &self,
        dockerfile: &Path,
        context: &Path,
        tag: &str,
        platform: &str,
    ) -> Result<String> {
        info!("building {tag}");
        let output = Command::new("docker")
            .args([
                "buildx",
                "build",
                "-f",
                &dockerfile.to_string_lossy(),
                "-t",
                tag,
                "--platform",
                platform,
                "--load",
                &context.to_string_lossy(),
            ])
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .output()
            .map_err(|source| Error::Io {
                path: "docker".into(),
                source,
            })?;

        if !output.status.success() {
            return Err(Error::Build(format!(
                "docker buildx build failed for {tag} (Dockerfile: {})",
                dockerfile.display()
            )));
        }

        self.inspect_digest(tag)
    }

    fn inspect_digest(&self, image: &str) -> Result<String> {
        let output = Command::new("docker")
            .args(["inspect", "--format", "{{index .RepoDigests 0}}", image])
            .output()
            .map_err(|source| Error::Io {
                path: "docker".into(),
                source,
            })?;

        if output.status.success() {
            let full = String::from_utf8_lossy(&output.stdout).trim().to_string();
            // Extract digest from "image@sha256:..." format.
            if let Some(digest) = full.split('@').nth(1) {
                return Ok(digest.to_string());
            }
        }

        // Fallback: use image ID as digest.
        let output = Command::new("docker")
            .args(["inspect", "--format", "{{.Id}}", image])
            .output()
            .map_err(|source| Error::Io {
                path: "docker".into(),
                source,
            })?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            Err(Error::Build(format!(
                "failed to inspect digest for {image}"
            )))
        }
    }
}

// ---------------------------------------------------------------------------
// Source fetching
// ---------------------------------------------------------------------------

/// Fetch all sources required by a build step.
fn fetch_sources(sources: &[SourceFetch], project_root: &Path) -> Result<()> {
    for source in sources {
        match source {
            SourceFetch::GitClone { url, refspec, dest } => {
                let abs_dest = project_root.join(dest);
                info!("cloning {url} -> {}", abs_dest.display());

                if abs_dest.exists() {
                    info!("  already exists, skipping");
                    continue;
                }

                if let Some(parent) = abs_dest.parent() {
                    std::fs::create_dir_all(parent).map_err(|source| Error::Io {
                        path: parent.to_path_buf(),
                        source,
                    })?;
                }

                let dest_str = abs_dest.to_string_lossy().to_string();
                let mut cmd_args: Vec<&str> = vec!["clone", "--depth", "1"];
                if let Some(r) = refspec {
                    cmd_args.extend(["--branch", r.as_str()]);
                }
                cmd_args.push(url);
                cmd_args.push(&dest_str);

                let output = Command::new("git")
                    .args(&cmd_args)
                    .stdout(std::process::Stdio::inherit())
                    .stderr(std::process::Stdio::inherit())
                    .output()
                    .map_err(|source| Error::Io {
                        path: "git".into(),
                        source,
                    })?;

                if !output.status.success() {
                    return Err(Error::Build(format!(
                        "git clone failed for {url} -> {}",
                        abs_dest.display()
                    )));
                }
            }
            SourceFetch::LocalPath { path } => {
                let abs_path = project_root.join(path);
                if !abs_path.exists() {
                    return Err(Error::Build(format!(
                        "local source path does not exist: {}",
                        abs_path.display()
                    )));
                }
                info!("using local source: {}", abs_path.display());
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Build execution
// ---------------------------------------------------------------------------

/// Execute a build plan: fetch sources, generate Dockerfiles, pull/build images.
pub fn execute_plan(
    engine: &dyn DockerEngine,
    manifest: &ManifestConfig,
    plan: &BuildPlan,
    platform: &ResolvedPlatform,
    project_root: &Path,
) -> Result<BuildResult> {
    let start = Instant::now();
    let docker_platform = platform.arch.docker_platform();

    // 1. Fetch all sources.
    for step in &plan.steps {
        if let BuildStep::BuildOverlay { sources, .. } = step {
            fetch_sources(sources, project_root)?;
        }
    }

    // 2. Generate Dockerfiles and compose.
    codegen::generate_all(manifest, plan, platform, project_root)?;

    // 3. Execute each step in order.
    let mut step_results = Vec::new();
    let mut pulled = 0u32;
    let mut built = 0u32;

    for step in &plan.steps {
        match step {
            BuildStep::Pull { component, image } => {
                let digest = engine.pull(image, docker_platform)?;
                step_results.push(StepResult {
                    component: component.clone(),
                    image: image.clone(),
                    digest,
                    layer_type: None,
                });
                pulled += 1;
            }
            BuildStep::BuildOverlay {
                component,
                dockerfile,
                context,
                tag,
                layer_type,
                ..
            } => {
                let df_path = project_root.join(dockerfile);
                let ctx_path = project_root.join(context);
                let digest = engine.build(&df_path, &ctx_path, tag, docker_platform)?;
                step_results.push(StepResult {
                    component: component.clone(),
                    image: tag.clone(),
                    digest,
                    layer_type: Some(*layer_type),
                });
                built += 1;
            }
        }
    }

    let elapsed = start.elapsed();
    info!(
        "build complete: {pulled} pulled, {built} built in {:.1}s",
        elapsed.as_secs_f64()
    );

    Ok(BuildResult { step_results })
}

/// Build a `LockFile` from a `BuildResult`.
pub fn build_lock(manifest: &ManifestConfig, result: &BuildResult) -> LockFile {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // Group step results by component.
    let mut comp_map: BTreeMap<String, Vec<&StepResult>> = BTreeMap::new();
    for sr in &result.step_results {
        comp_map.entry(sr.component.clone()).or_default().push(sr);
    }

    let mut components = Vec::new();
    for (name, steps) in &comp_map {
        // Base pull is the first step with no layer_type.
        let base = steps.iter().find(|s| s.layer_type.is_none());
        let patch = steps
            .iter()
            .find(|s| s.layer_type == Some(LayerType::Patch));
        let extension = steps
            .iter()
            .find(|s| s.layer_type == Some(LayerType::Extension));

        if let Some(base) = base {
            components.push(LockedComponent {
                name: name.clone(),
                image: base.image.clone(),
                digest: base.digest.clone(),
                patched: patch.is_some(),
                patch_image: patch.map(|p| p.image.clone()),
                patch_digest: patch.map(|p| p.digest.clone()),
                extension_image: extension.map(|e| e.image.clone()),
                extension_digest: extension.map(|e| e.digest.clone()),
            });
        }
    }

    let packages = manifest
        .package
        .iter()
        .filter_map(|pkg| {
            // Find the extension step for this package's component.
            let ext = result.step_results.iter().find(|sr| {
                sr.component == pkg.extends && sr.layer_type == Some(LayerType::Extension)
            });
            ext.map(|e| LockedPackage {
                name: pkg.name.clone(),
                extends: pkg.extends.clone(),
                image: e.image.clone(),
                digest: e.digest.clone(),
            })
        })
        .collect();

    LockFile {
        workspace: LockWorkspace {
            autoware: manifest.workspace.autoware.clone(),
            generated: format!("{now}"),
        },
        components,
        packages,
    }
}

/// Verify that the current build plan matches the lock file.
pub fn verify_locked(
    engine: &dyn DockerEngine,
    lock: &LockFile,
    plan: &BuildPlan,
    platform: &ResolvedPlatform,
) -> Result<()> {
    let docker_platform = platform.arch.docker_platform();
    let mut current_digests = Vec::new();

    for step in &plan.steps {
        let image = step.output_image();
        match engine.inspect_digest(image) {
            Ok(digest) => {
                current_digests.push((image.to_string(), digest));
            }
            Err(_) => {
                // Image not found locally — try pulling for Pull steps.
                if let BuildStep::Pull { image, .. } = step {
                    match engine.pull(image, docker_platform) {
                        Ok(digest) => {
                            current_digests.push((image.clone(), digest));
                        }
                        Err(_) => {
                            warn!("image not available: {image}");
                        }
                    }
                } else {
                    return Err(Error::Build(format!(
                        "locked mode: image {image} not found locally — build first without --locked"
                    )));
                }
            }
        }
    }

    let mismatches = lock.verify(&current_digests);
    if mismatches.is_empty() {
        info!("lock file verification passed");
        Ok(())
    } else {
        let mut msg = String::from("lock file verification failed:\n");
        for m in &mismatches {
            msg.push_str(&format!(
                "  {}: expected {}, got {}\n",
                m.image, m.expected, m.actual
            ));
        }
        Err(Error::Build(msg))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        platform::{Arch, DockerRuntime},
        resolver::{BuildPlan, BuildStep, LayerType, SourceFetch},
    };
    use std::{cell::RefCell, path::PathBuf};

    /// Mock docker engine that records commands and returns fake digests.
    struct MockEngine {
        pulls: RefCell<Vec<String>>,
        builds: RefCell<Vec<String>>,
    }

    impl MockEngine {
        fn new() -> Self {
            Self {
                pulls: RefCell::new(Vec::new()),
                builds: RefCell::new(Vec::new()),
            }
        }
    }

    impl DockerEngine for MockEngine {
        fn pull(&self, image: &str, _platform: &str) -> Result<String> {
            self.pulls.borrow_mut().push(image.to_string());
            Ok(format!("sha256:pull-{}", image.len()))
        }

        fn build(
            &self,
            _dockerfile: &Path,
            _context: &Path,
            tag: &str,
            _platform: &str,
        ) -> Result<String> {
            self.builds.borrow_mut().push(tag.to_string());
            Ok(format!("sha256:build-{}", tag.len()))
        }

        fn inspect_digest(&self, image: &str) -> Result<String> {
            Ok(format!("sha256:inspect-{}", image.len()))
        }
    }

    fn desktop_platform() -> ResolvedPlatform {
        ResolvedPlatform {
            arch: Arch::Amd64,
            device: None,
            jetpack: None,
            cuda_arch: None,
            use_cuda: false,
            base_image: "ros:humble-ros-base-jammy".to_string(),
            runtime: DockerRuntime::Default,
            device_mounts: Vec::new(),
        }
    }

    fn minimal_manifest() -> ManifestConfig {
        toml::from_str(
            r#"
            [workspace]
            autoware = "0.45.1"

            [platform]
            arch = "amd64"

            [components]
            planning-control = true
            vehicle-system   = true
            "#,
        )
        .unwrap()
    }

    #[test]
    fn execute_plan_pulls_images() {
        let engine = MockEngine::new();
        let manifest = minimal_manifest();
        let platform = desktop_platform();
        let plan = BuildPlan {
            steps: vec![
                BuildStep::Pull {
                    component: "planning-control".to_string(),
                    image: "ghcr.io/autowarefoundation/openadkit:planning-control".to_string(),
                },
                BuildStep::Pull {
                    component: "vehicle-system".to_string(),
                    image: "ghcr.io/autowarefoundation/openadkit:vehicle-system".to_string(),
                },
            ],
        };

        let dir = tempfile::tempdir().unwrap();
        let result = execute_plan(&engine, &manifest, &plan, &platform, dir.path()).unwrap();

        assert_eq!(engine.pulls.borrow().len(), 2);
        assert_eq!(result.step_results.len(), 2);
        assert!(result.step_results[0].digest.starts_with("sha256:"));
    }

    #[test]
    fn execute_plan_builds_overlays() {
        let engine = MockEngine::new();
        let manifest: ManifestConfig = toml::from_str(
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
        )
        .unwrap();
        let platform = desktop_platform();
        let plan = BuildPlan {
            steps: vec![
                BuildStep::Pull {
                    component: "planning-control".to_string(),
                    image: "ghcr.io/autowarefoundation/openadkit:planning-control".to_string(),
                },
                BuildStep::BuildOverlay {
                    component: "planning-control".to_string(),
                    base_image: "ghcr.io/autowarefoundation/openadkit:planning-control".to_string(),
                    dockerfile: PathBuf::from(".aw-kit/build/planning-control.extended.Dockerfile"),
                    context: PathBuf::from("."),
                    tag: "planning-control-0.45.1-x12345678".to_string(),
                    layer_type: LayerType::Extension,
                    sources: vec![SourceFetch::LocalPath {
                        path: PathBuf::from("./src/my_planner"),
                    }],
                },
            ],
        };

        let dir = tempfile::tempdir().unwrap();
        // Create the fake local source path so fetch_sources doesn't fail.
        std::fs::create_dir_all(dir.path().join("src/my_planner")).unwrap();

        let result = execute_plan(&engine, &manifest, &plan, &platform, dir.path()).unwrap();

        assert_eq!(engine.pulls.borrow().len(), 1);
        assert_eq!(engine.builds.borrow().len(), 1);
        assert_eq!(result.step_results.len(), 2);
        assert!(
            result.step_results[1]
                .image
                .contains("planning-control-0.45.1-x")
        );
    }

    #[test]
    fn build_lock_from_result() {
        let manifest = minimal_manifest();
        let result = BuildResult {
            step_results: vec![
                StepResult {
                    component: "planning-control".to_string(),
                    image: "ghcr.io/autowarefoundation/openadkit:planning-control".to_string(),
                    digest: "sha256:aaa".to_string(),
                    layer_type: None,
                },
                StepResult {
                    component: "vehicle-system".to_string(),
                    image: "ghcr.io/autowarefoundation/openadkit:vehicle-system".to_string(),
                    digest: "sha256:bbb".to_string(),
                    layer_type: None,
                },
            ],
        };

        let lock = build_lock(&manifest, &result);
        assert_eq!(lock.workspace.autoware, "0.45.1");
        assert_eq!(lock.components.len(), 2);
        assert_eq!(lock.components[0].name, "planning-control");
        assert_eq!(lock.components[0].digest, "sha256:aaa");
        assert!(!lock.components[0].patched);
    }

    #[test]
    fn build_lock_with_patches() {
        let manifest: ManifestConfig = toml::from_str(
            r#"
            [workspace]
            autoware = "0.45.1"

            [platform]
            arch = "amd64"

            [components]
            localization-mapping = true
            "#,
        )
        .unwrap();

        let result = BuildResult {
            step_results: vec![
                StepResult {
                    component: "localization-mapping".to_string(),
                    image: "ghcr.io/autowarefoundation/openadkit:localization-mapping".to_string(),
                    digest: "sha256:base".to_string(),
                    layer_type: None,
                },
                StepResult {
                    component: "localization-mapping".to_string(),
                    image: "localization-mapping-0.45.1-p12345678".to_string(),
                    digest: "sha256:patched".to_string(),
                    layer_type: Some(LayerType::Patch),
                },
            ],
        };

        let lock = build_lock(&manifest, &result);
        assert_eq!(lock.components.len(), 1);
        assert!(lock.components[0].patched);
        assert_eq!(
            lock.components[0].patch_digest.as_deref(),
            Some("sha256:patched")
        );
    }

    #[test]
    fn verify_locked_passes_when_matching() {
        let engine = MockEngine::new();
        let platform = desktop_platform();
        let plan = BuildPlan {
            steps: vec![BuildStep::Pull {
                component: "api".to_string(),
                image: "ghcr.io/autowarefoundation/openadkit:api".to_string(),
            }],
        };

        // The mock engine's inspect_digest returns "sha256:inspect-<len>".
        let image = "ghcr.io/autowarefoundation/openadkit:api";
        let expected_digest = format!("sha256:inspect-{}", image.len());

        let lock = LockFile {
            workspace: LockWorkspace {
                autoware: "0.45.1".to_string(),
                generated: "0".to_string(),
            },
            components: vec![LockedComponent {
                name: "api".to_string(),
                image: image.to_string(),
                digest: expected_digest,
                patched: false,
                patch_image: None,
                patch_digest: None,
                extension_image: None,
                extension_digest: None,
            }],
            packages: vec![],
        };

        let result = verify_locked(&engine, &lock, &plan, &platform);
        assert!(result.is_ok());
    }
}
