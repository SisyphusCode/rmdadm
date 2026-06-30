#![allow(dead_code)]
//! Parser for the version 1.x superblock format.
//! Based on Linux kernel md_p.h and md_u.h

use super::Superblock;
use crate::error::{MdError, MdResult};
use std::path::Path;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write, Seek, SeekFrom};
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use tracing::{debug, warn};

pub const MD_SB_MAGIC: u32 = 0xa92b4efc;

// Feature flags
pub const MD_FEATURE_BITMAP_OFFSET: u32 = 1;
pub const MD_FEATURE_RECOVERY_OFFSET: u32 = 2;
pub const MD_FEATURE_RESHAPE_ACTIVE: u32 = 4;
pub const MD_FEATURE_BAD_BLOCKS: u32 = 8;
pub const MD_FEATURE_REPLACEMENT: u32 = 16;
pub const MD_FEATURE_RESHAPE_BACKWARDS: u32 = 32;
pub const MD_FEATURE_NEW_OFFSET: u32 = 64;
pub const MD_FEATURE_RECOVERY_BITMAP: u32 = 128;
pub const MD_FEATURE_CLUSTERED: u32 = 256;
pub const MD_FEATURE_JOURNAL: u32 = 512;

// Device roles
pub const MD_DISK_ROLE_SPARE: u16 = 0xffff;
pub const MD_DISK_ROLE_FAULTY: u16 = 0xfffe;
pub const MD_DISK_ROLE_JOURNAL: u16 = 0xfffd;
pub const MD_DISK_ROLE_MAX: u16 = 0xff00;

/// Complete version 1.x superblock structure
#[derive(Debug, Clone)]
pub struct SuperblockV1 {
    // Header
    pub magic: u32,
    pub major_version: u32,
    pub feature_map: u32,
    pub pad0: u32,
    
    // Array identification
    pub set_uuid: [u8; 16],
    pub set_name: [u8; 32],
    
    // Time information
    pub ctime: u64,      // Creation time
    pub utime: u64,      // Update time
    
    // Array configuration
    pub level: i32,
    pub layout: i32,
    pub size: u64,       // Size of component devices in 512-byte sectors
    pub chunksize: i32,
    pub raid_disks: i32,
    pub bitmap_offset: i32, // Sectors after start of superblock
    
    // Reshape information
    pub new_level: i32,
    pub reshape_position: u64,
    pub delta_disks: i32,
    pub new_layout: i32,
    pub new_chunk: i32,
    pub new_offset: i32,
    
    // Device information
    pub data_offset: u64,    // Sectors from start of device
    pub data_size: u64,      // Sectors available for data
    pub super_offset: u64,   // Sectors from start of device to superblock
    pub recovery_offset: u64, // Sectors for recovery
    pub dev_number: u32,     // Persistent device number
    pub cnt_corrected_read: u32, // Number of read errors corrected
    
    // Device role in array
    pub dev_roles: Vec<u16>, // Role of each device (index by dev_number)
    
    // Padding and reserved space
    pub sb_csum: u32,        // Checksum of superblock
    pub events: u64,         // Event counter
    pub resync_offset: u64,  // Resync position
    
    // Bad block log
    pub bblog_shift: u8,
    pub bblog_size: u16,
    pub bblog_offset: u32,
    
    // Device list
    pub max_dev: u32,        // Maximum number of devices
    
    // Metadata version (not in actual superblock, used for positioning)
    pub minor_version: u32,
    
    // Padding to match actual superblock size
    pub pad_bytes: Vec<u8>,
}

