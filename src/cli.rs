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
    /// Reshape an existing RAID array
    Reshape {
        md_device: PathBuf,
        #[arg(long)]
        level: Option<u8>,
        #[arg(long)]
        chunk_size: Option<u32>,
        #[arg(long)]
        layout: Option<String>,
        #[arg(long)]
        delta: Option<i32>,
        #[arg(long)]
        backup_file: Option<String>,
        #[arg(long)]
        force: bool,
    },
    /// Manage write-intent bitmaps
    Bitmap {
        md_device: PathBuf,
        #[command(subcommand)]
        action: BitmapAction,
    },
    /// Manage hot spare disks
    Spare {
        md_device: PathBuf,
        #[command(subcommand)]
        action: SpareAction,
    },
}

#[derive(Subcommand, Debug)]
pub enum BitmapAction {
    /// Add bitmap to array
    Add {
        #[arg(long, default_value = "internal")]
        location: String,
        #[arg(long)]
        chunk_size: Option<u32>,
        #[arg(long)]
        file: Option<String>,
    },
    /// Remove bitmap from array
    Remove,
    /// Show bitmap information
    Info,
    /// Clear bitmap (mark all blocks clean)
    Clear,
}

#[derive(Subcommand, Debug)]
pub enum SpareAction {
    /// Add a hot spare to array
    Add {
        spare_device: PathBuf,
        #[arg(long)]
        force: bool,
    },
    /// Remove a spare from array
    Remove {
        spare_device: PathBuf,
    },
    /// List all spares in array
    List,
    /// Activate a spare disk
    Activate {
        spare_device: PathBuf,
        #[arg(long)]
        slot: Option<u32>,
    },
}
