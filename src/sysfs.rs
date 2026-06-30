#![allow(dead_code)]
//! Interfaces with /sys/block/md* for querying and setting array state without ioctls where possible.

use std::fs;
use std::path::PathBuf;
use crate::error::MdError;

pub struct MdSysfs {
    base_path: PathBuf,
}

impl MdSysfs {
    pub fn new(md_name: &str) -> Self {
        Self {
            base_path: PathBuf::from(format!("/sys/block/{}", md_name)),
        }
    }

    pub fn get_array_state(&self) -> Result<ArrayState, MdError> {
        let path = self.base_path.join("md/array_state");
        let state_str = fs::read_to_string(&path)
            .map_err(|e| MdError::Sysfs(format!("Failed to read array state: {}", e)))?;
        state_str.trim().parse()
    }

    pub fn start_scrub(&self, repair: bool) -> Result<(), MdError> {
        let path = self.base_path.join("md/sync_action");
        let action = if repair { "repair" } else { "check" };
        fs::write(&path, action)
            .map_err(|e| MdError::Sysfs(format!("Failed to start scrub via sync_action: {}", e)))
    }

    pub fn get_sync_action(&self) -> Result<String, MdError> {
        let path = self.base_path.join("md/sync_action");
        if !path.exists() {
            return Ok("idle".to_string());
        }
        let action = fs::read_to_string(&path)
            .map_err(|e| MdError::Sysfs(format!("Failed to read sync_action: {}", e)))?;
        Ok(action.trim().to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArrayState {
    Clear,
    Inactive,
    Suspended,
    Readonly,
    ReadAuto,
    Clean,
    Active,
    WritePending,
    ActiveIdle,
    Unknown(String),
}

impl std::str::FromStr for ArrayState {
    type Err = MdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "clear" => Ok(ArrayState::Clear),
            "inactive" => Ok(ArrayState::Inactive),
            "suspended" => Ok(ArrayState::Suspended),
            "readonly" => Ok(ArrayState::Readonly),
            "read-auto" => Ok(ArrayState::ReadAuto),
            "clean" => Ok(ArrayState::Clean),
            "active" => Ok(ArrayState::Active),
            "write-pending" => Ok(ArrayState::WritePending),
            "active-idle" => Ok(ArrayState::ActiveIdle),
            other => Ok(ArrayState::Unknown(other.to_string())),
        }
    }
}

impl std::fmt::Display for ArrayState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ArrayState::Clear => write!(f, "clear"),
            ArrayState::Inactive => write!(f, "inactive"),
            ArrayState::Suspended => write!(f, "suspended"),
            ArrayState::Readonly => write!(f, "readonly"),
            ArrayState::ReadAuto => write!(f, "read-auto"),
            ArrayState::Clean => write!(f, "clean"),
            ArrayState::Active => write!(f, "active"),
            ArrayState::WritePending => write!(f, "write-pending"),
            ArrayState::ActiveIdle => write!(f, "active-idle"),
            ArrayState::Unknown(s) => write!(f, "{}", s),
        }
    }
}
