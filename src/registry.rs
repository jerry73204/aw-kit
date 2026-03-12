use std::process::Command;

use tracing::{info, warn};

use crate::{
    builder::BuildResult,
    error::{Error, Result},
    manifest::Registry,
    resolver::{BuildPlan, BuildStep},
};

/// Compute the registry tag for a locally-built image.
///
/// Format: `<registry_url>/<prefix>/<component>:<local_tag_suffix>`
/// where `local_tag_suffix` is the part after `:` in the local tag,
/// or the component name for pull-only images.
pub fn registry_tag(registry: &Registry, image: &str) -> String {
    // Extract the tag part (after last ':').
    let tag_part = image.rsplit(':').next().unwrap_or(image);
    // Extract component name (first segment of the tag).
    let component = tag_part.split('-').next().unwrap_or(tag_part);
    format!(
        "{}/{}/{component}:{tag_part}",
        registry.url, registry.prefix
    )
}

/// Push all locally-built overlay images to the configured registry.
///
/// Only pushes images that were built locally (BuildOverlay steps),
/// not upstream images that were merely pulled.
pub fn push(registry: &Registry, plan: &BuildPlan, _result: &BuildResult) -> Result<()> {
    // Collect images from overlay build steps.
    let built_images: Vec<&str> = plan
        .steps
        .iter()
        .filter_map(|step| {
            if let BuildStep::BuildOverlay { tag, .. } = step {
                Some(tag.as_str())
            } else {
                None
            }
        })
        .collect();

    if built_images.is_empty() {
        info!("no locally-built images to push");
        return Ok(());
    }

    for local_image in &built_images {
        let remote_tag = registry_tag(registry, local_image);
        info!("tagging {local_image} -> {remote_tag}");

        let status = Command::new("docker")
            .args(["tag", local_image, &remote_tag])
            .status()
            .map_err(|source| Error::Io {
                path: "docker".into(),
                source,
            })?;

        if !status.success() {
            return Err(Error::Build(format!(
                "docker tag failed for {local_image} -> {remote_tag}"
            )));
        }

        info!("pushing {remote_tag}");
        let status = Command::new("docker")
            .args(["push", &remote_tag])
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .status()
            .map_err(|source| Error::Io {
                path: "docker".into(),
                source,
            })?;

        if !status.success() {
            return Err(Error::Build(format!("docker push failed for {remote_tag}")));
        }
    }

    info!(
        "pushed {} images to {}/{}",
        built_images.len(),
        registry.url,
        registry.prefix
    );
    Ok(())
}

/// Check if an image exists in the remote registry.
///
/// Returns the digest if found, None otherwise.
pub fn check_remote(image: &str) -> Option<String> {
    let output = Command::new("docker")
        .args(["buildx", "imagetools", "inspect", image, "--raw"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;

    if output.status.success() {
        // Image exists — extract digest from the manifest.
        // For simplicity, use docker inspect format.
        let output = Command::new("docker")
            .args(["buildx", "imagetools", "inspect", image])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .output()
            .ok()?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            // Look for "Digest:" line.
            for line in stdout.lines() {
                let trimmed = line.trim();
                if let Some(digest) = trimmed.strip_prefix("Digest:") {
                    return Some(digest.trim().to_string());
                }
            }
        }
        // Image exists but couldn't parse digest — still indicate it exists.
        Some("unknown".to_string())
    } else {
        None
    }
}

/// Try to pull pre-built images from the registry for overlay steps.
///
/// Returns the list of step indices that were satisfied from the registry
/// (and thus don't need local building).
pub fn pull_from_registry(registry: &Registry, plan: &BuildPlan) -> Vec<usize> {
    let mut pulled_indices = Vec::new();

    for (i, step) in plan.steps.iter().enumerate() {
        if let BuildStep::BuildOverlay { tag, .. } = step {
            let remote = registry_tag(registry, tag);
            if let Some(digest) = check_remote(&remote) {
                info!("found {remote} in registry (digest: {digest}), pulling");
                let status = Command::new("docker")
                    .args(["pull", &remote])
                    .stdout(std::process::Stdio::inherit())
                    .stderr(std::process::Stdio::inherit())
                    .status();

                if let Ok(s) = status
                    && s.success()
                {
                    // Re-tag as the local tag.
                    let _ = Command::new("docker").args(["tag", &remote, tag]).status();
                    pulled_indices.push(i);
                    continue;
                }
                warn!("failed to pull {remote}, will build locally");
            }
        }
    }

    pulled_indices
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn test_registry() -> Registry {
        Registry {
            url: "harbor.autosdv.edu.tw".to_string(),
            prefix: "autosdv/openadkit".to_string(),
        }
    }

    #[test]
    fn registry_tag_for_overlay() {
        let reg = test_registry();
        let tag = registry_tag(
            &reg,
            "ghcr.io/autowarefoundation/openadkit:localization-mapping-0.45.1-p12345678",
        );
        assert_eq!(
            tag,
            "harbor.autosdv.edu.tw/autosdv/openadkit/localization:localization-mapping-0.45.1-p12345678"
        );
    }

    #[test]
    fn registry_tag_for_plain_component() {
        let reg = test_registry();
        let tag = registry_tag(
            &reg,
            "ghcr.io/autowarefoundation/openadkit:planning-control",
        );
        assert_eq!(
            tag,
            "harbor.autosdv.edu.tw/autosdv/openadkit/planning:planning-control"
        );
    }
}
