use std::path::PathBuf;
use std::os::fd::AsRawFd;
use std::fs::OpenOptions;
use crate::error::MdError;
use crate::ioctl;
use crate::sysfs::MdSysfs;
use crate::validation;

pub fn stop(md_device: &PathBuf, force: bool, dry_run: bool) -> Result<(), MdError> {
    println!("Stopping array: {}", md_device.display());
    
    let name = md_device.file_name().unwrap_or_default().to_string_lossy().to_string();
    if !force {
        let sysfs = MdSysfs::new(&name);
        if let Ok(state) = sysfs.get_array_state() {
            let state_str = state.to_string();
            validation::check_data_loss_risk("stop", &state_str)?;
        }
    }
    
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(md_device)
        .map_err(MdError::Io)?;

    if dry_run {
        println!("  [DRY RUN] Would stop array via ioctl");
    } else {
        unsafe {
            ioctl::stop_array(file.as_raw_fd()).map_err(MdError::Nix)?;
        }
    }
    
    println!("Array stopped successfully.");
    Ok(())
}

fn get_rdev(path: &PathBuf) -> Result<libc::c_int, MdError> {
    use std::os::linux::fs::MetadataExt;
    let meta = std::fs::metadata(path).map_err(MdError::Io)?;
    Ok(meta.st_rdev() as libc::c_int)
}

pub fn manage(
    md_device: &PathBuf,
    add: Option<Vec<PathBuf>>,
    remove: Option<Vec<PathBuf>>,
    fail: Option<Vec<PathBuf>>,
    force: bool,
    dry_run: bool,
) -> Result<(), MdError> {
    let name = md_device.file_name().unwrap_or_default().to_string_lossy().to_string();
    let sysfs = MdSysfs::new(&name);
    let state_str = sysfs.get_array_state().map(|s| s.to_string()).unwrap_or_else(|_| "unknown".to_string());

    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(md_device)
        .map_err(MdError::Io)?;

    if let Some(devs) = fail {
        if !force {
            validation::check_data_loss_risk("fail", &state_str)?;
        }
        for dev in devs {
            let rdev = get_rdev(&dev)?;
            if dry_run {
                println!("  [DRY RUN] Would set disk faulty: {}", dev.display());
            } else {
                unsafe {
                    ioctl::set_disk_faulty(file.as_raw_fd(), rdev).map_err(MdError::Nix)?;
                }
            }
            println!("Failed device: {}", dev.display());
        }
    }

    if let Some(devs) = remove {
        if !force {
            validation::check_data_loss_risk("remove", &state_str)?;
        }
        for dev in devs {
            let rdev = get_rdev(&dev)?;
            if dry_run {
                println!("  [DRY RUN] Would hot remove disk: {}", dev.display());
            } else {
                unsafe {
                    ioctl::hot_remove_disk(file.as_raw_fd(), rdev).map_err(MdError::Nix)?;
                }
            }
            println!("Removed device: {}", dev.display());
        }
    }

    if let Some(devs) = add {
        for dev in devs {
            let rdev = get_rdev(&dev)?;
            if dry_run {
                println!("  [DRY RUN] Would hot add disk: {}", dev.display());
            } else {
                unsafe {
                    ioctl::hot_add_disk(file.as_raw_fd(), rdev).map_err(MdError::Nix)?;
                }
            }
            println!("Added device: {}", dev.display());
        }
    }

    Ok(())
}