impl Default for SuperblockV1 {
    fn default() -> Self {
        Self {
            magic: MD_SB_MAGIC,
            major_version: 1,
            feature_map: 0,
            pad0: 0,
            set_uuid: [0; 16],
            set_name: [0; 32],
            ctime: 0,
            utime: 0,
            level: 0,
            layout: 0,
            size: 0,
            chunksize: 0,
            raid_disks: 0,
            bitmap_offset: 0,
            new_level: 0,
            reshape_position: 0,
            delta_disks: 0,
            new_layout: 0,
            new_chunk: 0,
            new_offset: 0,
            data_offset: 0,
            data_size: 0,
            super_offset: 0,
            recovery_offset: 0,
            dev_number: 0,
            cnt_corrected_read: 0,
            dev_roles: Vec::new(),
            sb_csum: 0,
            events: 0,
            resync_offset: 0,
            bblog_shift: 0,
            bblog_size: 0,
            bblog_offset: 0,
            max_dev: 0,
            minor_version: 2,
            pad_bytes: Vec::new(),
        }
    }
}

fn get_device_size(file: &File) -> u64 {
    let mut size = 0u64;
    use std::os::fd::AsRawFd;
    if unsafe { crate::ioctl::blkgetsize64(file.as_raw_fd(), &mut size) }.is_ok() {
        size
    } else {
        file.metadata().map(|m| m.len()).unwrap_or(0)
    }
}

impl SuperblockV1 {
    fn offset_for_minor(minor: u32, device_size: u64) -> u64 {
        match minor {
            0 => {
                // 1.0 is at the end of the device, aligned to 8K
                let aligned_size = device_size & !0x1FFF;
                if aligned_size > 8192 {
                    aligned_size - 8192
                } else {
                    0
                }
            }
            1 => 0,    // 1.1 is at the start
            2 => 4096, // 1.2 is 4K from start
            _ => 4096, // Default to 1.2
        }
    }
    
    /// Calculate checksum for superblock validation
    fn calculate_checksum(&self) -> u32 {
        // Simplified checksum - in production, implement proper MD checksum
        let mut sum: u32 = 0;
        sum = sum.wrapping_add(self.magic);
        sum = sum.wrapping_add(self.major_version);
        sum = sum.wrapping_add(self.feature_map);
        sum = sum.wrapping_add(self.ctime as u32);
        sum = sum.wrapping_add(self.level as u32);
        sum
    }
    
    /// Check if array has bitmap
    pub fn has_bitmap(&self) -> bool {
        (self.feature_map & MD_FEATURE_BITMAP_OFFSET) != 0
    }
    
    /// Check if array is being reshaped
    pub fn is_reshaping(&self) -> bool {
        (self.feature_map & MD_FEATURE_RESHAPE_ACTIVE) != 0
    }
    
    /// Check if array has bad block log
    pub fn has_bad_blocks(&self) -> bool {
        (self.feature_map & MD_FEATURE_BAD_BLOCKS) != 0
    }
    
    /// Check if array has journal device
    pub fn has_journal(&self) -> bool {
        (self.feature_map & MD_FEATURE_JOURNAL) != 0
    }
    
    /// Get device role description
    pub fn get_device_role(&self, dev_num: usize) -> &str {
        if dev_num >= self.dev_roles.len() {
            return "unknown";
        }
        
        match self.dev_roles[dev_num] {
            MD_DISK_ROLE_SPARE => "spare",
            MD_DISK_ROLE_FAULTY => "faulty",
            MD_DISK_ROLE_JOURNAL => "journal",
            role if role < MD_DISK_ROLE_MAX => "active",
            _ => "unknown",
        }
    }
}

