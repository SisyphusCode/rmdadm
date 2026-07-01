//! Write-intent bitmap support for RAID arrays
//! Bitmaps track which blocks have been written, enabling faster resync after crashes

use std::path::Path;
use tracing::{info, warn};
use crate::error::MdError;
use crate::sysfs::MdSysfs;

/// Bitmap location types
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum BitmapLocation {
    /// Internal bitmap (stored in array metadata)
    Internal,
    /// External bitmap (stored in separate file)
    External,
    /// No bitmap
    None,
}

/// Bitmap configuration
#[derive(Debug, Clone)]
pub struct BitmapConfig {
    /// Bitmap location
    pub location: BitmapLocation,
    /// Chunk size in KB (granularity of bitmap tracking)
    pub chunk_size: Option<u32>,
    /// External bitmap file path (if using external bitmap)
    pub file_path: Option<String>,
    /// Delay before marking blocks as clean (in seconds)
    pub write_behind: Option<u32>,
}

impl Default for BitmapConfig {
    fn default() -> Self {
        Self {
            location: BitmapLocation::Internal,
            chunk_size: Some(64), // 64KB default
            file_path: None,
            write_behind: None,
        }
    }
}

/// Add bitmap to an existing array
pub fn add_bitmap(
    md_device: &Path,
    config: BitmapConfig,
    dry_run: bool,
) -> Result<(), MdError> {
    info!("Adding bitmap to array: {}", md_device.display());
    
    // Validate array exists
    if !md_device.exists() {
        return Err(MdError::DeviceNotFound(md_device.to_string_lossy().to_string()));
    }
    
    let array_name = md_device
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| MdError::InvalidDevice("Invalid device name".to_string()))?;
    
    let sys = MdSysfs::new(array_name);
    
    // Check if bitmap already exists
    if has_bitmap(&sys)? {
        warn!("Array already has a bitmap");
        return Ok(());
    }
    
    // Validate configuration
    validate_bitmap_config(&config)?;
    
    if dry_run {
        info!("DRY RUN: Would add bitmap with config: {:?}", config);
        print_bitmap_plan(&config)?;
        return Ok(());
    }
    
    // Add bitmap based on location type
    match config.location {
        BitmapLocation::Internal => add_internal_bitmap(&sys, &config)?,
        BitmapLocation::External => add_external_bitmap(&sys, &config)?,
        BitmapLocation::None => {
            return Err(MdError::ConfigValidation(
                "Cannot add bitmap with location 'None'".to_string()
            ));
        }
    }
    
    info!("✅ Bitmap added successfully");
    Ok(())
}

/// Remove bitmap from an array
pub fn remove_bitmap(md_device: &Path, dry_run: bool) -> Result<(), MdError> {
    info!("Removing bitmap from array: {}", md_device.display());
    
    if !md_device.exists() {
        return Err(MdError::DeviceNotFound(md_device.to_string_lossy().to_string()));
    }
    
    let array_name = md_device
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| MdError::InvalidDevice("Invalid device name".to_string()))?;
    
    let sys = MdSysfs::new(array_name);
    
    // Check if bitmap exists
    if !has_bitmap(&sys)? {
        warn!("Array does not have a bitmap");
        return Ok(());
    }
    
    if dry_run {
        info!("DRY RUN: Would remove bitmap from array");
        return Ok(());
    }
    
    // Remove bitmap by writing "none" to bitmap/location
    sys.write_sysfs_value("bitmap/location", "none")
        .map_err(|e| MdError::Bitmap(format!("Failed to remove bitmap: {}", e)))?;
    
    info!("✅ Bitmap removed successfully");
    Ok(())
}

