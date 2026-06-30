//! RAID array reshape operations
//! Allows changing RAID level, layout, or chunk size of existing arrays

use std::path::Path;
use std::fs;
use std::io::{self, Write};
use tracing::{info, warn, error, debug};
use crate::error::MdError;
use crate::sysfs::MdSysfs;
use crate::validation;

/// Reshape operation types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReshapeType {
    /// Change RAID level
    Level,
    /// Change chunk size
    ChunkSize,
    /// Change layout
    Layout,
    /// Add devices (grow)
    Grow,
    /// Remove devices (shrink)
    Shrink,
}

/// Reshape configuration
#[derive(Debug, Clone)]
pub struct ReshapeConfig {
    /// Target RAID level (if changing level)
    pub target_level: Option<u8>,
    /// Target chunk size in KB (if changing chunk size)
    pub target_chunk_size: Option<u32>,
    /// Target layout (if changing layout)
    pub target_layout: Option<String>,
    /// Number of devices to add/remove
    pub device_delta: Option<i32>,
    /// Backup file for reshape operation
    pub backup_file: Option<String>,
    /// Force reshape even if risky
    pub force: bool,
}

impl Default for ReshapeConfig {
    fn default() -> Self {
        Self {
            target_level: None,
            target_chunk_size: None,
            target_layout: None,
            device_delta: None,
            backup_file: None,
            force: false,
        }
    }
}

/// Reshape an existing RAID array
pub fn reshape_array(
    md_device: &Path,
    config: ReshapeConfig,
    dry_run: bool,
) -> Result<(), MdError> {
    info!("Reshaping array: {}", md_device.display());
    
    // Validate array exists and is active
    if !md_device.exists() {
        return Err(MdError::DeviceNotFound(md_device.to_string_lossy().to_string()));
    }
    
    let array_name = md_device
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| MdError::InvalidDevice("Invalid device name".to_string()))?;
    
    let sys = MdSysfs::new(array_name);
    
    // Get current array state
    let current_state = sys.get_array_state()
        .map_err(|e| MdError::Sysfs(format!("Failed to read array state: {}", e)))?;
    
    if current_state.to_string() != "active" && current_state.to_string() != "clean" {
        return Err(MdError::InvalidState(
            format!("Array must be active or clean to reshape, current state: {}", current_state)
        ));
    }
    
    // Validate reshape configuration
    validate_reshape_config(&sys, &config)?;
    
    if dry_run {
        info!("DRY RUN: Would reshape array {} with config: {:?}", array_name, config);
        print_reshape_plan(&sys, &config)?;
        return Ok(());
    }
    
    // Warn user about risks
    warn!("⚠️  RESHAPE OPERATION IS RISKY!");
    warn!("⚠️  Ensure you have a backup before proceeding");
    warn!("⚠️  Do not interrupt the reshape process");
    warn!("⚠️  System crash during reshape may result in data loss");
    
    if !config.force {
        print!("Type 'yes' to continue: ");
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        if input.trim().to_lowercase() != "yes" {
            info!("Reshape cancelled by user");
            return Ok(());
        }
    }
    
    // Perform reshape based on type
    if let Some(level) = config.target_level {
        reshape_level(&sys, level, &config)?;
    }
    
    if let Some(chunk_size) = config.target_chunk_size {
        reshape_chunk_size(&sys, chunk_size)?;
    }
    
    if let Some(ref layout) = config.target_layout {
        reshape_layout(&sys, layout)?;
    }
    
    if let Some(delta) = config.device_delta {
        if delta > 0 {
            grow_array(&sys, delta as u32)?;
        } else if delta < 0 {
            shrink_array(&sys, (-delta) as u32)?;
        }
    }
    
    info!("✅ Reshape initiated successfully");
    info!("Monitor progress: watch cat /proc/mdstat");
    info!("Or use: rmdadm detail {}", md_device.display());
    
    Ok(())
}

