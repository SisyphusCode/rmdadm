//! Device and configuration validation module
//! Provides comprehensive checks before performing operations

use std::path::{Path, PathBuf};
use std::fs;
use std::process::Command;
use crate::error::{MdError, MdResult};
use tracing::{debug, warn, info};

/// Minimum device size for RAID arrays (100MB)
const MIN_DEVICE_SIZE: u64 = 100 * 1024 * 1024;

/// Check if a device is suitable for use in a RAID array
pub fn check_device_suitable(device: &Path) -> MdResult<DeviceInfo> {
    info!("Validating device: {}", device.display());
    
    // Check if device exists
    if !device.exists() {
        return Err(MdError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("Device {} does not exist", device.display())
        )));
    }
    
    // Check if it's a block device
    let metadata = fs::metadata(device)?;
    if !is_block_device(&metadata) {
        return Err(MdError::NotBlockDevice(device.to_path_buf()));
    }
    
    // Check if device is mounted
    if let Some(mount_point) = is_mounted(device)? {
        return Err(MdError::DeviceMounted(device.to_path_buf(), mount_point));
    }
    
    // Check if device has a filesystem
    if let Some(fs_type) = has_filesystem(device)? {
        return Err(MdError::DeviceHasFilesystem(device.to_path_buf(), fs_type));
    }
    
    // Check if device is already in use by MD
    if is_in_md_use(device)? {
        return Err(MdError::DeviceInUse(device.to_path_buf()));
    }
    
    // Get device size
    let size = get_device_size(device)?;
    if size < MIN_DEVICE_SIZE {
        return Err(MdError::DeviceTooSmall(
            device.to_path_buf(),
            size,
            MIN_DEVICE_SIZE
        ));
    }
    
    // Check SMART status (non-fatal)
    let smart_status = check_smart_health(device);
    if let Err(ref e) = smart_status {
        warn!("SMART check failed for {}: {}", device.display(), e);
    }
    
    debug!("Device {} validated successfully (size: {} bytes)", device.display(), size);
    
    Ok(DeviceInfo {
        path: device.to_path_buf(),
        size,
        smart_healthy: smart_status.is_ok(),
    })
}

/// Information about a validated device
#[derive(Debug, Clone)]
pub struct DeviceInfo {
    pub path: PathBuf,
    pub size: u64,
    pub smart_healthy: bool,
}

/// Check if metadata is a block device
fn is_block_device(metadata: &fs::Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::FileTypeExt;
        metadata.file_type().is_block_device()
    }
    #[cfg(not(unix))]
    {
        false
    }
}

/// Check if device is mounted
fn is_mounted(device: &Path) -> MdResult<Option<String>> {
    let output = Command::new("findmnt")
        .arg("-n")
        .arg("-o")
        .arg("TARGET")
        .arg("-S")
        .arg(device)
        .output();
    
    match output {
        Ok(out) if out.status.success() => {
            let mount_point = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if mount_point.is_empty() {
                Ok(None)
            } else {
                Ok(Some(mount_point))
            }
        }
        _ => Ok(None), // findmnt not available or device not mounted
    }
}

/// Check if device has a filesystem
fn has_filesystem(device: &Path) -> MdResult<Option<String>> {
    let output = Command::new("blkid")
        .arg("-s")
        .arg("TYPE")
        .arg("-o")
        .arg("value")
        .arg(device)
        .output();
    
    match output {
        Ok(out) if out.status.success() => {
            let fs_type = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if fs_type.is_empty() || fs_type.contains("linux_raid_member") {
                Ok(None)
            } else {
                Ok(Some(fs_type))
            }
        }
        _ => Ok(None), // blkid not available or no filesystem
    }
}

/// Check if device is in use by MD
fn is_in_md_use(device: &Path) -> MdResult<bool> {
    if let Some(name) = device.file_name() {
        let holders_path = PathBuf::from("/sys/class/block")
            .join(name)
            .join("holders");
        
        if holders_path.exists() {
            if let Ok(entries) = fs::read_dir(holders_path) {
                for entry in entries.flatten() {
                    let holder_name = entry.file_name();
                    if holder_name.to_string_lossy().starts_with("md") {
                        return Ok(true);
                    }
                }
            }
        }
    }
    Ok(false)
}

