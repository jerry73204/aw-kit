use std::{path::PathBuf, process::Command};

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
// Docker login detection
// ---------------------------------------------------------------------------

/// Path to the Docker config file.
fn docker_config_path() -> PathBuf {
    if let Ok(config_dir) = std::env::var("DOCKER_CONFIG") {
        PathBuf::from(config_dir).join("config.json")
    } else if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home).join(".docker/config.json")
    } else {
        PathBuf::from("~/.docker/config.json")
    }
}

/// Check if the user is authenticated to the given registry.
///
/// Inspects `~/.docker/config.json` (or `$DOCKER_CONFIG/config.json`)
/// for auth entries or credential helpers matching the registry URL.
pub fn check_login(registry_url: &str) -> LoginStatus {
    let config_path = docker_config_path();
    let content = match std::fs::read_to_string(&config_path) {
        Ok(c) => c,
        Err(_) => return LoginStatus::NoConfig,
    };

    let config: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(_) => return LoginStatus::NoConfig,
    };

    // Check "auths" section for direct credentials.
    if let Some(auths) = config.get("auths").and_then(|v| v.as_object()) {
        for key in auths.keys() {
            if key.contains(registry_url) {
                return LoginStatus::Authenticated;
            }
        }
    }

    // Check "credHelpers" for registry-specific credential helpers.
    if let Some(helpers) = config.get("credHelpers").and_then(|v| v.as_object()) {
        for key in helpers.keys() {
            if key.contains(registry_url) {
                return LoginStatus::Authenticated;
            }
        }
    }

    // Check "credsStore" — a global credential store handles all registries.
    if config.get("credsStore").and_then(|v| v.as_str()).is_some() {
        return LoginStatus::CredentialStore;
    }

    LoginStatus::NotAuthenticated
}

/// Result of checking Docker login status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoginStatus {
    /// Credentials found for the registry.
    Authenticated,
    /// A global credential store is configured (may or may not have this registry).
    CredentialStore,
    /// No credentials found for this registry.
    NotAuthenticated,
    /// Docker config file not found.
    NoConfig,
}

impl LoginStatus {
    /// Returns true if we're confident the user is authenticated.
    pub fn is_authenticated(&self) -> bool {
        matches!(self, Self::Authenticated)
    }

    /// Returns a warning message if there may be an auth issue.
    pub fn warning(&self, registry_url: &str) -> Option<String> {
        match self {
            Self::Authenticated => None,
            Self::CredentialStore => None, // Credential store likely handles it.
            Self::NotAuthenticated => Some(format!(
                "not authenticated to registry '{registry_url}'. Run `docker login {registry_url}` first."
            )),
            Self::NoConfig => Some(
                "Docker config file not found. Run `docker login` to configure credentials."
                    .to_string(),
            ),
        }
    }
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

    #[test]
    fn login_status_warning_for_not_authenticated() {
        let status = LoginStatus::NotAuthenticated;
        let msg = status.warning("harbor.autosdv.edu.tw").unwrap();
        assert!(msg.contains("docker login"));
        assert!(msg.contains("harbor.autosdv.edu.tw"));
    }

    #[test]
    fn login_status_no_warning_when_authenticated() {
        assert!(LoginStatus::Authenticated.warning("x").is_none());
        assert!(LoginStatus::CredentialStore.warning("x").is_none());
    }

    #[test]
    fn check_login_with_auths() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        std::fs::write(
            &config_path,
            r#"{"auths": {"harbor.autosdv.edu.tw": {"auth": "dGVzdDp0ZXN0"}}}"#,
        )
        .unwrap();

        // Point DOCKER_CONFIG to our temp dir.
        let prev = std::env::var("DOCKER_CONFIG").ok();
        // SAFETY: test is single-threaded for this env var usage.
        unsafe { std::env::set_var("DOCKER_CONFIG", dir.path()) };

        let status = check_login("harbor.autosdv.edu.tw");
        assert_eq!(status, LoginStatus::Authenticated);

        // Restore.
        unsafe {
            match prev {
                Some(v) => std::env::set_var("DOCKER_CONFIG", v),
                None => std::env::remove_var("DOCKER_CONFIG"),
            }
        }
    }

    #[test]
    fn check_login_not_authenticated() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        std::fs::write(&config_path, r#"{"auths": {}}"#).unwrap();

        let prev = std::env::var("DOCKER_CONFIG").ok();
        unsafe { std::env::set_var("DOCKER_CONFIG", dir.path()) };

        let status = check_login("harbor.autosdv.edu.tw");
        assert_eq!(status, LoginStatus::NotAuthenticated);

        unsafe {
            match prev {
                Some(v) => std::env::set_var("DOCKER_CONFIG", v),
                None => std::env::remove_var("DOCKER_CONFIG"),
            }
        }
    }

    #[test]
    fn check_login_with_cred_store() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        std::fs::write(&config_path, r#"{"credsStore": "desktop"}"#).unwrap();

        let prev = std::env::var("DOCKER_CONFIG").ok();
        unsafe { std::env::set_var("DOCKER_CONFIG", dir.path()) };

        let status = check_login("anything.example.com");
        assert_eq!(status, LoginStatus::CredentialStore);

        unsafe {
            match prev {
                Some(v) => std::env::set_var("DOCKER_CONFIG", v),
                None => std::env::remove_var("DOCKER_CONFIG"),
            }
        }
    }
}
