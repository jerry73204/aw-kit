use clap::Parser;
use tracing::info;

use aw_kit::{
    cli::{Cli, Command},
    manifest::ManifestConfig,
    platform::resolve_platform,
    resolver,
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
            pull: _,
            locked: _,
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
            } else {
                info!("build is not yet implemented");
            }
        }
        Command::Run { .. } => info!("run is not yet implemented"),
        Command::Stop => info!("stop is not yet implemented"),
        Command::Logs { .. } => info!("logs is not yet implemented"),
        Command::New { .. } => info!("new is not yet implemented"),
        Command::Upgrade { .. } => info!("upgrade is not yet implemented"),
        Command::Push => info!("push is not yet implemented"),
        Command::Rebase { .. } => info!("rebase is not yet implemented"),
    }
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
