#![allow(dead_code)]
//! Low-level kernel ioctl bindings for MD devices.
//! Refer to linux/major.h and linux/raid/md_u.h

use nix::{ioctl_none, ioctl_read, ioctl_write_ptr, ioctl_write_int_bad};

pub const MD_MAJOR: u32 = 9;

// MD ioctl magic number
const MD_MAGIC: u8 = 0x09;

// Define a few core ioctls as a baseline
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
    pub utime: u32,
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
ioctl_write_ptr!(run_array, MD_MAGIC, 0x30, MduParam);

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

#[repr(C)]
#[derive(Debug, Default)]
pub struct MduParam {
    pub personality: i32,
    pub chunk_size: i32,
    pub max_fault: i32,
}

// Disk state flags
pub const MD_DISK_FAULTY: i32 = 0;
pub const MD_DISK_ACTIVE: i32 = 1;
pub const MD_DISK_SYNC: i32 = 2;
pub const MD_DISK_REMOVED: i32 = 3;
pub const MD_DISK_WRITEMOSTLY: i32 = 9;

pub fn dev_major(dev: u64) -> i32 {
    (((dev >> 8) & 0x0fff_u64) | ((dev >> 32) & !0x0fff_u64)) as i32
}

pub fn dev_minor(dev: u64) -> i32 {
    ((dev & 0x00ff_u64) | ((dev >> 12) & !0x00ff_u64)) as i32
}

pub fn disk_state(flags: &[i32]) -> i32 {
    flags.iter().fold(0, |state, flag| state | (1 << flag))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::size_of;

    #[test]
    fn ioctl_structs_match_kernel_uapi_sizes() {
        assert_eq!(size_of::<MduArrayInfo>(), 72);
        assert_eq!(size_of::<MduDiskInfo>(), 20);
        assert_eq!(size_of::<MduParam>(), 12);
    }

    #[test]
    fn splits_linux_dev_t_major_and_minor() {
        let dev = (7_u64 << 8) | (259_u64 & 0x00ff) | ((259_u64 & !0x00ff) << 12);

        assert_eq!(dev_major(dev), 7);
        assert_eq!(dev_minor(dev), 259);
    }

    #[test]
    fn disk_state_uses_kernel_bit_positions() {
        assert_eq!(disk_state(&[MD_DISK_ACTIVE, MD_DISK_SYNC]), 0b110);
    }
}
