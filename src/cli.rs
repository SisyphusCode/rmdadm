use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "rmdadm", about = "Manage MD devices (software RAID)")]
pub struct Cli {
    #[arg(global = true, long)]
    pub json: bool,
    #[arg(global = true, long)]
    pub dry_run: bool,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Assemble a previously created array
    Assemble {
        md_device: Option<PathBuf>,
        components: Option<Vec<PathBuf>>,
        #[arg(long)]
        auto: Option<PathBuf>,
    },
    /// Create a new array
    Create {
        md_device: PathBuf,
        #[arg(short, long)]
        level: u8,
        #[arg(short = 'n', long)]
        raid_devices: u32,
        #[arg(short = 'm', long, default_value = "1.2")]
        metadata: String,
        components: Vec<PathBuf>,
    },
    /// Print details of one or more md devices
    Detail {
        md_device: PathBuf,
    },
    /// Stop an active array
    Stop {
        md_device: PathBuf,
        #[arg(long)]
        force: bool,
    },
    /// Manage an active array (add, remove, fail disks)
    Manage {
        md_device: PathBuf,
        #[arg(long, num_args = 1..)]
        add: Option<Vec<PathBuf>>,
        #[arg(long, num_args = 1..)]
        remove: Option<Vec<PathBuf>>,
        #[arg(long, num_args = 1..)]
        fail: Option<Vec<PathBuf>>,
        #[arg(long)]
        force: bool,
    },
    /// Apply a declarative configuration from a YAML file
    Apply {
        config_file: PathBuf,
    },
    /// Interactive TUI monitor
    Monitor,
    /// Run a Prometheus metrics exporter on port 9090
    Exporter,
    /// Run the daemon (API Server + Background Monitor)
    Daemon {
        #[arg(long, default_value = "0.0.0.0:8080")]
        addr: String,
    },
}
