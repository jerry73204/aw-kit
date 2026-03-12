use crate::{
    manifest::Package,
    platform::{DockerRuntime, ResolvedPlatform},
};

use super::GENERATED_HEADER_JSON;

/// Generate a `devcontainer.json` for a custom package.
pub fn generate(pkg: &Package, image: &str, platform: &ResolvedPlatform) -> String {
    let mut json = String::new();
    json.push_str(&format!("{GENERATED_HEADER_JSON}\n"));
    json.push_str("{\n");
    json.push_str(&format!("  \"name\": \"{}\",\n", pkg.name));
    json.push_str(&format!("  \"image\": \"{image}\",\n"));

    // Workspace mount.
    json.push_str(&format!(
        "  \"workspaceMount\": \"source={{}},target=/opt/overlay_ws/src/{},type=bind\",\n",
        pkg.name
    ));
    json.push_str(&format!(
        "  \"workspaceFolder\": \"/opt/overlay_ws/src/{}\",\n",
        pkg.name
    ));

    // Run args.
    let mut run_args = vec!["\"--network=host\"".to_string()];
    if platform.runtime == DockerRuntime::Nvidia {
        run_args.push("\"--runtime=nvidia\"".to_string());
    }
    json.push_str(&format!("  \"runArgs\": [{}],\n", run_args.join(", ")));

    // Container env.
    json.push_str("  \"containerEnv\": {\n");
    json.push_str("    \"ROS_DOMAIN_ID\": \"0\"\n");
    json.push_str("  }\n");

    json.push_str("}\n");
    json
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::Arch;
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

    fn orin_platform() -> ResolvedPlatform {
        ResolvedPlatform {
            arch: Arch::Arm64,
            device: Some("jetson-agx-orin".to_string()),
            jetpack: Some("6.1".to_string()),
            cuda_arch: Some(87),
            use_cuda: true,
            base_image: "nvcr.io/nvidia/l4t-cuda:12.6.68-devel".to_string(),
            runtime: DockerRuntime::Nvidia,
            device_mounts: vec!["/dev/nvhost-ctrl".to_string()],
        }
    }

    #[test]
    fn devcontainer_desktop() {
        let pkg = Package {
            name: "my_planner".to_string(),
            path: PathBuf::from("./src/my_planner"),
            extends: "planning-control".to_string(),
        };

        let content = generate(
            &pkg,
            "ghcr.io/autowarefoundation/openadkit:planning-control",
            &desktop_platform(),
        );

        assert!(content.starts_with(GENERATED_HEADER_JSON));
        assert!(content.contains("\"name\": \"my_planner\""));
        assert!(
            content
                .contains("\"image\": \"ghcr.io/autowarefoundation/openadkit:planning-control\"")
        );
        assert!(content.contains("\"--network=host\""));
        assert!(!content.contains("--runtime=nvidia"));
        assert!(content.contains("ROS_DOMAIN_ID"));
    }

    #[test]
    fn devcontainer_orin_has_nvidia_runtime() {
        let pkg = Package {
            name: "my_detector".to_string(),
            path: PathBuf::from("./src/my_detector"),
            extends: "sensing-perception".to_string(),
        };

        let content = generate(
            &pkg,
            "ghcr.io/autowarefoundation/openadkit:sensing-perception-cuda",
            &orin_platform(),
        );

        assert!(content.contains("\"--runtime=nvidia\""));
    }
}
