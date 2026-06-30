use std::path::PathBuf;
use std::fs::OpenOptions;
use std::os::fd::AsRawFd;
use crate::error::{MdError, MdResult};
use crate::ioctl::{self, MduArrayInfo, MduDiskInfo, MD_DISK_ACTIVE, MD_DISK_SYNC};
use crate::metadata::{Superblock, v1::SuperblockV1};
use rayon::prelude::*;
use tracing::{info, debug, instrument};

#[instrument(skip(components))]
pub fn run(md_device: &PathBuf, components: Vec<PathBuf>, dry_run: bool) -> MdResult<()> {
    info!(
        "Assembling {} from {} components (dry_run: {})",
        md_device.display(),
        components.len(),
        dry_run
    );
    
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
    
    // We should parse the first component's superblock to get array info
    if components.is_empty() {
        return Err(MdError::InsufficientDevices {
            level: 0,
            needed: 1,
            actual: 0,
        });
    }
    
    debug!("Loading superblock from first component: {}", components[0].display());
    let first_sb = SuperblockV1::load(&components[0])
        .map_err(|e| e.context(format!("Failed to load superblock from {}", components[0].display())))?;
    
    info!(
        "Array configuration: RAID{}, {} disks, chunk size: {} KB",
        first_sb.level,
        first_sb.raid_disks,
        first_sb.chunksize / 1024
    );
    
    array_info.level = first_sb.level;
    array_info.size = first_sb.size as i32;
    array_info.nr_disks = first_sb.raid_disks;
    array_info.raid_disks = first_sb.raid_disks;
    array_info.md_minor = 0;
    array_info.not_persistent = 0;
    array_info.state = 0;
    array_info.active_disks = 0;
    array_info.working_disks = 0;
    array_info.failed_disks = 0;
    array_info.spare_disks = 0;
    array_info.layout = first_sb.layout;
    array_info.chunk_size = first_sb.chunksize;

    if dry_run {
        info!("[DRY RUN] Would set array info");
    } else {
        debug!("Setting array info: level={}, raid_disks={}, chunk_size={}", 
               array_info.level, array_info.raid_disks, array_info.chunk_size);
        unsafe {
            ioctl::set_array_info(md_file.as_raw_fd(), &mut array_info as *mut _)
                .map_err(|e| MdError::Nix(e).context("Failed to set array info"))?;
        }
        info!("Array info set successfully");
    }

    info!("Loading and validating superblocks from all components");
    let loaded_sbs: Result<Vec<_>, MdError> = components
        .par_iter()
        .map(|comp| {
            debug!("Loading superblock from {}", comp.display());
            SuperblockV1::load(comp)
                .map(|sb| (comp, sb))
                .map_err(|e| e.context(format!("Failed to load superblock from {}", comp.display())))
        })
        .collect();
    
    let loaded_sbs = loaded_sbs?;
    info!("Successfully loaded {} superblocks", loaded_sbs.len());

    for (i, (comp, sb)) in loaded_sbs.iter().enumerate() {
        if sb.set_uuid != first_sb.set_uuid {
            info!("UUID mismatch on {}: expected {:?}, found {:?}", 
                  comp.display(), first_sb.set_uuid, sb.set_uuid);
            return Err(MdError::UuidMismatch {
                expected: format!("{:?}", first_sb.set_uuid),
                found: format!("{:?}", sb.set_uuid),
            });
        }
        
        debug!("Component {}/{}: {} - UUID matches", i + 1, loaded_sbs.len(), comp.display());

        let comp_meta = std::fs::metadata(comp).map_err(MdError::Io)?;
        use std::os::linux::fs::MetadataExt;
        let rdev = comp_meta.st_rdev();
        
        let mut disk_info = MduDiskInfo::default();
        disk_info.number = i as i32;
        disk_info.raid_disk = i as i32;
        disk_info.state = MD_DISK_ACTIVE | MD_DISK_SYNC;
        disk_info.major = ((rdev >> 8) & 0xff) as i32;
        disk_info.minor = (rdev & 0xff) as i32;

        if dry_run {
            info!("[DRY RUN] Would ADD_NEW_DISK {} (major={}, minor={})", 
                  comp.display(), disk_info.major, disk_info.minor);
        } else {
            debug!("Adding disk {} to array (major={}, minor={})", 
                   comp.display(), disk_info.major, disk_info.minor);
            unsafe {
                ioctl::add_new_disk(md_file.as_raw_fd(), &mut disk_info as *mut _)
                    .map_err(|e| MdError::Nix(e).context(format!("Failed to add disk {}", comp.display())))?;
            }
            info!("Successfully added device {}/{}: {}", i + 1, loaded_sbs.len(), comp.display());
        }
    }

    if dry_run {
        info!("[DRY RUN] Would RUN_ARRAY");
        info!("[DRY RUN] Array assembly simulation completed successfully");
    } else {
        info!("Starting array");
        unsafe {
            ioctl::run_array(md_file.as_raw_fd())
                .map_err(|e| MdError::Nix(e).context("Failed to start array"))?;
        }
        info!("Array {} assembled and started successfully", md_device.display());
    }
    
    Ok(())
}
