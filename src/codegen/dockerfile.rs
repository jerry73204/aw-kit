use crate::{
    platform::ResolvedPlatform,
    resolver::{BuildStep, LayerType, SourceFetch},
};

use super::GENERATED_HEADER;

/// Generate a Dockerfile for a `BuildOverlay` step.
///
/// Returns empty string for non-overlay steps.
pub fn generate(step: &BuildStep, _platform: &ResolvedPlatform) -> String {
    let BuildStep::BuildOverlay {
        base_image,
        layer_type,
        sources,
        ..
    } = step
    else {
        return String::new();
    };

    match layer_type {
        LayerType::Patch => generate_patch(base_image, sources),
        LayerType::Extension => generate_extension(base_image, sources),
    }
}

/// Patch overlay: rebuild specific packages on top of the component image.
fn generate_patch(base_image: &str, sources: &[SourceFetch]) -> String {
    let mut lines = Vec::new();
    lines.push(GENERATED_HEADER.to_string());
    lines.push(format!("FROM {base_image}"));
    lines.push(String::new());
    lines.push("SHELL [\"/bin/bash\", \"-c\"]".to_string());
    lines.push(String::new());

    // Create overlay workspace.
    lines.push("RUN mkdir -p /opt/overlay_ws/src".to_string());
    lines.push("WORKDIR /opt/overlay_ws".to_string());
    lines.push(String::new());

    // Copy patch sources.
    let mut pkg_names = Vec::new();
    for source in sources {
        let (src_path, name) = source_copy_args(source);
        lines.push(format!("COPY {src_path} src/{name}"));
        pkg_names.push(name);
    }
    lines.push(String::new());

    // Build.
    let select = pkg_names.join(" ");
    lines.push(format!(
        "RUN . /opt/autoware/install/setup.bash && \\\n    colcon build --packages-select {select}"
    ));
    lines.push(String::new());

    // Entrypoint that sources the overlay.
    lines.push(entrypoint());
    lines.push(String::new());

    lines.join("\n")
}

/// Extension overlay: add custom user packages on top of the (possibly patched) image.
fn generate_extension(base_image: &str, sources: &[SourceFetch]) -> String {
    let mut lines = Vec::new();
    lines.push(GENERATED_HEADER.to_string());
    lines.push(format!("FROM {base_image}"));
    lines.push(String::new());
    lines.push("SHELL [\"/bin/bash\", \"-c\"]".to_string());
    lines.push(String::new());

    // Create overlay workspace.
    lines.push("RUN mkdir -p /opt/overlay_ws/src".to_string());
    lines.push("WORKDIR /opt/overlay_ws".to_string());
    lines.push(String::new());

    // Copy package sources.
    let mut pkg_names = Vec::new();
    for source in sources {
        let (src_path, name) = source_copy_args(source);
        lines.push(format!("COPY {src_path} src/{name}"));
        pkg_names.push(name);
    }
    lines.push(String::new());

    // Build.
    let select = pkg_names.join(" ");
    lines.push(format!(
        "RUN . /opt/autoware/install/setup.bash && \\\n    colcon build --packages-select {select}"
    ));
    lines.push(String::new());

    // Entrypoint that sources the overlay.
    lines.push(entrypoint());
    lines.push(String::new());

    lines.join("\n")
}

/// Extract COPY source path and package name from a SourceFetch.
fn source_copy_args(source: &SourceFetch) -> (String, String) {
    match source {
        SourceFetch::GitClone { dest, .. } => {
            let name = dest
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "pkg".to_string());
            (dest.to_string_lossy().to_string(), name)
        }
        SourceFetch::LocalPath { path } => {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "pkg".to_string());
            (path.to_string_lossy().to_string(), name)
        }
    }
}

fn entrypoint() -> String {
    r#"CMD ["/bin/bash", "-c", "source /opt/autoware/install/setup.bash && source /opt/overlay_ws/install/setup.bash && exec bash"]"#.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::{Arch, DockerRuntime};
    use std::path::PathBuf;

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

    #[test]
    fn patch_dockerfile_content() {
        let step = BuildStep::BuildOverlay {
            component: "localization-mapping".to_string(),
            base_image: "ghcr.io/autowarefoundation/openadkit:localization-mapping".to_string(),
            dockerfile: PathBuf::from(".aw-kit/build/localization-mapping.patch.Dockerfile"),
            context: PathBuf::from("."),
            tag: "localization-mapping-0.45.1-p12345678".to_string(),
            layer_type: LayerType::Patch,
            sources: vec![SourceFetch::GitClone {
                url: "https://github.com/autosdv/ndt_fix.git".to_string(),
                refspec: Some("fix".to_string()),
                dest: PathBuf::from(".aw-kit/src/ndt_scan_matcher"),
            }],
        };

        let content = generate(&step, &desktop_platform());
        assert!(content.starts_with(GENERATED_HEADER));
        assert!(content.contains("FROM ghcr.io/autowarefoundation/openadkit:localization-mapping"));
        assert!(content.contains("COPY .aw-kit/src/ndt_scan_matcher src/ndt_scan_matcher"));
        assert!(content.contains("colcon build --packages-select ndt_scan_matcher"));
        assert!(content.contains("setup.bash"));
    }

    #[test]
    fn extension_dockerfile_content() {
        let step = BuildStep::BuildOverlay {
            component: "planning-control".to_string(),
            base_image: "ghcr.io/autowarefoundation/openadkit:planning-control".to_string(),
            dockerfile: PathBuf::from(".aw-kit/build/planning-control.extended.Dockerfile"),
            context: PathBuf::from("."),
            tag: "planning-control-0.45.1-x87654321".to_string(),
            layer_type: LayerType::Extension,
            sources: vec![SourceFetch::LocalPath {
                path: PathBuf::from("./src/my_planner"),
            }],
        };

        let content = generate(&step, &desktop_platform());
        assert!(content.starts_with(GENERATED_HEADER));
        assert!(content.contains("FROM ghcr.io/autowarefoundation/openadkit:planning-control"));
        assert!(content.contains("COPY ./src/my_planner src/my_planner"));
        assert!(content.contains("colcon build --packages-select my_planner"));
    }

    #[test]
    fn multiple_sources_in_single_overlay() {
        let step = BuildStep::BuildOverlay {
            component: "sensing-perception".to_string(),
            base_image: "ghcr.io/autowarefoundation/openadkit:sensing-perception".to_string(),
            dockerfile: PathBuf::from(".aw-kit/build/sensing-perception.patch.Dockerfile"),
            context: PathBuf::from("."),
            tag: "sensing-perception-0.45.1-p99999999".to_string(),
            layer_type: LayerType::Patch,
            sources: vec![
                SourceFetch::GitClone {
                    url: "https://github.com/example/pkg_a.git".to_string(),
                    refspec: None,
                    dest: PathBuf::from(".aw-kit/src/pkg_a"),
                },
                SourceFetch::LocalPath {
                    path: PathBuf::from("./patches/pkg_b"),
                },
            ],
        };

        let content = generate(&step, &desktop_platform());
        assert!(content.contains("COPY .aw-kit/src/pkg_a src/pkg_a"));
        assert!(content.contains("COPY ./patches/pkg_b src/pkg_b"));
        assert!(content.contains("colcon build --packages-select pkg_a pkg_b"));
    }

    #[test]
    fn pull_step_returns_empty() {
        let step = BuildStep::Pull {
            component: "api".to_string(),
            image: "ghcr.io/autowarefoundation/openadkit:api".to_string(),
        };
        assert!(generate(&step, &desktop_platform()).is_empty());
    }
}
