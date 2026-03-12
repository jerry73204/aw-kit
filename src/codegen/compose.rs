use crate::{
    platform::{DockerRuntime, ResolvedPlatform},
    resolver::BuildPlan,
};

use super::GENERATED_HEADER;

/// Generate a `docker-compose.yml` from the build plan and platform.
pub fn generate(plan: &BuildPlan, platform: &ResolvedPlatform) -> String {
    let mut lines = Vec::new();
    lines.push(GENERATED_HEADER.to_string());
    lines.push(String::new());
    lines.push("services:".to_string());

    // Track which components we've already emitted (use final image per component).
    let mut seen = std::collections::BTreeMap::new();
    for step in &plan.steps {
        seen.insert(
            step.component().to_string(),
            step.output_image().to_string(),
        );
    }

    for (component, image) in &seen {
        lines.push(format!("  {component}:"));
        lines.push(format!("    image: {image}"));
        lines.push("    network_mode: host".to_string());

        if platform.runtime == DockerRuntime::Nvidia {
            lines.push("    runtime: nvidia".to_string());
        }

        if !platform.device_mounts.is_empty() {
            lines.push("    devices:".to_string());
            for mount in &platform.device_mounts {
                lines.push(format!("      - {mount}"));
            }
        }

        lines.push("    environment:".to_string());
        lines.push("      - ROS_DOMAIN_ID=${ROS_DOMAIN_ID:-0}".to_string());
    }

    lines.push(String::new());
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        platform::Arch,
        resolver::{BuildPlan, BuildStep},
    };

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

    fn orin_platform() -> ResolvedPlatform {
        ResolvedPlatform {
            arch: Arch::Arm64,
            device: Some("jetson-agx-orin".to_string()),
            jetpack: Some("6.1".to_string()),
            cuda_arch: Some(87),
            use_cuda: true,
            base_image: "nvcr.io/nvidia/l4t-cuda:12.6.68-devel".to_string(),
            runtime: DockerRuntime::Nvidia,
            device_mounts: vec![
                "/dev/nvhost-ctrl".to_string(),
                "/dev/nvhost-ctrl-gpu".to_string(),
                "/dev/nvmap".to_string(),
            ],
        }
    }

    #[test]
    fn desktop_compose_no_runtime() {
        let plan = BuildPlan {
            steps: vec![
                BuildStep::Pull {
                    component: "api".to_string(),
                    image: "ghcr.io/autowarefoundation/openadkit:api".to_string(),
                },
                BuildStep::Pull {
                    component: "planning-control".to_string(),
                    image: "ghcr.io/autowarefoundation/openadkit:planning-control".to_string(),
                },
            ],
        };

        let content = generate(&plan, &desktop_platform());
        assert!(content.starts_with(GENERATED_HEADER));
        assert!(content.contains("network_mode: host"));
        assert!(!content.contains("runtime: nvidia"));
        assert!(!content.contains("devices:"));
        assert!(content.contains("ROS_DOMAIN_ID"));
    }

    #[test]
    fn orin_compose_has_nvidia_runtime_and_devices() {
        let plan = BuildPlan {
            steps: vec![BuildStep::Pull {
                component: "sensing-perception".to_string(),
                image: "ghcr.io/autowarefoundation/openadkit:sensing-perception-cuda".to_string(),
            }],
        };

        let content = generate(&plan, &orin_platform());
        assert!(content.contains("runtime: nvidia"));
        assert!(content.contains("devices:"));
        assert!(content.contains("/dev/nvhost-ctrl"));
        assert!(content.contains("/dev/nvmap"));
    }

    #[test]
    fn compose_uses_final_image_for_component() {
        let plan = BuildPlan {
            steps: vec![
                BuildStep::Pull {
                    component: "localization-mapping".to_string(),
                    image: "ghcr.io/autowarefoundation/openadkit:localization-mapping".to_string(),
                },
                BuildStep::BuildOverlay {
                    component: "localization-mapping".to_string(),
                    base_image: "ghcr.io/autowarefoundation/openadkit:localization-mapping"
                        .to_string(),
                    dockerfile: ".aw-kit/build/localization-mapping.patch.Dockerfile".into(),
                    context: ".".into(),
                    tag: "localization-mapping-0.45.1-p12345678".to_string(),
                    layer_type: crate::resolver::LayerType::Patch,
                    sources: vec![],
                },
            ],
        };

        let content = generate(&plan, &desktop_platform());
        // Should use the final (patched) image, not the pull image.
        assert!(content.contains("localization-mapping-0.45.1-p12345678"));
        // Should not have the plain upstream as the service image.
        assert!(
            !content.contains("image: ghcr.io/autowarefoundation/openadkit:localization-mapping\n")
        );
    }
}
