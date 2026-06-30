#![allow(dead_code)]
use thiserror::Error;
use std::io;
use std::path::PathBuf;

#[derive(Error, Debug)]
pub enum MdError {
    #[error("IO Error: {0}")]
    Io(#[from] io::Error),
    
    #[error("Nix/Ioctl Error: {0}")]
    Nix(#[from] nix::Error),
    
    #[error("Invalid Metadata: {0}")]
    InvalidMetadata(String),
    
    #[error("Sysfs Error: {0}")]
    Sysfs(String),
    
    #[error("Device {0} is already in use by another array or filesystem")]
    DeviceInUse(PathBuf),
    
    #[error("Device {0} is currently mounted at {1}")]
    DeviceMounted(PathBuf, String),
    
    #[error("Device {0} has an existing filesystem: {1}")]
    DeviceHasFilesystem(PathBuf, String),
    
    #[error("Device {0} is not a block device")]
    NotBlockDevice(PathBuf),
    
    #[error("Device {0} is too small: {1} bytes (minimum: {2} bytes)")]
    DeviceTooSmall(PathBuf, u64, u64),
    
    #[error("Insufficient devices: need {needed} for RAID{level}, got {actual}")]
    InsufficientDevices { level: u8, needed: u32, actual: u32 },
    
    #[error("Array {0} is degraded: {1} failed disks out of {2}")]
    ArrayDegraded(String, i32, i32),
    
    #[error("Array {0} is not found or not active")]
    ArrayNotFound(String),
    
    #[error("Device not found: {0}")]
    DeviceNotFound(String),
    
    #[error("Invalid device: {0}")]
    InvalidDevice(String),
    
    #[error("Invalid state: {0}")]
    InvalidState(String),
    
    #[error("Array {0} is in state '{1}', expected '{2}'")]
    InvalidArrayState(String, String, String),
    
    #[error("UUID mismatch: expected {expected}, found {found}")]
    UuidMismatch { expected: String, found: String },
    
    #[error("Superblock magic mismatch: expected {expected:#x}, found {found:#x}")]
    MagicMismatch { expected: u32, found: u32 },
    
    #[error("Unsupported metadata version: {0}")]
    UnsupportedMetadataVersion(String),
    
    #[error("Unsupported RAID level: {0}")]
    UnsupportedRaidLevel(u8),
    
    #[error("SMART health check failed for device {0}: {1}")]
    SmartCheckFailed(PathBuf, String),
    
    #[error("Configuration validation error: {0}")]
    ConfigValidation(String),
    
    #[error("Operation would result in data loss: {0}")]
    DataLossRisk(String),
    
    #[error("Bitmap error: {0}")]
    Bitmap(String),
    
    #[error("Reshape operation error: {0}")]
    Reshape(String),
    
    #[error("Transaction rollback failed: {0}")]
    RollbackFailed(String),
    
    #[error("API error: {0}")]
    Api(String),
    
    #[error("Notification error: {0}")]
    Notification(String),
    
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    
    #[error("YAML error: {0}")]
    Yaml(#[from] serde_yaml::Error),
    
    #[error("HTTP request error: {0}")]
    Http(#[from] reqwest::Error),
    
    #[error("Email error: {0}")]
    Email(String),
    
    #[error("Permission denied: {0}")]
    PermissionDenied(String),
    
    #[error("Operation timeout after {0} seconds")]
    Timeout(u64),
    
    #[error("Concurrent modification detected: {0}")]
    ConcurrentModification(String),
}

impl MdError {
    /// Add context to an error
    pub fn context(self, ctx: impl Into<String>) -> Self {
        match self {
            MdError::InvalidMetadata(msg) => MdError::InvalidMetadata(format!("{}: {}", ctx.into(), msg)),
            MdError::Sysfs(msg) => MdError::Sysfs(format!("{}: {}", ctx.into(), msg)),
            _ => self,
        }
    }
    
    /// Check if error is recoverable
    pub fn is_recoverable(&self) -> bool {
        matches!(
            self,
            MdError::Timeout(_) | 
            MdError::ConcurrentModification(_) |
            MdError::Http(_)
        )
    }
    
    /// Check if error indicates data loss risk
    pub fn is_data_loss_risk(&self) -> bool {
        matches!(
            self,
            MdError::DataLossRisk(_) |
            MdError::ArrayDegraded(_, _, _)
        )
    }
}

/// Result type alias for MD operations
pub type MdResult<T> = Result<T, MdError>;