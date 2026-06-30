#![allow(dead_code)]
//! Parser for the version 0.90 superblock format (legacy)
//! Based on Linux kernel md_p.h

use super::Superblock;
use crate::error::{MdError, MdResult};
use std::path::Path;
use std::fs::{File, OpenOptions};
use std::io::{Write, Seek, SeekFrom};
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use tracing::debug;

pub const MD_SB_MAGIC_V0: u32 = 0xa92b4efc;
pub const MD_SB_CLEAN: u32 = 0;
pub const MD_SB_ERRORS: u32 = 1;

// Disk states
pub const MD_DISK_FAULTY_V0: u32 = 1;
pub const MD_DISK_ACTIVE_V0: u32 = 2;
pub const MD_DISK_SYNC_V0: u32 = 4;
pub const MD_DISK_REMOVED_V0: u32 = 8;

// Superblock size
const MD_SB_BYTES: usize = 4096;
const MD_SB_WORDS: usize = MD_SB_BYTES / 4;

/// Version 0.90 superblock structure
/// This is the legacy format, located at end of device
#[derive(Debug, Clone)]
pub struct SuperblockV0 {
    // Identification
    pub md_magic: u32,
    pub major_version: u32,
    pub minor_version: u32,
    pub patch_version: u32,
    pub gvalid_words: u32,
    pub set_uuid0: u32,
    pub ctime: u32,
    pub level: u32,
    pub size: u32,
    pub nr_disks: u32,
    pub raid_disks: u32,
    pub md_minor: u32,
    pub not_persistent: u32,
    
    // UUID (4 x u32)
    pub set_uuid1: u32,
    pub set_uuid2: u32,
    pub set_uuid3: u32,
    
    // Time information
    pub utime: u32,
    pub state: u32,
    pub active_disks: u32,
    pub working_disks: u32,
    pub failed_disks: u32,
    pub spare_disks: u32,
    
    // Checksum
    pub sb_csum: u32,
    pub events_lo: u32,
    pub events_hi: u32,
    pub cp_events_lo: u32,
    pub cp_events_hi: u32,
    
    // Recovery
    pub recovery_cp: u32,
    
    // Reshape
    pub reshape_position_lo: u32,
    pub reshape_position_hi: u32,
    pub new_level: u32,
    pub delta_disks: u32,
    pub new_layout: u32,
    pub new_chunk: u32,
    
    // Layout and chunk
    pub layout: u32,
    pub chunk_size: u32,
    
    // Disk information (up to MD_SB_DISKS)
    pub disks: Vec<DiskInfoV0>,
    
    // Padding
    pub pad: Vec<u32>,
}

#[derive(Debug, Clone, Default)]
pub struct DiskInfoV0 {
    pub number: u32,
    pub major: u32,
    pub minor: u32,
    pub raid_disk: u32,
    pub state: u32,
}

impl Default for SuperblockV0 {
    fn default() -> Self {
        Self {
            md_magic: MD_SB_MAGIC_V0,
            major_version: 0,
            minor_version: 90,
            patch_version: 0,
            gvalid_words: 0,
            set_uuid0: 0,
            ctime: 0,
            level: 0,
            size: 0,
            nr_disks: 0,
            raid_disks: 0,
            md_minor: 0,
            not_persistent: 0,
            set_uuid1: 0,
            set_uuid2: 0,
            set_uuid3: 0,
            utime: 0,
            state: MD_SB_CLEAN,
            active_disks: 0,
            working_disks: 0,
            failed_disks: 0,
            spare_disks: 0,
            sb_csum: 0,
            events_lo: 0,
            events_hi: 0,
            cp_events_lo: 0,
            cp_events_hi: 0,
            recovery_cp: 0,
            reshape_position_lo: 0,
            reshape_position_hi: 0,
            new_level: 0,
            delta_disks: 0,
            new_layout: 0,
            new_chunk: 0,
            layout: 0,
            chunk_size: 0,
            disks: Vec::new(),
            pad: Vec::new(),
        }
    }
}

impl SuperblockV0 {
    /// Get 64-bit event counter
    pub fn events(&self) -> u64 {
        ((self.events_hi as u64) << 32) | (self.events_lo as u64)
    }
    
    /// Get 64-bit reshape position
    pub fn reshape_position(&self) -> u64 {
        ((self.reshape_position_hi as u64) << 32) | (self.reshape_position_lo as u64)
    }
    
