#![allow(dead_code)]
pub mod v0;
pub mod v1;

use crate::error::MdError;
use std::path::Path;

/// Trait defining how to interact with different RAID superblock versions
pub trait Superblock {
    fn load(device: &Path) -> Result<Self, MdError> where Self: Sized;
    fn examine(&self);
    fn update_uuid(&mut self, new_uuid: [u8; 16]);
    fn write_to_disk(&self, device: &Path) -> Result<(), MdError>;
}
