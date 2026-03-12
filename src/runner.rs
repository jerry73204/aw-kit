use std::{
    path::Path,
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use tracing::info;

use crate::error::{Error, Result};

/// Path to the generated compose file relative to the project root.
const COMPOSE_FILE: &str = ".aw-kit/compose/docker-compose.yml";

/// Start all services via `docker compose up`.
///
/// In foreground mode (no `--detach`), Ctrl-C pauses the containers
/// instead of stopping them. Use `aw-kit stop` to fully stop.
pub fn run(project_root: &Path, detach: bool) -> Result<()> {
    let compose_path = project_root.join(COMPOSE_FILE);
    ensure_compose_exists(&compose_path)?;
    let compose_str = compose_path.to_string_lossy().to_string();

    if detach {
        info!("starting services (detached)");
        exec_docker(&["compose", "-f", &compose_str, "up", "-d"])
    } else {
        run_foreground(&compose_str)
    }
}

/// Foreground run: start containers detached, follow logs, pause on Ctrl-C.
fn run_foreground(compose_file: &str) -> Result<()> {
    // Start containers in the background so they survive Ctrl-C.
    info!("starting services");
    exec_docker(&["compose", "-f", compose_file, "up", "-d"])?;

    // Set up Ctrl-C handler to pause containers.
    let interrupted = Arc::new(AtomicBool::new(false));
    let interrupted_clone = Arc::clone(&interrupted);
    let compose_owned = compose_file.to_string();

    ctrlc::set_handler(move || {
        interrupted_clone.store(true, Ordering::SeqCst);
        // Pause containers.
        let _ = Command::new("docker")
            .args(["compose", "-f", &compose_owned, "pause"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::inherit())
            .status();
        eprintln!();
        eprintln!("containers paused. Use `aw-kit run` to resume or `aw-kit stop` to shut down.");
    })
    .map_err(|e| Error::Build(format!("failed to set signal handler: {e}")))?;

    // Follow logs until interrupted.
    let mut child = Command::new("docker")
        .args(["compose", "-f", compose_file, "logs", "-f"])
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .spawn()
        .map_err(|source| Error::Io {
            path: "docker".into(),
            source,
        })?;

    let _ = child.wait();

    if interrupted.load(Ordering::SeqCst) {
        // Already printed the pause message in the handler.
        Ok(())
    } else {
        // Logs ended naturally (containers exited).
        Ok(())
    }
}

/// Resume paused services and follow logs again.
pub fn resume(project_root: &Path) -> Result<()> {
    let compose_path = project_root.join(COMPOSE_FILE);
    ensure_compose_exists(&compose_path)?;
    let compose_str = compose_path.to_string_lossy().to_string();

    info!("resuming paused services");
    exec_docker(&["compose", "-f", &compose_str, "unpause"])
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

    #[test]
    fn resume_missing_compose_errors() {
        let dir = tempfile::tempdir().unwrap();
        let err = resume(dir.path()).unwrap_err();
        assert!(err.to_string().contains("compose file not found"));
    }
}