impl Superblock for SuperblockV1 {
    fn load(device: &Path) -> MdResult<Self> {
        debug!("Loading superblock from {}", device.display());
        let mut file = File::open(device)?;
        let device_size = get_device_size(&file);
        debug!("Device size: {} bytes", device_size);

        let minors_to_try = [2, 1, 0];
        
        for minor in minors_to_try {
            let offset = Self::offset_for_minor(minor, device_size);
            debug!("Trying metadata version 1.{} at offset {}", minor, offset);
            
            if file.seek(SeekFrom::Start(offset)).is_err() {
                continue;
            }

            if let Ok(magic) = file.read_u32::<LittleEndian>() {
                if magic == MD_SB_MAGIC {
                    debug!("Found valid magic at offset {}", offset);
                    
                    let major_version = file.read_u32::<LittleEndian>()?;
                    if major_version != 1 {
                        warn!("Major version {} is not 1.x", major_version);
                        continue;
                    }
                    
                    let feature_map = file.read_u32::<LittleEndian>()?;
                    let pad0 = file.read_u32::<LittleEndian>()?;
                    
                    let mut set_uuid = [0u8; 16];
                    file.read_exact(&mut set_uuid)?;
                    
                    let mut set_name = [0u8; 32];
                    file.read_exact(&mut set_name)?;
                    
                    let ctime = file.read_u64::<LittleEndian>()?;
                    let level = file.read_i32::<LittleEndian>()?;
                    let layout = file.read_i32::<LittleEndian>()?;
                    let size = file.read_u64::<LittleEndian>()?;
                    let chunksize = file.read_i32::<LittleEndian>()?;
                    let raid_disks = file.read_i32::<LittleEndian>()?;
                    let bitmap_offset = file.read_i32::<LittleEndian>()?;
                    
                    // New fields for complete parsing
                    let new_level = file.read_i32::<LittleEndian>().unwrap_or(level);
                    let reshape_position = file.read_u64::<LittleEndian>().unwrap_or(0);
                    let delta_disks = file.read_i32::<LittleEndian>().unwrap_or(0);
                    let new_layout = file.read_i32::<LittleEndian>().unwrap_or(layout);
                    let new_chunk = file.read_i32::<LittleEndian>().unwrap_or(chunksize);
                    let new_offset = file.read_i32::<LittleEndian>().unwrap_or(0);
                    
                    let data_offset = file.read_u64::<LittleEndian>().unwrap_or(0);
                    let data_size = file.read_u64::<LittleEndian>().unwrap_or(size);
                    let super_offset = file.read_u64::<LittleEndian>().unwrap_or(offset / 512);
                    let recovery_offset = file.read_u64::<LittleEndian>().unwrap_or(0);
                    let dev_number = file.read_u32::<LittleEndian>().unwrap_or(0);
                    let cnt_corrected_read = file.read_u32::<LittleEndian>().unwrap_or(0);
                    
                    let utime = file.read_u64::<LittleEndian>().unwrap_or(ctime);
                    let events = file.read_u64::<LittleEndian>().unwrap_or(0);
                    let resync_offset = file.read_u64::<LittleEndian>().unwrap_or(0);
                    let sb_csum = file.read_u32::<LittleEndian>().unwrap_or(0);
                    let max_dev = file.read_u32::<LittleEndian>().unwrap_or(raid_disks as u32);
                    
                    // Read device roles
                    let mut dev_roles = Vec::new();
                    for _ in 0..max_dev {
                        if let Ok(role) = file.read_u16::<LittleEndian>() {
                            dev_roles.push(role);
                        } else {
                            break;
                        }
                    }
                    
                    let bblog_shift = file.read_u8().unwrap_or(0);
                    let bblog_size = file.read_u16::<LittleEndian>().unwrap_or(0);
                    let bblog_offset = file.read_u32::<LittleEndian>().unwrap_or(0);

                    let sb = SuperblockV1 {
                        magic,
                        major_version,
                        feature_map,
                        pad0,
                        set_uuid,
                        set_name,
                        ctime,
                        utime,
                        level,
                        layout,
                        size,
                        chunksize,
                        raid_disks,
                        bitmap_offset,
                        new_level,
                        reshape_position,
                        delta_disks,
                        new_layout,
                        new_chunk,
                        new_offset,
                        data_offset,
                        data_size,
                        super_offset,
                        recovery_offset,
                        dev_number,
                        cnt_corrected_read,
                        dev_roles,
                        sb_csum,
                        events,
                        resync_offset,
                        bblog_shift,
                        bblog_size,
                        bblog_offset,
                        max_dev,
                        minor_version: minor,
                        pad_bytes: Vec::new(),
                    };
                    
                    debug!("Successfully loaded superblock version 1.{}", minor);
                    return Ok(sb);
                }
            }
        }
        
        Err(MdError::InvalidMetadata("No valid 1.x superblock found".into()))
    }