/// Get bitmap information for an array
pub fn get_bitmap_info(md_device: &Path) -> Result<BitmapInfo, MdError> {
    let array_name = md_device
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| MdError::InvalidDevice("Invalid device name".to_string()))?;
    
    let sys = MdSysfs::new(array_name);
    
    let has_bitmap = has_bitmap(&sys)?;
    
    if !has_bitmap {
        return Ok(BitmapInfo {
            enabled: false,
            location: BitmapLocation::None,
            chunk_size: None,
            file_path: None,
            pages: None,
            dirty_pages: None,
        });
    }
    
    // Read bitmap location
    let location_str = sys.read_sysfs_value("bitmap/location")
        .unwrap_or_else(|_| "unknown".to_string());
    
    let location = match location_str.as_str() {
        "none" => BitmapLocation::None,
        loc if loc.starts_with('+') || loc.starts_with('-') => BitmapLocation::Internal,
        _ => BitmapLocation::External,
    };
    
    // Read chunk size
    let chunk_size = sys.read_sysfs_value("bitmap/chunksize")
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
        .map(|bytes| bytes / 1024); // Convert to KB
    
    // Read bitmap statistics
    let pages = sys.read_sysfs_value("bitmap/metadata/pages")
        .ok()
        .and_then(|s| s.parse::<u64>().ok());
    
    let dirty_pages = sys.read_sysfs_value("bitmap/metadata/dirty_pages")
        .ok()
        .and_then(|s| s.parse::<u64>().ok());
    
    let file_path = if location == BitmapLocation::External {
        sys.read_sysfs_value("bitmap/location").ok()
    } else {
        None
    };
    
    Ok(BitmapInfo {
        enabled: true,
        location,
        chunk_size,
        file_path,
        pages,
        dirty_pages,
    })
}

/// Bitmap information
#[derive(Debug, Clone, serde::Serialize)]
pub struct BitmapInfo {
    pub enabled: bool,
    pub location: BitmapLocation,
    pub chunk_size: Option<u32>,
    pub file_path: Option<String>,
    pub pages: Option<u64>,
    pub dirty_pages: Option<u64>,
}

/// Check if array has a bitmap
fn has_bitmap(sys: &MdSysfs) -> Result<bool, MdError> {
    match sys.read_sysfs_value("bitmap/location") {
        Ok(loc) => Ok(loc.trim() != "none"),
        Err(_) => Ok(false),
    }
}

/// Validate bitmap configuration
fn validate_bitmap_config(config: &BitmapConfig) -> Result<(), MdError> {
    // Validate chunk size
    if let Some(chunk_size) = config.chunk_size {
        if chunk_size < 4 || chunk_size > 2048 {
            return Err(MdError::ConfigValidation(
                format!("Invalid bitmap chunk size: {} KB (must be 4-2048 KB)", chunk_size)
            ));
        }
        if !chunk_size.is_power_of_two() {
            return Err(MdError::ConfigValidation(
                format!("Bitmap chunk size must be power of 2: {}", chunk_size)
            ));
        }
    }
    
    // Validate external bitmap has file path
    if config.location == BitmapLocation::External && config.file_path.is_none() {
        return Err(MdError::ConfigValidation(
            "External bitmap requires file_path".to_string()
        ));
    }
    
    Ok(())
}

/// Print bitmap plan
fn print_bitmap_plan(config: &BitmapConfig) -> Result<(), MdError> {
    println!("\n📋 Bitmap Configuration:");
    println!("========================");
    
    match config.location {
        BitmapLocation::Internal => println!("Location: Internal (stored in array metadata)"),
        BitmapLocation::External => {
            println!("Location: External");
            if let Some(ref path) = config.file_path {
                println!("File Path: {}", path);
            }
        }
        BitmapLocation::None => println!("Location: None"),
    }
    
    if let Some(chunk_size) = config.chunk_size {
        println!("Chunk Size: {} KB", chunk_size);
        println!("  (Smaller = more accurate, larger = less overhead)");
    }
    
    if let Some(write_behind) = config.write_behind {
        println!("Write-Behind Delay: {} seconds", write_behind);
    }
    
    println!("\n💡 Benefits:");
    println!("  - Faster resync after unclean shutdown");
    println!("  - Only dirty regions need to be resynced");
    println!("  - Minimal performance impact");
    println!();
    
    Ok(())
}

