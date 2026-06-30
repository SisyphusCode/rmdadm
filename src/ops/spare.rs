//! Hot spare management for RAID arrays
//! Manages spare disks that can automatically replace failed disks

use std::path::{Path, PathBuf};
use std::fs;
use tracing::{info, debug};
use crate::error::MdError;
use crate::sysfs::MdSysfs;
use crate::validation;

/// Spare disk configuration
#[derive(Debug, Clone)]
pub struct SpareConfig {
    /// Spare disk device path
    pub device: PathBuf,
    /// Whether to force addition even if disk has data
    pub force: bool,
}

/// Spare disk information
#[derive(Debug, Clone, serde::Serialize)]
pub struct SpareInfo {
    /// Device path
    pub device: String,
    /// Disk state (spare, active, faulty, etc.)
    pub state: String,
    /// Disk slot number
    pub slot: Option<i32>,
    /// Whether disk is a hot spare
    pub is_spare: bool,
}

/// Add a hot spare to an array
pub fn add_spare(
    md_device: &Path,
    spare_device: &Path,
    force: bool,
    dry_run: bool,
) -> Result<(), MdError> {
    info!("Adding spare {} to array {}", spare_device.display(), md_device.display());
    
    // Validate array exists
    if !md_device.exists() {
        return Err(MdError::DeviceNotFound(md_device.to_string_lossy().to_string()));
    }
    
    // Validate spare device exists
    if !spare_device.exists() {
        return Err(MdError::DeviceNotFound(spare_device.to_string_lossy().to_string()));
    }
    
    // Check if device is suitable (unless force)
    if !force {
        validation::check_device_suitable(spare_device)?;
    }
    
    let array_name = md_device
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| MdError::InvalidDevice("Invalid device name".to_string()))?;
    
    let sys = MdSysfs::new(array_name);
    
    // Check array state
    let state = sys.get_array_state()
        .map_err(|e| MdError::Sysfs(format!("Failed to read array state: {}", e)))?;
    
    debug!("Array state: {:?}", state);
    
    if dry_run {
        info!("DRY RUN: Would add spare {} to array {}", spare_device.display(), array_name);
        return Ok(());
    }
    
    // Add spare by writing device path to md/new_dev
    let spare_path = spare_device.to_string_lossy();
    sys.write_sysfs_value("md/new_dev", &spare_path)
        .map_err(|e| MdError::Sysfs(format!("Failed to add spare: {}", e)))?;
    
    info!("✅ Spare disk added successfully");
    info!("The spare will automatically replace any failed disk");
    
    Ok(())
}

/// Remove a spare from an array
pub fn remove_spare(
    md_device: &Path,
    spare_device: &Path,
    dry_run: bool,
) -> Result<(), MdError> {
    info!("Removing spare {} from array {}", spare_device.display(), md_device.display());
    
    if !md_device.exists() {
        return Err(MdError::DeviceNotFound(md_device.to_string_lossy().to_string()));
    }
    
    let array_name = md_device
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| MdError::InvalidDevice("Invalid device name".to_string()))?;
    
    let sys = MdSysfs::new(array_name);
    
    // Verify device is actually a spare
    let spares = list_spares(md_device)?;
    let spare_path = spare_device.to_string_lossy();
    
    if !spares.iter().any(|s| s.device == spare_path) {
        return Err(MdError::ConfigValidation(
            format!("Device {} is not a spare in array {}", spare_path, array_name)
        ));
    }
    
    if dry_run {
        info!("DRY RUN: Would remove spare {} from array {}", spare_device.display(), array_name);
        return Ok(());
    }
    
    // Remove spare by writing "remove" to the device's state
    let dev_name = spare_device
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| MdError::InvalidDevice("Invalid device name".to_string()))?;
    
    sys.write_sysfs_value(&format!("md/dev-{}/state", dev_name), "remove")
        .map_err(|e| MdError::Sysfs(format!("Failed to remove spare: {}", e)))?;
    
    info!("✅ Spare disk removed successfully");
    Ok(())
}

