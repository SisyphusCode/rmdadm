#![allow(dead_code)]
//! Low-level kernel ioctl bindings for MD devices.
//! Refer to linux/major.h and linux/raid/md_u.h

use nix::{ioctl_none, ioctl_read, ioctl_write_ptr, ioctl_write_int_bad};

pub const MD_MAJOR: u32 = 9;

// MD ioctl magic number
const MD_MAGIC: u8 = 0x09;

// Define a few core ioctls as a baseline
ioctl_none!(run_array, MD_MAGIC, 0x30);
ioctl_none!(stop_array, MD_MAGIC, 0x32);
ioctl_none!(stop_array_ro, MD_MAGIC, 0x33);
ioctl_none!(restart_array_rw, MD_MAGIC, 0x34);

// Structs for array info would go here, mimicking mdu_array_info_t
#[repr(C)]
#[derive(Default, Debug, Clone)]
pub struct MduArrayInfo {
    pub major_version: i32,
    pub minor_version: i32,
    pub patch_version: i32,
    pub ctime: u32,
    pub level: i32,
    pub size: i32,
    pub nr_disks: i32,
    pub raid_disks: i32,
    pub md_minor: i32,
    pub not_persistent: i32,
    pub state: i32,
    pub active_disks: i32,
    pub working_disks: i32,
    pub failed_disks: i32,
    pub spare_disks: i32,
    pub layout: i32,
    pub chunk_size: i32,
}

ioctl_read!(get_array_info, MD_MAGIC, 0x11, MduArrayInfo);
ioctl_write_ptr!(set_array_info, MD_MAGIC, 0x23, MduArrayInfo);
ioctl_write_ptr!(add_new_disk, MD_MAGIC, 0x21, MduDiskInfo);
ioctl_read!(get_disk_info, MD_MAGIC, 0x12, MduDiskInfo);

use nix::request_code_none;
ioctl_write_int_bad!(hot_remove_disk, request_code_none!(MD_MAGIC, 0x22));
ioctl_write_int_bad!(hot_add_disk, request_code_none!(MD_MAGIC, 0x28));
ioctl_write_int_bad!(set_disk_faulty, request_code_none!(MD_MAGIC, 0x29));

ioctl_read!(blkgetsize64, 0x12, 114, u64);

#[repr(C)]
#[derive(Debug, Default)]
pub struct MduDiskInfo {
    pub number: i32,
    pub major: i32,
    pub minor: i32,
    pub raid_disk: i32,
    pub state: i32,
}

// Disk state flags
pub const MD_DISK_FAULTY: i32 = 0;
pub const MD_DISK_ACTIVE: i32 = 1;
pub const MD_DISK_SYNC: i32 = 2;
pub const MD_DISK_REMOVED: i32 = 3;
pub const MD_DISK_WRITEMOSTLY: i32 = 9;