/// Add internal bitmap
fn add_internal_bitmap(sys: &MdSysfs, config: &BitmapConfig) -> Result<(), MdError> {
    info!("Adding internal bitmap");
    
    // Set chunk size if specified
    if let Some(chunk_size) = config.chunk_size {
        let chunk_bytes = chunk_size * 1024;
        sys.write_sysfs_value("bitmap/chunksize", &chunk_bytes.to_string())
            .map_err(|e| MdError::Bitmap(format!("Failed to set chunk size: {}", e)))?;
    }
    
    // Enable internal bitmap by writing "internal" or offset
    sys.write_sysfs_value("bitmap/location", "+0")
        .map_err(|e| MdError::Bitmap(format!("Failed to enable internal bitmap: {}", e)))?;
    
    // Set write-behind if specified
    if let Some(write_behind) = config.write_behind {
        sys.write_sysfs_value("bitmap/time_base", &write_behind.to_string())
            .map_err(|e| MdError::Bitmap(format!("Failed to set write-behind: {}", e)))?;
    }
    
    Ok(())
}

/// Add external bitmap
fn add_external_bitmap(sys: &MdSysfs, config: &BitmapConfig) -> Result<(), MdError> {
    info!("Adding external bitmap");
    
    let file_path = config.file_path.as_ref()
        .ok_or_else(|| MdError::ConfigValidation("External bitmap requires file_path".to_string()))?;
    
    // Set chunk size if specified
    if let Some(chunk_size) = config.chunk_size {
        let chunk_bytes = chunk_size * 1024;
        sys.write_sysfs_value("bitmap/chunksize", &chunk_bytes.to_string())
            .map_err(|e| MdError::Bitmap(format!("Failed to set chunk size: {}", e)))?;
    }
    
    // Enable external bitmap by writing file path
    sys.write_sysfs_value("bitmap/location", file_path)
        .map_err(|e| MdError::Bitmap(format!("Failed to enable external bitmap: {}", e)))?;
    
    // Set write-behind if specified
    if let Some(write_behind) = config.write_behind {
        sys.write_sysfs_value("bitmap/time_base", &write_behind.to_string())
            .map_err(|e| MdError::Bitmap(format!("Failed to set write-behind: {}", e)))?;
    }
    
    Ok(())
}

/// Clear bitmap (mark all blocks as clean)
pub fn clear_bitmap(md_device: &Path) -> Result<(), MdError> {
    info!("Clearing bitmap for array: {}", md_device.display());
    
    let array_name = md_device
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| MdError::InvalidDevice("Invalid device name".to_string()))?;
    
    let sys = MdSysfs::new(array_name);
    
    if !has_bitmap(&sys)? {
        return Err(MdError::Bitmap("Array does not have a bitmap".to_string()));
    }
    
    // Clear bitmap by writing to can_clear
    sys.write_sysfs_value("bitmap/can_clear", "1")
        .map_err(|e| MdError::Bitmap(format!("Failed to clear bitmap: {}", e)))?;
    
    info!("✅ Bitmap cleared successfully");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bitmap_config_default() {
        let config = BitmapConfig::default();
        assert_eq!(config.location, BitmapLocation::Internal);
        assert_eq!(config.chunk_size, Some(64));
    }

    #[test]
    fn test_bitmap_location() {
        assert_eq!(BitmapLocation::Internal, BitmapLocation::Internal);
        assert_ne!(BitmapLocation::Internal, BitmapLocation::External);
    }

    #[test]
    fn test_validate_chunk_size() {
        let mut config = BitmapConfig::default();
        
        // Valid chunk sizes
        config.chunk_size = Some(64);
        assert!(validate_bitmap_config(&config).is_ok());
        
        config.chunk_size = Some(128);
        assert!(validate_bitmap_config(&config).is_ok());
        
        // Invalid: too small
        config.chunk_size = Some(2);
        assert!(validate_bitmap_config(&config).is_err());
        
        // Invalid: not power of 2
        config.chunk_size = Some(100);
        assert!(validate_bitmap_config(&config).is_err());
    }
}