    fn examine(&self) {
        println!("          Magic : {:08x}", self.magic);
        println!("        Version : {}.{}", self.major_version, self.minor_version);
        println!("    Feature Map : {:08x}", self.feature_map);
        
        if self.has_bitmap() {
            println!("         Bitmap : present (offset: {} sectors)", self.bitmap_offset);
        }
        if self.is_reshaping() {
            println!("       Reshape : active (position: {})", self.reshape_position);
        }
        if self.has_bad_blocks() {
            println!("     Bad Blocks : tracking enabled");
        }
        if self.has_journal() {
            println!("        Journal : present");
        }
        
        let uuid_str = format!(
            "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
            self.set_uuid[0], self.set_uuid[1], self.set_uuid[2], self.set_uuid[3],
            self.set_uuid[4], self.set_uuid[5], self.set_uuid[6], self.set_uuid[7],
            self.set_uuid[8], self.set_uuid[9], self.set_uuid[10], self.set_uuid[11],
            self.set_uuid[12], self.set_uuid[13], self.set_uuid[14], self.set_uuid[15]
        );
        println!("           UUID : {}", uuid_str);
        
        let name = String::from_utf8_lossy(&self.set_name)
            .trim_end_matches('\0')
            .to_string();
        if !name.is_empty() {
            println!("           Name : {}", name);
        }
        
        println!("     Raid Level : raid{}", self.level);
        println!("     Raid Disks : {}", self.raid_disks);
        println!("           Size : {} sectors ({} MB)", 
                 self.size, self.size * 512 / 1024 / 1024);
        println!("     Chunk Size : {} KB", self.chunksize / 1024);
        println!("         Layout : {}", self.layout);
        println!("    Data Offset : {} sectors", self.data_offset);
        println!("      Data Size : {} sectors", self.data_size);
        println!("   Super Offset : {} sectors", self.super_offset);
        println!("         Events : {}", self.events);
        
        let ctime = chrono::DateTime::from_timestamp(self.ctime as i64, 0)
            .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
            .unwrap_or_else(|| "unknown".to_string());
        println!("   Created Time : {}", ctime);
        
        let utime = chrono::DateTime::from_timestamp(self.utime as i64, 0)
            .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
            .unwrap_or_else(|| "unknown".to_string());
        println!("   Updated Time : {}", utime);
        
        if !self.dev_roles.is_empty() {
            println!("   Device Roles :");
            for (i, &role) in self.dev_roles.iter().enumerate() {
                let role_str = match role {
                    MD_DISK_ROLE_SPARE => "spare".to_string(),
                    MD_DISK_ROLE_FAULTY => "faulty".to_string(),
                    MD_DISK_ROLE_JOURNAL => "journal".to_string(),
                    r if r < MD_DISK_ROLE_MAX => format!("active (slot {})", r),
                    _ => "unknown".to_string(),
                };
                println!("      Device {} : {}", i, role_str);
            }
        }
    }

    fn update_uuid(&mut self, new_uuid: [u8; 16]) {
        self.set_uuid = new_uuid;
    }