    /// Get UUID as bytes
    pub fn uuid_bytes(&self) -> [u8; 16] {
        let mut uuid = [0u8; 16];
        uuid[0..4].copy_from_slice(&self.set_uuid0.to_le_bytes());
        uuid[4..8].copy_from_slice(&self.set_uuid1.to_le_bytes());
        uuid[8..12].copy_from_slice(&self.set_uuid2.to_le_bytes());
        uuid[12..16].copy_from_slice(&self.set_uuid3.to_le_bytes());
        uuid
    }
    
    /// Calculate superblock offset (at end of device, aligned to 64K)
    fn calculate_offset(device_size: u64) -> u64 {
        let aligned_size = device_size & !0xFFFF; // Align to 64K
        if aligned_size >= MD_SB_BYTES as u64 {
            aligned_size - MD_SB_BYTES as u64
        } else {
            0
        }
    }
    
    /// Calculate checksum
    fn calculate_checksum(&self) -> u32 {
        // Simplified checksum - sum of all words except checksum field
        let mut sum: u32 = 0;
        sum = sum.wrapping_add(self.md_magic);
        sum = sum.wrapping_add(self.major_version);
        sum = sum.wrapping_add(self.minor_version);
        sum = sum.wrapping_add(self.ctime);
        sum = sum.wrapping_add(self.level);
        sum = sum.wrapping_add(self.size);
        sum
    }
}

impl Superblock for SuperblockV0 {
    fn load(device: &Path) -> MdResult<Self> {
        debug!("Loading v0.90 superblock from {}", device.display());
        let mut file = File::open(device)?;
        
        // Get device size
        let device_size = {
            let mut size = 0u64;
            use std::os::fd::AsRawFd;
            if unsafe { crate::ioctl::blkgetsize64(file.as_raw_fd(), &mut size) }.is_ok() {
                size
            } else {
                file.metadata()?.len()
            }
        };
        
        debug!("Device size: {} bytes", device_size);
        
        // Calculate offset (at end of device)
        let offset = Self::calculate_offset(device_size);
        debug!("Trying v0.90 superblock at offset {}", offset);
        
        file.seek(SeekFrom::Start(offset))?;
        
        // Read magic
        let md_magic = file.read_u32::<LittleEndian>()?;
        if md_magic != MD_SB_MAGIC_V0 {
            return Err(MdError::MagicMismatch {
                expected: MD_SB_MAGIC_V0,
                found: md_magic,
            });
        }
        
        debug!("Found valid v0.90 magic");
        
        // Read header
        let major_version = file.read_u32::<LittleEndian>()?;
        let minor_version = file.read_u32::<LittleEndian>()?;
        let patch_version = file.read_u32::<LittleEndian>()?;
        
        if major_version != 0 || minor_version != 90 {
            return Err(MdError::UnsupportedMetadataVersion(
                format!("{}.{}", major_version, minor_version)
            ));
        }
        
        let gvalid_words = file.read_u32::<LittleEndian>()?;
        let set_uuid0 = file.read_u32::<LittleEndian>()?;
        let ctime = file.read_u32::<LittleEndian>()?;
        let level = file.read_u32::<LittleEndian>()?;
        let size = file.read_u32::<LittleEndian>()?;
        let nr_disks = file.read_u32::<LittleEndian>()?;
        let raid_disks = file.read_u32::<LittleEndian>()?;
        let md_minor = file.read_u32::<LittleEndian>()?;
        let not_persistent = file.read_u32::<LittleEndian>()?;
        let set_uuid1 = file.read_u32::<LittleEndian>()?;
        let set_uuid2 = file.read_u32::<LittleEndian>()?;
        let set_uuid3 = file.read_u32::<LittleEndian>()?;
        let utime = file.read_u32::<LittleEndian>()?;
        let state = file.read_u32::<LittleEndian>()?;
        let active_disks = file.read_u32::<LittleEndian>()?;
        let working_disks = file.read_u32::<LittleEndian>()?;
        let failed_disks = file.read_u32::<LittleEndian>()?;
        let spare_disks = file.read_u32::<LittleEndian>()?;
        let sb_csum = file.read_u32::<LittleEndian>()?;
        let events_lo = file.read_u32::<LittleEndian>()?;
        let events_hi = file.read_u32::<LittleEndian>()?;
        let cp_events_lo = file.read_u32::<LittleEndian>()?;
        let cp_events_hi = file.read_u32::<LittleEndian>()?;
        let recovery_cp = file.read_u32::<LittleEndian>()?;
        let reshape_position_lo = file.read_u32::<LittleEndian>()?;
        let reshape_position_hi = file.read_u32::<LittleEndian>()?;
        let new_level = file.read_u32::<LittleEndian>()?;
        let delta_disks = file.read_u32::<LittleEndian>()?;
        let new_layout = file.read_u32::<LittleEndian>()?;
        let new_chunk = file.read_u32::<LittleEndian>()?;
        let layout = file.read_u32::<LittleEndian>()?;
        let chunk_size = file.read_u32::<LittleEndian>()?;
        
        // Read disk information (typically up to 27 disks in v0.90)
        let mut disks = Vec::new();
        for _ in 0..nr_disks.min(27) {
            let number = file.read_u32::<LittleEndian>()?;
            let major = file.read_u32::<LittleEndian>()?;
            let minor = file.read_u32::<LittleEndian>()?;
            let raid_disk = file.read_u32::<LittleEndian>()?;
            let state = file.read_u32::<LittleEndian>()?;
            
            disks.push(DiskInfoV0 {
                number,
                major,
                minor,
                raid_disk,
                state,
            });
        }
        
        debug!("Successfully loaded v0.90 superblock");
        
        Ok(SuperblockV0 {
            md_magic,
            major_version,
            minor_version,
            patch_version,
            gvalid_words,
            set_uuid0,
            ctime,
            level,
            size,
            nr_disks,
            raid_disks,
            md_minor,
            not_persistent,
            set_uuid1,
            set_uuid2,
            set_uuid3,
            utime,
            state,
            active_disks,
            working_disks,
            failed_disks,
            spare_disks,
            sb_csum,
            events_lo,
            events_hi,
            cp_events_lo,
            cp_events_hi,
            recovery_cp,
            reshape_position_lo,
            reshape_position_hi,
            new_level,
            delta_disks,
            new_layout,
            new_chunk,
            layout,
            chunk_size,
            disks,
            pad: Vec::new(),
        })
    }