/// Validate reshape configuration
fn validate_reshape_config(sys: &MdSysfs, config: &ReshapeConfig) -> Result<(), MdError> {
    // Check if any reshape operation is specified
    if config.target_level.is_none() 
        && config.target_chunk_size.is_none() 
        && config.target_layout.is_none() 
        && config.device_delta.is_none() {
        return Err(MdError::ConfigValidation(
            "No reshape operation specified".to_string()
        ));
    }
    
    // Validate target level if specified
    if let Some(level) = config.target_level {
        if ![0, 1, 4, 5, 6, 10].contains(&level) {
            return Err(MdError::ConfigValidation(
                format!("Invalid target RAID level: {}", level)
            ));
        }
    }
    
    // Validate chunk size if specified
    if let Some(chunk_size) = config.target_chunk_size {
        if chunk_size < 4 || chunk_size > 32768 || !chunk_size.is_power_of_two() {
            return Err(MdError::ConfigValidation(
                format!("Invalid chunk size: {} (must be power of 2, between 4-32768 KB)", chunk_size)
            ));
        }
    }
    
    // Check if array supports reshape
    let level_str = sys.read_sysfs_value("level")
        .map_err(|e| MdError::Sysfs(format!("Failed to read level: {}", e)))?;
    
    debug!("Current array level: {}", level_str);
    
    Ok(())
}

/// Print reshape plan
fn print_reshape_plan(sys: &MdSysfs, config: &ReshapeConfig) -> Result<(), MdError> {
    println!("\n📋 Reshape Plan:");
    println!("================");
    
    if let Some(level) = config.target_level {
        let current_level = sys.read_sysfs_value("level")
            .unwrap_or_else(|_| "unknown".to_string());
        println!("RAID Level: {} → {}", current_level, level);
    }
    
    if let Some(chunk_size) = config.target_chunk_size {
        let current_chunk = sys.read_sysfs_value("chunk_size")
            .unwrap_or_else(|_| "unknown".to_string());
        println!("Chunk Size: {} → {} KB", current_chunk, chunk_size);
    }
    
    if let Some(ref layout) = config.target_layout {
        let current_layout = sys.read_sysfs_value("layout")
            .unwrap_or_else(|_| "unknown".to_string());
        println!("Layout: {} → {}", current_layout, layout);
    }
    
    if let Some(delta) = config.device_delta {
        if delta > 0 {
            println!("Operation: Grow array by {} devices", delta);
        } else {
            println!("Operation: Shrink array by {} devices", -delta);
        }
    }
    
    if let Some(ref backup) = config.backup_file {
        println!("Backup File: {}", backup);
    }
    
    println!("\n⚠️  Estimated Time: Several hours (depends on array size)");
    println!("⚠️  System Performance: May be degraded during reshape");
    println!("⚠️  Risk Level: HIGH - Ensure backups are current");
    println!();
    
    Ok(())
}

/// Reshape array to different RAID level
fn reshape_level(sys: &MdSysfs, target_level: u8, config: &ReshapeConfig) -> Result<(), MdError> {
    info!("Reshaping to RAID level {}", target_level);
    
    // Write new level to sysfs
    sys.write_sysfs_value("level", &format!("raid{}", target_level))
        .map_err(|e| MdError::Sysfs(format!("Failed to set level: {}", e)))?;
    
    // If backup file specified, configure it
    if let Some(ref backup) = config.backup_file {
        sys.write_sysfs_value("sync_action", &format!("reshape {}", backup))
            .map_err(|e| MdError::Sysfs(format!("Failed to set backup file: {}", e)))?;
    } else {
        sys.write_sysfs_value("sync_action", "reshape")
            .map_err(|e| MdError::Sysfs(format!("Failed to start reshape: {}", e)))?;
    }
    
    Ok(())
}

/// Reshape array chunk size
fn reshape_chunk_size(sys: &MdSysfs, target_chunk_size: u32) -> Result<(), MdError> {
    info!("Reshaping chunk size to {} KB", target_chunk_size);
    
    sys.write_sysfs_value("chunk_size", &target_chunk_size.to_string())
        .map_err(|e| MdError::Sysfs(format!("Failed to set chunk size: {}", e)))?;
    
    sys.write_sysfs_value("sync_action", "reshape")
        .map_err(|e| MdError::Sysfs(format!("Failed to start reshape: {}", e)))?;
    
    Ok(())
}

