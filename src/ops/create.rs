use std::path::{Path, PathBuf};
use std::fs::{self, OpenOptions};
use std::os::linux::fs::MetadataExt;
use std::os::unix::fs::FileTypeExt;
use std::time::{SystemTime, UNIX_EPOCH};
use crate::error::{MdError, MdResult};
use crate::ioctl;
use crate::metadata::{Superblock, v1::{SuperblockV1, MD_SB_MAGIC}};
use crate::validation;
use tracing::{info, warn, debug, instrument};

const DEFAULT_CHUNK_SIZE: i32 = 512 * 1024; // 512K default chunk size
const MD_DEVICE_CREATE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

pub fn chunk_size_kib_to_bytes(chunk_size_kib: u32) -> MdResult<i32> {
    if !(4..=32768).contains(&chunk_size_kib) || !chunk_size_kib.is_power_of_two() {
        return Err(MdError::ConfigValidation(format!(
            "Invalid chunk size: {} KiB (must be a power of 2 between 4 and 32768 KiB)",
            chunk_size_kib
        )));
    }

    i32::try_from(chunk_size_kib * 1024)
        .map_err(|_| MdError::ConfigValidation(format!("Invalid chunk size: {} KiB", chunk_size_kib)))
}

fn ensure_md_device(md_device: &Path, dry_run: bool) -> MdResult<()> {
    if md_device.exists() {
        let metadata = fs::metadata(md_device)?;
        if !metadata.file_type().is_block_device() {
            return Err(MdError::NotBlockDevice(md_device.to_path_buf()));
        }

        let major = ioctl::dev_major(metadata.st_rdev());
        if major != ioctl::MD_MAJOR as i32 {
            return Err(MdError::InvalidDevice(format!(
                "{} is block major {}, not an MD device (major {})",
                md_device.display(),
                major,
                ioctl::MD_MAJOR
            )));
        }

        return Ok(());
    }

    let md_name = md_device
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| MdError::InvalidMetadata("Invalid MD device name".to_string()))?;

    if dry_run {
        info!("[DRY RUN] Would create MD device via sysfs: {}", md_device.display());
        return Ok(());
    }

    info!("Creating MD device via sysfs: {}", md_device.display());
    fs::write("/sys/module/md_mod/parameters/new_array", md_name)
        .map_err(|e| MdError::Sysfs(format!("Failed to create MD device {}: {}", md_device.display(), e)))?;

    let start = std::time::Instant::now();
    while start.elapsed() < MD_DEVICE_CREATE_TIMEOUT {
        if md_device.exists() {
            return ensure_md_device(md_device, false);
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    Err(MdError::Sysfs(format!(
        "MD device {} was requested from sysfs but did not appear",
        md_device.display()
    )))
}

#[instrument(skip(components))]
pub fn run(md_device: &PathBuf, level: u8, raid_devices: u32, metadata_str: &str, components: Vec<PathBuf>, chunk_size: Option<i32>, dry_run: bool) -> MdResult<()> {
    info!(
        "Creating RAID{} array {} with {} devices (dry_run: {})",
        level,
        md_device.display(),
        raid_devices,
        dry_run
    );
    
    if components.len() as u32 != raid_devices {
        return Err(MdError::InsufficientDevices {
            level,
            needed: raid_devices,
            actual: components.len() as u32,
        });
    }

    // Validate all devices comprehensively
    info!("Validating {} component devices", components.len());
    let device_infos = validation::validate_devices_for_array(&components, level)?;
    
    // Warn about unhealthy devices
    for info in &device_infos {
        if !info.smart_healthy {
            warn!("Device {} has SMART health issues - proceed with caution", info.path.display());
        }
    }

    let minor_version: u32 = match metadata_str {
        "1.0" => 0,
        "1.1" => 1,
        "1.2" => 2,
        _ => {
            warn!("Unknown metadata version '{}', defaulting to 1.2", metadata_str);
            2
        }
    };
    
    debug!("Using metadata version 1.{}", minor_version);

    ensure_md_device(md_device, dry_run)?;

    // Generate a random UUID
    let uuid = uuid::Uuid::new_v4();
    let uuid_bytes = *uuid.as_bytes();
    debug!("Generated UUID for array: {}", uuid);
    
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| MdError::InvalidMetadata(format!("System time error: {}", e)))?
        .as_secs();
    
    info!("Creating superblocks on {} devices", components.len());

    for (i, comp) in components.iter().enumerate() {
        debug!("Processing device {}/{}: {}", i + 1, components.len(), comp.display());
        
        // Get device size
        let comp_file = OpenOptions::new().read(true).open(comp)?;
        let device_size = {
            let mut size = 0u64;
            use std::os::fd::AsRawFd;
            if unsafe { ioctl::blkgetsize64(comp_file.as_raw_fd(), &mut size) }.is_ok() {
                size
            } else {
                comp_file.metadata()?.len()
            }
        };
        
        // Calculate offsets based on metadata version
        let (data_offset, super_offset, data_size) = match minor_version {
            0 => {
                // 1.0: superblock at end, data at start
                let sb_offset = (device_size & !0x1FFF).saturating_sub(8192) / 512;
                (0, sb_offset, sb_offset)
            },
            1 => {
                // 1.1: superblock at start, data after
                let data_off = 8192 / 512; // 8K in sectors
                (data_off, 0, (device_size / 512).saturating_sub(data_off))
            },
            2 | _ => {
                // 1.2: superblock at 4K, data after
                let data_off = 8192 / 512; // 8K in sectors  
                (data_off, 8, (device_size / 512).saturating_sub(data_off))
            },
        };
        
        // Create device roles array - all devices get the same complete array layout
        let dev_roles: Vec<u16> = (0..raid_devices).map(|idx| idx as u16).collect();
        let chunk_size_bytes = chunk_size.unwrap_or(DEFAULT_CHUNK_SIZE);
        let chunk_size_sectors = chunk_size_bytes / 512;
        
        // Write superblock
        let sb = SuperblockV1 {
            magic: MD_SB_MAGIC,
            major_version: 1,
            feature_map: 0,
            pad0: 0,
            set_uuid: uuid_bytes,
            set_name: [0; 32],
            ctime: now,
            utime: now,
            level: level as i32,
            layout: 2, // RAID5 left-symmetric
            size: data_size / raid_devices as u64,
            chunksize: chunk_size_sectors,
            raid_disks: raid_devices as i32,
            bitmap_offset: 0,
            new_level: level as i32,
            reshape_position: u64::MAX, // Not reshaping
            delta_disks: 0,
            new_layout: 0,
            new_chunk: chunk_size_sectors,
            new_offset: 0,
            data_offset,
            data_size,
            super_offset,
            recovery_offset: u64::MAX, // Fully recovered
            dev_number: i as u32,
            cnt_corrected_read: 0,
            device_uuid: [0; 16],
            devflags: 0,
            bblog_shift: 0,
            bblog_size: 0,
            bblog_offset: 0,
            dev_roles,
            sb_csum: 0, // Will be calculated
            events: 0,
            resync_offset: u64::MAX, // Mark as clean/in-sync
            pad3: [0; 32],
            max_dev: raid_devices,
            minor_version,
            pad_bytes: Vec::new(),
        };
        
        // Note: Checksum will be calculated by write_to_disk if needed

        if dry_run {
            info!("[DRY RUN] Would write superblock to {}", comp.display());
        } else {
            debug!("Writing superblock to {}", comp.display());
            sb.write_to_disk(comp)
                .map_err(|e| e.context(format!("Failed to write superblock to {}", comp.display())))?;
            info!("Superblock written to {}", comp.display());
        }
    }

    if dry_run {
        info!("[DRY RUN] Would trigger kernel auto-assembly");
        info!("[DRY RUN] Array creation simulation completed successfully");
    } else {
        info!("Assembling and starting {} via MD ioctls", md_device.display());
        super::assemble::run(md_device, components, false)?;
        info!("Array {} created and started successfully with UUID {}", md_device.display(), uuid);
    }
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_chunk_size_from_kib_to_kernel_bytes() {
        assert_eq!(chunk_size_kib_to_bytes(512).unwrap(), 512 * 1024);
        assert_eq!(chunk_size_kib_to_bytes(4).unwrap(), 4 * 1024);
        assert_eq!(chunk_size_kib_to_bytes(32768).unwrap(), 32768 * 1024);
    }

    #[test]
    fn rejects_invalid_chunk_sizes() {
        assert!(chunk_size_kib_to_bytes(0).is_err());
        assert!(chunk_size_kib_to_bytes(2).is_err());
        assert!(chunk_size_kib_to_bytes(100).is_err());
        assert!(chunk_size_kib_to_bytes(65536).is_err());
    }
}