    fn examine(&self) {
        println!("          Magic : {:08x}", self.md_magic);
        println!("        Version : {}.{}.{}", 
                 self.major_version, self.minor_version, self.patch_version);
        
        let uuid = self.uuid_bytes();
        let uuid_str = format!(
            "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
            uuid[0], uuid[1], uuid[2], uuid[3],
            uuid[4], uuid[5], uuid[6], uuid[7],
            uuid[8], uuid[9], uuid[10], uuid[11],
            uuid[12], uuid[13], uuid[14], uuid[15]
        );
        println!("           UUID : {}", uuid_str);
        
        println!("     Raid Level : raid{}", self.level);
        println!("     Raid Disks : {}", self.raid_disks);
        println!("    Total Disks : {}", self.nr_disks);
        println!("   Active Disks : {}", self.active_disks);
        println!("  Working Disks : {}", self.working_disks);
        println!("   Failed Disks : {}", self.failed_disks);
        println!("    Spare Disks : {}", self.spare_disks);
        println!("           Size : {} KB", self.size);
        println!("     Chunk Size : {} KB", self.chunk_size);
        println!("         Layout : {}", self.layout);
        println!("          State : {}", if self.state == MD_SB_CLEAN { "clean" } else { "dirty" });
        println!("         Events : {}", self.events());
        
        let ctime = chrono::DateTime::from_timestamp(self.ctime as i64, 0)
            .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
            .unwrap_or_else(|| "unknown".to_string());
        println!("   Created Time : {}", ctime);
        
        let utime = chrono::DateTime::from_timestamp(self.utime as i64, 0)
            .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
            .unwrap_or_else(|| "unknown".to_string());
        println!("   Updated Time : {}", utime);
        
        if !self.disks.is_empty() {
            println!("          Disks :");
            for (i, disk) in self.disks.iter().enumerate() {
                let state_str = if disk.state & MD_DISK_FAULTY_V0 != 0 {
                    "faulty"
                } else if disk.state & MD_DISK_ACTIVE_V0 != 0 {
                    "active"
                } else if disk.state & MD_DISK_SYNC_V0 != 0 {
                    "sync"
                } else if disk.state & MD_DISK_REMOVED_V0 != 0 {
                    "removed"
                } else {
                    "spare"
                };
                println!("      Disk {} : {}:{} (raid_disk: {}, state: {})",
                         i, disk.major, disk.minor, disk.raid_disk, state_str);
            }
        }
    }

    fn update_uuid(&mut self, new_uuid: [u8; 16]) {
        self.set_uuid0 = u32::from_le_bytes([new_uuid[0], new_uuid[1], new_uuid[2], new_uuid[3]]);
        self.set_uuid1 = u32::from_le_bytes([new_uuid[4], new_uuid[5], new_uuid[6], new_uuid[7]]);
        self.set_uuid2 = u32::from_le_bytes([new_uuid[8], new_uuid[9], new_uuid[10], new_uuid[11]]);
        self.set_uuid3 = u32::from_le_bytes([new_uuid[12], new_uuid[13], new_uuid[14], new_uuid[15]]);
    }