/// Get device size in bytes
fn get_device_size(device: &Path) -> MdResult<u64> {
    let output = Command::new("blockdev")
        .arg("--getsize64")
        .arg(device)
        .output();
    
    match output {
        Ok(out) if out.status.success() => {
            let size_str = String::from_utf8_lossy(&out.stdout).trim().to_string();
            size_str.parse::<u64>()
                .map_err(|e| MdError::InvalidMetadata(format!("Failed to parse device size: {}", e)))
        }
        Ok(out) => {
            Err(MdError::InvalidMetadata(
                format!("blockdev failed: {}", String::from_utf8_lossy(&out.stderr))
            ))
        }
        Err(e) => Err(MdError::Io(e)),
    }
}

/// Check SMART health status
fn check_smart_health(device: &Path) -> MdResult<()> {
    let output = Command::new("smartctl")
        .arg("-H")
        .arg(device)
        .output();
    
    match output {
        Ok(out) if out.status.success() => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            if stdout.contains("PASSED") {
                Ok(())
            } else {
                Err(MdError::SmartCheckFailed(
                    device.to_path_buf(),
                    "Health check did not pass".to_string()
                ))
            }
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            Err(MdError::SmartCheckFailed(device.to_path_buf(), stderr.to_string()))
        }
        Err(e) => {
            // smartctl not available is not fatal
            warn!("smartctl not available: {}", e);
            Ok(())
        }
    }
}

/// Validate RAID level and device count
pub fn validate_raid_config(level: u8, device_count: u32) -> MdResult<()> {
    let min_devices = match level {
        0 => 2,  // RAID0: minimum 2 devices
        1 => 2,  // RAID1: minimum 2 devices
        4 => 3,  // RAID4: minimum 3 devices
        5 => 3,  // RAID5: minimum 3 devices
        6 => 4,  // RAID6: minimum 4 devices
        10 => 4, // RAID10: minimum 4 devices
        _ => return Err(MdError::UnsupportedRaidLevel(level)),
    };
    
    if device_count < min_devices {
        return Err(MdError::InsufficientDevices {
            level,
            needed: min_devices,
            actual: device_count,
        });
    }
    
    // RAID10 needs even number of devices
    if level == 10 && device_count % 2 != 0 {
        return Err(MdError::ConfigValidation(
            format!("RAID10 requires an even number of devices, got {}", device_count)
        ));
    }
    
    Ok(())
}

/// Validate all devices for array creation
pub fn validate_devices_for_array(devices: &[PathBuf], level: u8) -> MdResult<Vec<DeviceInfo>> {
    info!("Validating {} devices for RAID{}", devices.len(), level);
    
    // Check RAID configuration
    validate_raid_config(level, devices.len() as u32)?;
    
    // Validate each device
    let mut device_infos = Vec::new();
    for device in devices {
        let info = check_device_suitable(device)?;
        device_infos.push(info);
    }
    
    // Check that all devices are roughly the same size
    if let Some(first) = device_infos.first() {
        let first_size = first.size;
        for info in &device_infos {
            let size_diff = if info.size > first_size {
                info.size - first_size
            } else {
                first_size - info.size
            };
            
            // Warn if size difference is more than 10%
            if size_diff > first_size / 10 {
                warn!(
                    "Device {} size ({} bytes) differs significantly from first device ({} bytes)",
                    info.path.display(),
                    info.size,
                    first_size
                );
            }
        }
    }
    
    info!("All devices validated successfully");
    Ok(device_infos)
}

/// Check if operation would risk data loss
pub fn check_data_loss_risk(operation: &str, array_state: &str) -> MdResult<()> {
    match array_state {
        "degraded" | "failed" => {
            Err(MdError::DataLossRisk(
                format!("Cannot perform {} on array in {} state", operation, array_state)
            ))
        }
        _ => Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_raid_config() {
        assert!(validate_raid_config(0, 2).is_ok());
        assert!(validate_raid_config(1, 2).is_ok());
        assert!(validate_raid_config(5, 3).is_ok());
        assert!(validate_raid_config(6, 4).is_ok());
        
        assert!(validate_raid_config(0, 1).is_err());
        assert!(validate_raid_config(5, 2).is_err());
        assert!(validate_raid_config(10, 3).is_err()); // Odd number
        assert!(validate_raid_config(99, 10).is_err()); // Invalid level
    }
    
    #[test]
    fn test_check_data_loss_risk() {
        assert!(check_data_loss_risk("remove", "active").is_ok());
        assert!(check_data_loss_risk("remove", "degraded").is_err());
        assert!(check_data_loss_risk("stop", "failed").is_err());
    }
}
