use std::path::PathBuf;
use std::fs::File;
use std::os::fd::AsRawFd;
use crate::error::MdError;
use crate::sysfs::MdSysfs;
use crate::ioctl::{self, MduArrayInfo};
use serde::Serialize;

#[derive(Serialize)]
struct ArrayDetail {
    device: String,
    state: String,
    version: String,
    level: String,
    size: u64,
    raid_disks: i32,
    total_disks: i32,
    active_disks: i32,
    working_disks: i32,
    failed_disks: i32,
    spare_disks: i32,
}

pub fn run(md_device: &PathBuf, json: bool) -> Result<(), MdError> {
    let dev_name = md_device.file_name()
        .ok_or_else(|| MdError::InvalidMetadata("Invalid device path".into()))?
        .to_str()
        .ok_or_else(|| MdError::InvalidMetadata("Invalid device name".into()))?;

    let sys = MdSysfs::new(dev_name);
    let state_str = match sys.get_array_state() {
        Ok(state) => state.to_string(),
        Err(e) => format!("Unknown ({})", e),
    };

    if !json {
        println!("{}:", md_device.display());
        println!("      State : {}", state_str);
    }

    let file = match File::open(md_device) {
        Ok(f) => f,
        Err(e) => {
            println!("      Error : Could not open device ({})", e);
            return Ok(()); // Soft fail to continue showing other info if possible
        }
    };
    
    let mut info = MduArrayInfo::default();

    unsafe {
        if let Err(e) = ioctl::get_array_info(file.as_raw_fd(), &mut info as *mut _) {
            println!("      Error : Could not get array info ({})", e);
            return Ok(());
        }
    }

    if json {
        let detail = ArrayDetail {
            device: dev_name.to_string(),
            state: state_str,
            version: format!("{}.{}.{}", info.major_version, info.minor_version, info.patch_version),
            level: format!("raid{}", info.level),
            size: info.size as u64,
            raid_disks: info.raid_disks,
            total_disks: info.nr_disks,
            active_disks: info.active_disks,
            working_disks: info.working_disks,
            failed_disks: info.failed_disks,
            spare_disks: info.spare_disks,
        };
        println!("{}", serde_json::to_string_pretty(&detail).unwrap());
    } else {
        println!("    Version : {}.{}.{}", info.major_version, info.minor_version, info.patch_version);
        println!(" RAID Level : raid{}", info.level);
        println!(" Array Size : {}", info.size);
        println!(" Raid Disks : {}", info.raid_disks);
        println!("Total Disks : {}", info.nr_disks);
        println!("State Flags : {}", info.state);
        println!("Active Disks: {}", info.active_disks);
        println!("Working Disks: {}", info.working_disks);
        println!("Failed Disks: {}", info.failed_disks);
        println!(" Spare Disks: {}", info.spare_disks);
    }

    Ok(())
}