/// List all spare disks in an array
pub fn list_spares(md_device: &Path) -> Result<Vec<SpareInfo>, MdError> {
    let array_name = md_device
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| MdError::InvalidDevice("Invalid device name".to_string()))?;
    
    let md_path = format!("/sys/block/{}/md", array_name);
    
    let mut spares = Vec::new();
    
    // Read all device directories
    let entries = fs::read_dir(&md_path)
        .map_err(|e| MdError::Sysfs(format!("Failed to read md directory: {}", e)))?;
    
    for entry in entries {
        let entry = entry.map_err(|e| MdError::Sysfs(format!("Failed to read entry: {}", e)))?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        
        // Look for dev-* directories
        if !name_str.starts_with("dev-") {
            continue;
        }
        
        // Read device state
        let state_path = entry.path().join("state");
        let state = fs::read_to_string(&state_path)
            .map_err(|e| MdError::Sysfs(format!("Failed to read device state: {}", e)))?;
        let state = state.trim().to_string();
        
        // Check if it's a spare
        let is_spare = state.contains("spare");
        
        // Read slot number
        let slot_path = entry.path().join("slot");
        let slot = fs::read_to_string(&slot_path)
            .ok()
            .and_then(|s| s.trim().parse::<i32>().ok());
        
        // Get device path
        let block_path = entry.path().join("block");
        let device = if block_path.exists() {
            fs::read_dir(&block_path)
                .ok()
                .and_then(|mut entries| entries.next())
                .and_then(|e| e.ok())
                .map(|e| format!("/dev/{}", e.file_name().to_string_lossy()))
                .unwrap_or_else(|| "unknown".to_string())
        } else {
            "unknown".to_string()
        };
        
        if is_spare {
            spares.push(SpareInfo {
                device,
                state,
                slot,
                is_spare,
            });
        }
    }
    
    Ok(spares)
}

/// Get spare disk count for an array
pub fn get_spare_count(md_device: &Path) -> Result<u32, MdError> {
    let spares = list_spares(md_device)?;
    Ok(spares.len() as u32)
}

/// Configure automatic spare activation
pub fn configure_spare_policy(
    md_device: &Path,
    min_spares: Option<u32>,
    max_spares: Option<u32>,
) -> Result<(), MdError> {
    info!("Configuring spare policy for array: {}", md_device.display());
    
    let array_name = md_device
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| MdError::InvalidDevice("Invalid device name".to_string()))?;
    
    let sys = MdSysfs::new(array_name);
    
    // Set minimum spares if specified
    if let Some(min) = min_spares {
        sys.write_sysfs_value("md/min_spare", &min.to_string())
            .map_err(|e| MdError::Sysfs(format!("Failed to set min_spare: {}", e)))?;
        info!("Set minimum spares to: {}", min);
    }
    
    // Set maximum spares if specified
    if let Some(max) = max_spares {
        sys.write_sysfs_value("md/max_spare", &max.to_string())
            .map_err(|e| MdError::Sysfs(format!("Failed to set max_spare: {}", e)))?;
        info!("Set maximum spares to: {}", max);
    }
    
    Ok(())
}

/// Check if a device is a spare in any array
pub fn is_spare_device(device: &Path) -> Result<bool, MdError> {
    // Read /proc/mdstat to find all arrays
    let mdstat = fs::read_to_string("/proc/mdstat")
        .map_err(|e| MdError::Sysfs(format!("Failed to read /proc/mdstat: {}", e)))?;
    
    let device_name = device
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| MdError::InvalidDevice("Invalid device name".to_string()))?;
    
    // Check if device appears in mdstat with (S) marker (spare)
    for line in mdstat.lines() {
        if line.contains(device_name) && line.contains("(S)") {
            return Ok(true);
        }
    }
    
    Ok(false)
}

/// Activate a spare disk (force it to replace a specific failed disk)
pub fn activate_spare(
    md_device: &Path,
    spare_device: &Path,
    target_slot: Option<u32>,
) -> Result<(), MdError> {
    info!("Activating spare {} in array {}", spare_device.display(), md_device.display());
    
    let array_name = md_device
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| MdError::InvalidDevice("Invalid device name".to_string()))?;
    
    let sys = MdSysfs::new(array_name);
    
    // Verify device is a spare
    let spares = list_spares(md_device)?;
    let spare_path = spare_device.to_string_lossy();
    
    if !spares.iter().any(|s| s.device == spare_path) {
        return Err(MdError::ConfigValidation(
            format!("Device {} is not a spare in array {}", spare_path, array_name)
        ));
    }
    
    // If target slot specified, write to that slot
    if let Some(slot) = target_slot {
        let dev_name = spare_device
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| MdError::InvalidDevice("Invalid device name".to_string()))?;
        
        sys.write_sysfs_value(&format!("md/dev-{}/slot", dev_name), &slot.to_string())
            .map_err(|e| MdError::Sysfs(format!("Failed to activate spare: {}", e)))?;
    } else {
        // Trigger rebuild by writing to sync_action
        sys.write_sysfs_value("sync_action", "recover")
            .map_err(|e| MdError::Sysfs(format!("Failed to trigger recovery: {}", e)))?;
    }
    
    info!("✅ Spare activation initiated");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spare_config() {
        let config = SpareConfig {
            device: PathBuf::from("/dev/sdd1"),
            force: false,
        };
        assert_eq!(config.device, PathBuf::from("/dev/sdd1"));
        assert!(!config.force);
    }

    #[test]
    fn test_spare_info() {
        let info = SpareInfo {
            device: "/dev/sdd1".to_string(),
            state: "spare".to_string(),
            slot: Some(3),
            is_spare: true,
        };
        assert!(info.is_spare);
        assert_eq!(info.slot, Some(3));
    }
}
