use clap::Parser;
use tracing::{info, warn};

use aw_kit::{
    builder::{self, ShellDockerEngine},
    cli::{Cli, Command},
    lockfile::LockFile,
    manifest::ManifestConfig,
    platform::resolve_platform,
    registry, resolver, runner,
};

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_target(false)
        .without_time()
        .init();

    let cli = Cli::parse();

    match cli.command {
        Command::Build {
            dry_run,
            pull,
            locked,
        } => {
            let manifest = load_manifest(&cli.manifest);
            let resolved = match resolve_platform(&manifest.platform) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("error: {e}");
                    std::process::exit(1);
                }
            };
            let plan = match resolver::resolve(&manifest, &resolved) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("error: {e}");
                    std::process::exit(1);
                }
            };

            if dry_run {
                print_manifest(&manifest);
                resolved.print_summary();
                eprintln!();
                plan.print_summary();
                return;
            }

            let project_root = manifest_dir(&cli.manifest);
            let engine = ShellDockerEngine;

            // Try pulling pre-built images from registry before building.
            if pull {
                if let Some(reg) = &manifest.registry {
                    let pulled = registry::pull_from_registry(reg, &plan);
                    if !pulled.is_empty() {
                        info!("{} overlay images pulled from registry", pulled.len());
                    }
                } else {
                    warn!("--pull specified but no [registry] configured in manifest");
                }
            }

            if locked {
                let lock_path = project_root.join("Autoware.lock");
                let lock = match LockFile::read(&lock_path) {
                    Ok(l) => l,
                    Err(e) => {
                        eprintln!("error: {e}");
                        std::process::exit(1);
                    }
                };
                if let Err(e) = builder::verify_locked(&engine, &lock, &plan, &resolved) {
                    eprintln!("error: {e}");
                    std::process::exit(1);
                }
                return;
            }

            let result =
                match builder::execute_plan(&engine, &manifest, &plan, &resolved, project_root) {
                    Ok(r) => r,
                    Err(e) => {
                        eprintln!("error: {e}");
                        std::process::exit(1);
                    }
                };

            let lock = builder::build_lock(&manifest, &result);
            let lock_path = project_root.join("Autoware.lock");
            if let Err(e) = lock.write(&lock_path) {
                eprintln!("error: failed to write lock file: {e}");
                std::process::exit(1);
            }
            info!("wrote {}", lock_path.display());
        }
        Command::Run { detach } => {
            let project_root = manifest_dir(&cli.manifest);
            if let Err(e) = runner::run(project_root, detach) {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
        Command::Stop => {
            let project_root = manifest_dir(&cli.manifest);
            if let Err(e) = runner::stop(project_root) {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
        Command::Logs { component, follow } => {
            let project_root = manifest_dir(&cli.manifest);
            if let Err(e) = runner::logs(project_root, component.as_deref(), follow) {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
        Command::Push => {
            let manifest = load_manifest(&cli.manifest);
            let project_root = manifest_dir(&cli.manifest);
            let resolved = match resolve_platform(&manifest.platform) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("error: {e}");
                    std::process::exit(1);
                }
            };
            let plan = match resolver::resolve(&manifest, &resolved) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("error: {e}");
                    std::process::exit(1);
                }
            };

            let reg = match &manifest.registry {
                Some(r) => r,
                None => {
                    eprintln!("error: no [registry] configured in manifest");
                    std::process::exit(1);
                }
            };

            // Check login status before pushing.
            if let Some(warning) = registry::check_login(&reg.url).warning(&reg.url) {
                eprintln!("warning: {warning}");
            }

            // Read existing build result from lock file.
            let lock_path = project_root.join("Autoware.lock");
            let lock = match LockFile::read(&lock_path) {
                Ok(l) => l,
                Err(_) => {
                    eprintln!("error: no Autoware.lock found. Run `aw-kit build` first.");
                    std::process::exit(1);
                }
            };

            let result = builder::BuildResult {
                step_results: lock
                    .components
                    .iter()
                    .map(|c| builder::StepResult {
                        component: c.name.clone(),
                        image: c.image.clone(),
                        digest: c.digest.clone(),
                        layer_type: None,
                    })
                    .collect(),
            };

            if let Err(e) = registry::push(reg, &plan, &result) {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
        Command::New { .. } => info!("new is not yet implemented"),
        Command::Upgrade { .. } => info!("upgrade is not yet implemented"),
        Command::Rebase { .. } => info!("rebase is not yet implemented"),
    }
}

fn manifest_dir(path: &std::path::Path) -> &std::path::Path {
    path.parent().unwrap_or_else(|| std::path::Path::new("."))
}

fn load_manifest(path: &std::path::Path) -> ManifestConfig {
    match ManifestConfig::from_file(path) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    }
}

fn print_manifest(m: &ManifestConfig) {
    println!("Autoware {}", m.workspace.autoware);
    println!();

    let p = &m.platform;
    println!("Platform:");
    println!("  arch:    {}", p.arch);
    if let Some(cuda) = p.cuda {
        println!("  cuda:    {cuda}");
    }
    if let Some(ref device) = p.device {
        println!("  device:  {device}");
    }
    if let Some(ref jp) = p.jetpack {
        println!("  jetpack: {jp}");
    }
    println!();

    let enabled = m.enabled_components();
    println!("Components ({}):", enabled.len());
    for c in &enabled {
        println!("  {c}");
    }

    if !m.patch.is_empty() {
        println!();
        println!("Patches:");
        for (component, patches) in &m.patch {
            for (pkg, source) in patches {
                let desc = match source {
                    aw_kit::manifest::PatchSource::Git {
                        git, branch, tag, ..
                    } => {
                        let refspec = branch.as_deref().or(tag.as_deref()).unwrap_or("HEAD");
                        format!("{git} @ {refspec}")
                    }
                    aw_kit::manifest::PatchSource::Path { path } => {
                        format!("{}", path.display())
                    }
                };
                println!("  {component}/{pkg}: {desc}");
            }
        }
    }

    if !m.package.is_empty() {
        println!();
        println!("Packages:");
        for pkg in &m.package {
            println!(
                "  {} (extends {}, path: {})",
                pkg.name,
                pkg.extends,
                pkg.path.display()
            );
        }
    }

    if let Some(ref r) = m.registry {
        println!();
        println!("Registry: {}/{}", r.url, r.prefix);
    }
}