    fn write_to_disk(&self, device: &Path) -> MdResult<()> {
        debug!("Writing superblock to {}", device.display());
        let mut file = OpenOptions::new().read(true).write(true).open(device)?;
        let device_size = get_device_size(&file);

        let offset = Self::offset_for_minor(self.minor_version, device_size);
        debug!("Writing at offset {} for version 1.{}", offset, self.minor_version);
        file.seek(SeekFrom::Start(offset))?;

        file.write_u32::<LittleEndian>(self.magic)?;
        file.write_u32::<LittleEndian>(self.major_version)?;
        file.write_u32::<LittleEndian>(self.feature_map)?;
        file.write_u32::<LittleEndian>(self.pad0)?;
        file.write_all(&self.set_uuid)?;
        file.write_all(&self.set_name)?;
        file.write_u64::<LittleEndian>(self.ctime)?;
        file.write_i32::<LittleEndian>(self.level)?;
        file.write_i32::<LittleEndian>(self.layout)?;
        file.write_u64::<LittleEndian>(self.size)?;
        file.write_i32::<LittleEndian>(self.chunksize)?;
        file.write_i32::<LittleEndian>(self.raid_disks)?;
        file.write_i32::<LittleEndian>(self.bitmap_offset)?;
        
        // Write extended fields
        file.write_i32::<LittleEndian>(self.new_level)?;
        file.write_u64::<LittleEndian>(self.reshape_position)?;
        file.write_i32::<LittleEndian>(self.delta_disks)?;
        file.write_i32::<LittleEndian>(self.new_layout)?;
        file.write_i32::<LittleEndian>(self.new_chunk)?;
        file.write_i32::<LittleEndian>(self.new_offset)?;
        
        file.write_u64::<LittleEndian>(self.data_offset)?;
        file.write_u64::<LittleEndian>(self.data_size)?;
        file.write_u64::<LittleEndian>(self.super_offset)?;
        file.write_u64::<LittleEndian>(self.recovery_offset)?;
        file.write_u32::<LittleEndian>(self.dev_number)?;
        file.write_u32::<LittleEndian>(self.cnt_corrected_read)?;
        
        file.write_u64::<LittleEndian>(self.utime)?;
        file.write_u64::<LittleEndian>(self.events)?;
        file.write_u64::<LittleEndian>(self.resync_offset)?;
        file.write_u32::<LittleEndian>(self.sb_csum)?;
        file.write_u32::<LittleEndian>(self.max_dev)?;
        
        // Write device roles
        for &role in &self.dev_roles {
            file.write_u16::<LittleEndian>(role)?;
        }
        
        file.write_u8(self.bblog_shift)?;
        file.write_u16::<LittleEndian>(self.bblog_size)?;
        file.write_u32::<LittleEndian>(self.bblog_offset)?;
        
        file.flush()?;
        debug!("Superblock written successfully");

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_superblock_default() {
        let sb = SuperblockV1::default();
        assert_eq!(sb.magic, MD_SB_MAGIC);
        assert_eq!(sb.major_version, 1);
        assert_eq!(sb.minor_version, 2);
    }
    
    #[test]
    fn test_feature_checks() {
        let mut sb = SuperblockV1::default();
        assert!(!sb.has_bitmap());
        assert!(!sb.is_reshaping());
        assert!(!sb.has_bad_blocks());
        
        sb.feature_map = MD_FEATURE_BITMAP_OFFSET;
        assert!(sb.has_bitmap());
        
        sb.feature_map |= MD_FEATURE_RESHAPE_ACTIVE;
        assert!(sb.is_reshaping());
    }
    
    #[test]
    fn test_device_roles() {
        let mut sb = SuperblockV1::default();
        sb.dev_roles = vec![0, 1, MD_DISK_ROLE_SPARE, MD_DISK_ROLE_FAULTY];
        
        assert_eq!(sb.get_device_role(0), "active");
        assert_eq!(sb.get_device_role(1), "active");
        assert_eq!(sb.get_device_role(2), "spare");
        assert_eq!(sb.get_device_role(3), "faulty");
        assert_eq!(sb.get_device_role(10), "unknown");
    }
    
    #[test]
    fn test_offset_calculation() {
        // Test 1.0 (end of device)
        let offset = SuperblockV1::offset_for_minor(0, 1024 * 1024 * 1024);
        assert!(offset > 0);
        
        // Test 1.1 (start)
        let offset = SuperblockV1::offset_for_minor(1, 1024 * 1024 * 1024);
        assert_eq!(offset, 0);
        
        // Test 1.2 (4K from start)
        let offset = SuperblockV1::offset_for_minor(2, 1024 * 1024 * 1024);
        assert_eq!(offset, 4096);
    }
}