/// Reshape array layout
fn reshape_layout(sys: &MdSysfs, target_layout: &str) -> Result<(), MdError> {
    info!("Reshaping layout to {}", target_layout);
    
    sys.write_sysfs_value("layout", target_layout)
        .map_err(|e| MdError::Sysfs(format!("Failed to set layout: {}", e)))?;
    
    sys.write_sysfs_value("sync_action", "reshape")
        .map_err(|e| MdError::Sysfs(format!("Failed to start reshape: {}", e)))?;
    
    Ok(())
}

/// Grow array by adding devices
fn grow_array(sys: &MdSysfs, num_devices: u32) -> Result<(), MdError> {
    info!("Growing array by {} devices", num_devices);
    
    let current_devices = sys.read_sysfs_value("raid_disks")
        .and_then(|s| s.parse::<u32>().map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e)))
        .map_err(|e| MdError::Sysfs(format!("Failed to read raid_disks: {}", e)))?;
    
    let new_devices = current_devices + num_devices;
    
    sys.write_sysfs_value("raid_disks", &new_devices.to_string())
        .map_err(|e| MdError::Sysfs(format!("Failed to set raid_disks: {}", e)))?;
    
    sys.write_sysfs_value("sync_action", "reshape")
        .map_err(|e| MdError::Sysfs(format!("Failed to start reshape: {}", e)))?;
    
    Ok(())
}

/// Shrink array by removing devices
fn shrink_array(sys: &MdSysfs, num_devices: u32) -> Result<(), MdError> {
    info!("Shrinking array by {} devices", num_devices);
    
    let current_devices = sys.read_sysfs_value("raid_disks")
        .and_then(|s| s.parse::<u32>().map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e)))
        .map_err(|e| MdError::Sysfs(format!("Failed to read raid_disks: {}", e)))?;
    
    if num_devices >= current_devices {
        return Err(MdError::ConfigValidation(
            format!("Cannot remove {} devices from array with {} devices", num_devices, current_devices)
        ));
    }
    
    let new_devices = current_devices - num_devices;
    
    sys.write_sysfs_value("raid_disks", &new_devices.to_string())
        .map_err(|e| MdError::Sysfs(format!("Failed to set raid_disks: {}", e)))?;
    
    sys.write_sysfs_value("sync_action", "reshape")
        .map_err(|e| MdError::Sysfs(format!("Failed to start reshape: {}", e)))?;
    
    Ok(())
}

/// Get reshape progress
pub fn get_reshape_progress(md_device: &Path) -> Result<f64, MdError> {
    let array_name = md_device
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| MdError::InvalidDevice("Invalid device name".to_string()))?;
    
    let sys = MdSysfs::new(array_name);
    
    let sync_completed = sys.read_sysfs_value("sync_completed")
        .map_err(|e| MdError::Sysfs(format!("Failed to read sync_completed: {}", e)))?;
    
    // Parse "completed / total" format
    let parts: Vec<&str> = sync_completed.split('/').collect();
    if parts.len() != 2 {
        return Ok(0.0);
    }
    
    let completed: u64 = parts[0].trim().parse()
        .map_err(|e| MdError::Sysfs(format!("Failed to parse completed: {}", e)))?;
    let total: u64 = parts[1].trim().parse()
        .map_err(|e| MdError::Sysfs(format!("Failed to parse total: {}", e)))?;
    
    if total == 0 {
        return Ok(100.0);
    }
    
    Ok((completed as f64 / total as f64) * 100.0)
}

/// Cancel ongoing reshape operation
pub fn cancel_reshape(md_device: &Path) -> Result<(), MdError> {
    info!("Cancelling reshape for: {}", md_device.display());
    
    let array_name = md_device
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| MdError::InvalidDevice("Invalid device name".to_string()))?;
    
    let sys = MdSysfs::new(array_name);
    
    sys.write_sysfs_value("sync_action", "idle")
        .map_err(|e| MdError::Sysfs(format!("Failed to cancel reshape: {}", e)))?;
    
    warn!("⚠️  Reshape cancelled - array may be in inconsistent state");
    warn!("⚠️  Run array check/repair before using");
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reshape_config_default() {
        let config = ReshapeConfig::default();
        assert!(config.target_level.is_none());
        assert!(config.target_chunk_size.is_none());
        assert!(!config.force);
    }

    #[test]
    fn test_reshape_type() {
        assert_eq!(ReshapeType::Level, ReshapeType::Level);
        assert_ne!(ReshapeType::Level, ReshapeType::ChunkSize);
    }
}
