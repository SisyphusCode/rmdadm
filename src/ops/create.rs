use std::path::PathBuf;
use std::fs::OpenOptions;
use std::os::fd::AsRawFd;
use std::time::{SystemTime, UNIX_EPOCH};
use crate::error::{MdError, MdResult};
use crate::ioctl::{self, MduArrayInfo, MduDiskInfo, MD_DISK_ACTIVE, MD_DISK_SYNC};
use crate::metadata::{Superblock, v1::{SuperblockV1, MD_SB_MAGIC}};
use crate::validation;
use tracing::{info, warn, debug, instrument};
use std::os::linux::fs::MetadataExt;

const DEFAULT_CHUNK_SIZE: i32 = 512 * 1024; // 512K default chunk size

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

    let md_file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(md_device)
        .map_err(|e| {
            MdError::Io(std::io::Error::new(
                e.kind(),
                format!("Failed to open MD device {}: {}", md_device.display(), e)
            ))
        })?;

    let mut array_info = MduArrayInfo::default();
    array_info.level = level as i32;
    array_info.size = 0; // Kernel will figure this out or we can compute it
    array_info.nr_disks = raid_devices as i32;
    array_info.raid_disks = raid_devices as i32;
    array_info.md_minor = 0;
    array_info.not_persistent = 0;
    array_info.state = 0;
    array_info.active_disks = 0;
    array_info.working_disks = 0;
    array_info.failed_disks = 0;
    array_info.spare_disks = 0;
    array_info.layout = 0;
    array_info.chunk_size = chunk_size.unwrap_or(DEFAULT_CHUNK_SIZE);

    if dry_run {
        info!("[DRY RUN] Would SET_ARRAY_INFO for {}", md_device.display());
    } else {
        debug!("Setting array info: level={}, raid_disks={}, chunk_size={}", 
               array_info.level, array_info.raid_disks, array_info.chunk_size);
        unsafe {
            ioctl::set_array_info(md_file.as_raw_fd(), &mut array_info as *mut _)
                .map_err(|e| MdError::Nix(e).context("Failed to set array info"))?;
        }
        info!("Array info set successfully");
    }

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
            layout: 0,
            size: 0, 
            chunksize: chunk_size.unwrap_or(DEFAULT_CHUNK_SIZE),
            raid_disks: raid_devices as i32,
            bitmap_offset: 0,
            new_level: level as i32,
            reshape_position: 0,
            delta_disks: 0,
            new_layout: 0,
            new_chunk: chunk_size.unwrap_or(DEFAULT_CHUNK_SIZE),
            new_offset: 0,
            data_offset: 0,
            data_size: 0,
            super_offset: 0,
            recovery_offset: 0,
            dev_number: i as u32,
            cnt_corrected_read: 0,
            dev_roles: Vec::new(),
            sb_csum: 0,
            events: 0,
            resync_offset: 0,
            bblog_shift: 0,
            bblog_size: 0,
            bblog_offset: 0,
            max_dev: raid_devices,
            minor_version,
            pad_bytes: Vec::new(),
        };

        // Find major/minor for component
        let comp_meta = std::fs::metadata(comp)
            .map_err(|e| MdError::Io(std::io::Error::new(
                e.kind(),
                format!("Failed to get metadata for {}: {}", comp.display(), e)
            )))?;
        let rdev = comp_meta.st_rdev();
        
        let mut disk_info = MduDiskInfo::default();
        disk_info.number = i as i32;
        disk_info.raid_disk = i as i32;
        disk_info.state = MD_DISK_ACTIVE | MD_DISK_SYNC;
        disk_info.major = ((rdev >> 8) & 0xff) as i32;
        disk_info.minor = (rdev & 0xff) as i32;

        if dry_run {
            info!("[DRY RUN] Would write superblock to {}", comp.display());
            info!("[DRY RUN] Would ADD_NEW_DISK {} (major={}, minor={}) to array", 
                  comp.display(), disk_info.major, disk_info.minor);
        } else {
            debug!("Writing superblock to {}", comp.display());
            sb.write_to_disk(comp)
                .map_err(|e| e.context(format!("Failed to write superblock to {}", comp.display())))?;
            
            debug!("Adding disk {} to array (major={}, minor={})", 
                   comp.display(), disk_info.major, disk_info.minor);
            unsafe {
                ioctl::add_new_disk(md_file.as_raw_fd(), &mut disk_info as *mut _)
                    .map_err(|e| MdError::Nix(e).context(format!("Failed to add disk {}", comp.display())))?;
            }
            info!("Successfully added device {}/{}: {}", i + 1, components.len(), comp.display());
        }
    }

    if dry_run {
        info!("[DRY RUN] Would RUN_ARRAY");
        info!("[DRY RUN] Array creation simulation completed successfully");
    } else {
        info!("Starting array");
        unsafe {
            ioctl::run_array(md_file.as_raw_fd())
                .map_err(|e| MdError::Nix(e).context("Failed to start array"))?;
        }
        info!("Array {} created and started successfully with UUID {}", md_device.display(), uuid);
    }
    
    Ok(())
}