    fn write_to_disk(&self, device: &Path) -> MdResult<()> {
        debug!("Writing v0.90 superblock to {}", device.display());
        let mut file = OpenOptions::new().read(true).write(true).open(device)?;
        
        // Get device size
        let device_size = {
            let mut size = 0u64;
            use std::os::fd::AsRawFd;
            if unsafe { crate::ioctl::blkgetsize64(file.as_raw_fd(), &mut size) }.is_ok() {
                size
            } else {
                file.metadata()?.len()
            }
        };
        
        let offset = Self::calculate_offset(device_size);
        debug!("Writing at offset {}", offset);
        file.seek(SeekFrom::Start(offset))?;
        
        // Write all fields
        file.write_u32::<LittleEndian>(self.md_magic)?;
        file.write_u32::<LittleEndian>(self.major_version)?;
        file.write_u32::<LittleEndian>(self.minor_version)?;
        file.write_u32::<LittleEndian>(self.patch_version)?;
        file.write_u32::<LittleEndian>(self.gvalid_words)?;
        file.write_u32::<LittleEndian>(self.set_uuid0)?;
        file.write_u32::<LittleEndian>(self.ctime)?;
        file.write_u32::<LittleEndian>(self.level)?;
        file.write_u32::<LittleEndian>(self.size)?;
        file.write_u32::<LittleEndian>(self.nr_disks)?;
        file.write_u32::<LittleEndian>(self.raid_disks)?;
        file.write_u32::<LittleEndian>(self.md_minor)?;
        file.write_u32::<LittleEndian>(self.not_persistent)?;
        file.write_u32::<LittleEndian>(self.set_uuid1)?;
        file.write_u32::<LittleEndian>(self.set_uuid2)?;
        file.write_u32::<LittleEndian>(self.set_uuid3)?;
        file.write_u32::<LittleEndian>(self.utime)?;
        file.write_u32::<LittleEndian>(self.state)?;
        file.write_u32::<LittleEndian>(self.active_disks)?;
        file.write_u32::<LittleEndian>(self.working_disks)?;
        file.write_u32::<LittleEndian>(self.failed_disks)?;
        file.write_u32::<LittleEndian>(self.spare_disks)?;
        file.write_u32::<LittleEndian>(self.sb_csum)?;
        file.write_u32::<LittleEndian>(self.events_lo)?;
        file.write_u32::<LittleEndian>(self.events_hi)?;
        file.write_u32::<LittleEndian>(self.cp_events_lo)?;
        file.write_u32::<LittleEndian>(self.cp_events_hi)?;
        file.write_u32::<LittleEndian>(self.recovery_cp)?;
        file.write_u32::<LittleEndian>(self.reshape_position_lo)?;
        file.write_u32::<LittleEndian>(self.reshape_position_hi)?;
        file.write_u32::<LittleEndian>(self.new_level)?;
        file.write_u32::<LittleEndian>(self.delta_disks)?;
        file.write_u32::<LittleEndian>(self.new_layout)?;
        file.write_u32::<LittleEndian>(self.new_chunk)?;
        file.write_u32::<LittleEndian>(self.layout)?;
        file.write_u32::<LittleEndian>(self.chunk_size)?;
        
        // Write disk information
        for disk in &self.disks {
            file.write_u32::<LittleEndian>(disk.number)?;
            file.write_u32::<LittleEndian>(disk.major)?;
            file.write_u32::<LittleEndian>(disk.minor)?;
            file.write_u32::<LittleEndian>(disk.raid_disk)?;
            file.write_u32::<LittleEndian>(disk.state)?;
        }
        
        file.flush()?;
        debug!("v0.90 superblock written successfully");
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_superblock_v0_default() {
        let sb = SuperblockV0::default();
        assert_eq!(sb.md_magic, MD_SB_MAGIC_V0);
        assert_eq!(sb.major_version, 0);
        assert_eq!(sb.minor_version, 90);
    }
    
    #[test]
    fn test_events_64bit() {
        let mut sb = SuperblockV0::default();
        sb.events_hi = 1;
        sb.events_lo = 0x12345678;
        assert_eq!(sb.events(), 0x100000000 + 0x12345678);
    }
    
    #[test]
    fn test_uuid_conversion() {
        let mut sb = SuperblockV0::default();
        let uuid = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];
        sb.update_uuid(uuid);
        assert_eq!(sb.uuid_bytes(), uuid);
    }
    
    #[test]
    fn test_offset_calculation() {
        let offset = SuperblockV0::calculate_offset(1024 * 1024 * 1024);
        assert!(offset > 0);
        assert!(offset < 1024 * 1024 * 1024);
    }
}
