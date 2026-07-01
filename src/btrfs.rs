use crate::error::MdError;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BtrfsFilesystem {
    pub label: Option<String>,
    pub uuid: Option<String>,
    pub devices: Vec<String>,
}

pub fn filesystem_show(path: Option<&Path>) -> Result<String, MdError> {
    let mut command = Command::new("btrfs");
    command.arg("filesystem").arg("show");
    if let Some(path) = path {
        command.arg(path);
    }
    command_output(command)
}

pub fn scrub_start(path: &Path, readonly: bool) -> Result<String, MdError> {
    let mut command = Command::new("btrfs");
    command.arg("scrub").arg("start");
    if readonly {
        command.arg("-r");
    }
    command.arg(path);
    command_output(command)
}

pub fn scrub_status(path: &Path) -> Result<String, MdError> {
    let mut command = Command::new("btrfs");
    command.arg("scrub").arg("status").arg(path);
    command_output(command)
}

pub fn balance_start(path: &Path, full: bool) -> Result<String, MdError> {
    let mut command = Command::new("btrfs");
    command.arg("balance").arg("start");
    if !full {
        command.arg("-dusage=75").arg("-musage=75");
    }
    command.arg(path);
    command_output(command)
}

pub fn subvolume_snapshot(source: &Path, destination: &Path, readonly: bool) -> Result<String, MdError> {
    let mut command = Command::new("btrfs");
    command.arg("subvolume").arg("snapshot");
    if readonly {
        command.arg("-r");
    }
    command.arg(source).arg(destination);
    command_output(command)
}

fn command_output(mut command: Command) -> Result<String, MdError> {
    let output = command.output().map_err(MdError::Io)?;
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if output.status.success() {
        Ok(stdout)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(MdError::InvalidState(if stderr.is_empty() {
            "btrfs command failed".to_string()
        } else {
            stderr
        }))
    }
}
