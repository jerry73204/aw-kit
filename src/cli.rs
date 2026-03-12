use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "aw-kit", version, about = "Autoware deployment toolkit")]
pub struct Cli {
    /// Path to Autoware.toml manifest
    #[arg(long, default_value = "Autoware.toml")]
    pub manifest: PathBuf,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Resolve images and build containers from the manifest
    Build {
        /// Print the resolved build plan without executing
        #[arg(long)]
        dry_run: bool,

        /// Pull pre-built images from registry before building locally
        #[arg(long)]
        pull: bool,

        /// Require all images match Autoware.lock digests exactly
        #[arg(long)]
        locked: bool,
    },

    /// Start components via docker compose
    Run {
        /// Run containers in the background
        #[arg(short, long)]
        detach: bool,
    },

    /// Stop running components
    Stop,

    /// Show component logs
    Logs {
        /// Component to show logs for (all if omitted)
        component: Option<String>,

        /// Follow log output
        #[arg(short, long)]
        follow: bool,
    },

    /// Scaffold a new ROS2 package extending a component
    New {
        /// Package name
        name: String,

        /// Component to extend
        #[arg(long)]
        extends: String,
    },

    /// Upgrade Autoware version and check patch compatibility
    Upgrade {
        /// Target version
        #[arg(long)]
        to: String,
    },

    /// Push built images to the configured registry
    Push,

    /// Reapply patches after an upgrade
    Rebase {
        /// Component to rebase patches for
        component: String,

        /// Continue after manual conflict resolution
        #[arg(long)]
        r#continue: bool,
    },
}
