use std::path::PathBuf;
use std::fs::{self, OpenOptions};
use std::os::fd::AsRawFd;
use std::os::linux::fs::MetadataExt;
use std::os::unix::fs::FileTypeExt;
use crate::error::{MdError, MdResult};
use crate::ioctl::{self, MduArrayInfo, MduDiskInfo, MD_DISK_ACTIVE, MD_DISK_SYNC};
use crate::metadata::{Superblock, v1::SuperblockV1};
use rayon::prelude::*;
use tracing::{info, debug, instrument};

const MD_DEVICE_CREATE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

pub fn auto(seed_device: &PathBuf, dry_run: bool) -> MdResult<()> {
    let seed_sb = SuperblockV1::load(seed_device)
        .map_err(|e| e.context(format!("Failed to load superblock from {}", seed_device.display())))?;
    let components = find_components_by_uuid(seed_device, seed_sb.set_uuid)?;
    if components.len() < seed_sb.raid_disks as usize {
        return Err(MdError::InsufficientDevices {
            level: seed_sb.level as u8,
            needed: seed_sb.raid_disks as u32,
            actual: components.len() as u32,
        });
    }

    let md_name = array_name_from_superblock(&seed_sb);
    let md_device = PathBuf::from(format!("/dev/{}", md_name));
    ensure_md_device(&md_device, &md_name, dry_run)?;
    run(&md_device, components, dry_run)
}

fn array_name_from_superblock(sb: &SuperblockV1) -> String {
    let name = String::from_utf8_lossy(&sb.set_name)
        .trim_matches(char::from(0))
        .trim()
        .to_string();
    if !name.is_empty() {
        name
    } else {
        let prefix = sb
            .set_uuid
            .iter()
            .take(4)
            .map(|byte| format!("{:02x}", byte))
            .collect::<String>();
        format!("md-{}", prefix)
    }
}

fn find_components_by_uuid(seed_device: &PathBuf, uuid: [u8; 16]) -> MdResult<Vec<PathBuf>> {
    let mut components = vec![seed_device.clone()];

    for entry in fs::read_dir("/dev").map_err(MdError::Io)? {
        let path = entry.map_err(MdError::Io)?.path();
        if path == *seed_device || !is_candidate_block_device(&path) {
            continue;
        }

        if let Ok(sb) = SuperblockV1::load(&path) {
            if sb.set_uuid == uuid {
                components.push(path);
            }
        }
    }

    components.sort();
    components.dedup();
    Ok(components)
}

fn is_candidate_block_device(path: &PathBuf) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| {
            name.starts_with("sd")
                || name.starts_with("vd")
                || name.starts_with("xvd")
                || name.starts_with("nvme")
                || name.starts_with("loop")
        })
        .unwrap_or(false)
        && fs::metadata(path)
            .map(|metadata| metadata.file_type().is_block_device())
            .unwrap_or(false)
}

fn ensure_md_device(md_device: &PathBuf, md_name: &str, dry_run: bool) -> MdResult<()> {
    if md_device.exists() || dry_run {
        return Ok(());
    }

    fs::write("/sys/module/md_mod/parameters/new_array", md_name)
        .map_err(|e| MdError::Sysfs(format!("Failed to create MD device {}: {}", md_device.display(), e)))?;

    let start = std::time::Instant::now();
    while start.elapsed() < MD_DEVICE_CREATE_TIMEOUT {
        if md_device.exists() {
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    Err(MdError::Sysfs(format!("MD device {} did not appear", md_device.display())))
}

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
        first_sb.chunk_size_bytes() / 1024
    );
    
    let md_rdev = md_file.metadata().map_err(MdError::Io)?.st_rdev();

    array_info.major_version = first_sb.major_version as i32;
    array_info.minor_version = first_sb.minor_version as i32;
    array_info.patch_version = 0;
    array_info.ctime = first_sb.ctime as u32;
    array_info.utime = first_sb.utime as u32;
    array_info.level = first_sb.level;
    array_info.size = first_sb.size as i32;
    array_info.nr_disks = first_sb.raid_disks;
    array_info.raid_disks = first_sb.raid_disks;
    array_info.md_minor = ioctl::dev_minor(md_rdev);
    array_info.not_persistent = 0;
    array_info.state = 0;
    array_info.active_disks = components.len() as i32;
    array_info.working_disks = components.len() as i32;
    array_info.failed_disks = 0;
    array_info.spare_disks = 0;
    array_info.layout = first_sb.layout;
    array_info.chunk_size = first_sb.chunk_size_bytes();

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
        let rdev = comp_meta.st_rdev();
        
        let mut disk_info = MduDiskInfo::default();
        disk_info.number = i as i32;
        disk_info.raid_disk = i as i32;
        disk_info.state = ioctl::disk_state(&[MD_DISK_ACTIVE, MD_DISK_SYNC]);
        disk_info.major = ioctl::dev_major(rdev);
        disk_info.minor = ioctl::dev_minor(rdev);

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
        let mut param = ioctl::MduParam::default();
        unsafe {
            ioctl::run_array(md_file.as_raw_fd(), &mut param as *mut _)
                .map_err(|e| MdError::Nix(e).context("Failed to start array"))?;
        }
        info!("Array {} assembled and started successfully", md_device.display());
    }
    
    Ok(())
}
