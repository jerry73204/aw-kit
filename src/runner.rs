use std::{path::Path, process::Command};

use tracing::info;

use crate::error::{Error, Result};

/// Path to the generated compose file relative to the project root.
const COMPOSE_FILE: &str = ".aw-kit/compose/docker-compose.yml";

/// Start all services via `docker compose up`.
pub fn run(project_root: &Path, detach: bool) -> Result<()> {
    let compose_path = project_root.join(COMPOSE_FILE);
    ensure_compose_exists(&compose_path)?;

    let compose_str = compose_path.to_string_lossy().to_string();
    let mut args: Vec<&str> = vec!["compose", "-f", &compose_str, "up"];
    if detach {
        args.push("-d");
    }

    info!("starting services");
    exec_docker(&args)
}

/// Stop all services via `docker compose down`.
pub fn stop(project_root: &Path) -> Result<()> {
    let compose_path = project_root.join(COMPOSE_FILE);
    ensure_compose_exists(&compose_path)?;

    let compose_str = compose_path.to_string_lossy().to_string();
    info!("stopping services");
    exec_docker(&["compose", "-f", &compose_str, "down"])
}

/// Show logs via `docker compose logs`.
pub fn logs(project_root: &Path, component: Option<&str>, follow: bool) -> Result<()> {
    let compose_path = project_root.join(COMPOSE_FILE);
    ensure_compose_exists(&compose_path)?;

    let compose_str = compose_path.to_string_lossy().to_string();
    let mut args: Vec<&str> = vec!["compose", "-f", &compose_str, "logs"];
    if follow {
        args.push("-f");
    }
    if let Some(service) = component {
        args.push(service);
    }

    exec_docker(&args)
}

fn ensure_compose_exists(compose_path: &Path) -> Result<()> {
    if !compose_path.exists() {
        return Err(Error::Build(format!(
            "compose file not found at {}. Run `aw-kit build` first.",
            compose_path.display()
        )));
    }
    Ok(())
}

fn exec_docker(args: &[&str]) -> Result<()> {
    let status = Command::new("docker")
        .args(args)
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .stdin(std::process::Stdio::inherit())
        .status()
        .map_err(|source| Error::Io {
            path: "docker".into(),
            source,
        })?;

    if !status.success() {
        return Err(Error::Build(format!(
            "docker {} failed with exit code {}",
            args.first().unwrap_or(&""),
            status.code().unwrap_or(-1)
        )));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_compose_file_errors() {
        let dir = tempfile::tempdir().unwrap();
        let err = run(dir.path(), false).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("compose file not found"), "{msg}");
        assert!(msg.contains("aw-kit build"), "{msg}");
    }

    #[test]
    fn stop_missing_compose_errors() {
        let dir = tempfile::tempdir().unwrap();
        let err = stop(dir.path()).unwrap_err();
        assert!(err.to_string().contains("compose file not found"));
    }

    #[test]
    fn logs_missing_compose_errors() {
        let dir = tempfile::tempdir().unwrap();
        let err = logs(dir.path(), None, false).unwrap_err();
        assert!(err.to_string().contains("compose file not found"));
    }
}
