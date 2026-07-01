//! API data models

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Detailed information about a RAID array
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ArrayInfo {
    /// Array name (e.g., "md0")
    pub name: String,
    /// Device path (e.g., "/dev/md0")
    pub device: String,
    /// RAID level (e.g., "raid1", "raid5")
    pub level: String,
    /// Current array state
    pub state: String,
    /// Array size in bytes
    pub size: u64,
    /// Number of RAID disks
    pub raid_disks: i32,
    /// Total number of disks
    pub total_disks: i32,
    /// Number of active disks
    pub active_disks: i32,
    /// Number of working disks
    pub working_disks: i32,
    /// Number of failed disks
    pub failed_disks: i32,
    /// Number of spare disks
    pub spare_disks: i32,
    /// Array UUID
    pub uuid: String,
    /// Chunk size in bytes
    pub chunk_size: i32,
    /// Array layout
    pub layout: i32,
    /// Creation timestamp
    pub created: Option<String>,
    /// Last update timestamp
    pub updated: Option<String>,
}

/// Response containing list of arrays
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ArrayListResponse {
    /// List of array summaries
    pub arrays: Vec<ArraySummary>,
    /// Total number of arrays
    pub total: usize,
}

/// Summary information about a RAID array
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ArraySummary {
    /// Array name (e.g., "md0")
    pub name: String,
    /// Device path (e.g., "/dev/md0")
    pub device: String,
    /// RAID level (e.g., "raid1", "raid5")
    pub level: String,
    /// Current array state
    pub state: String,
    /// Health status (healthy, degraded, failed)
    pub health: String,
}

/// Request to create a new RAID array
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateArrayRequest {
    /// MD device path (e.g., "/dev/md0")
    #[schema(example = "/dev/md0")]
    pub device: String,
    /// RAID level (0, 1, 4, 5, 6, 10)
    #[schema(example = 1)]
    pub level: u8,
    /// Number of RAID devices
    #[schema(example = 2)]
    pub raid_devices: u32,
    /// Metadata version (e.g., "1.2")
    #[schema(example = "1.2")]
    pub metadata: Option<String>,
    /// Component device paths
    #[schema(example = json!(vec!["/dev/sdb1", "/dev/sdc1"]))]
    pub components: Vec<String>,
    /// Chunk size in KiB
    #[schema(example = 512)]
    pub chunk_size: Option<i32>,
}

/// Request to manage array disks
#[derive(Debug, Deserialize, ToSchema)]
pub struct ManageArrayRequest {
    /// Devices to add to the array
    pub add: Option<Vec<String>>,
    /// Devices to remove from the array
    pub remove: Option<Vec<String>>,
    /// Devices to mark as failed
    pub fail: Option<Vec<String>>,
}

/// Generic operation response
#[derive(Debug, Serialize, ToSchema)]
pub struct OperationResponse {
    /// Whether the operation succeeded
    pub success: bool,
    /// Human-readable message
    pub message: String,
    /// Additional details (optional)
    pub details: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct MigrationRequest {
    pub source: String,
    pub target: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ClusterJoinRequest {
    pub node_id: String,
    pub address: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct DiskHealthRequest {
    pub devices: Vec<String>,
    #[serde(default = "default_health_threshold")]
    pub threshold: u64,
}

fn default_health_threshold() -> u64 {
    100
}

/// Health check response
#[derive(Debug, Serialize, ToSchema)]
pub struct HealthResponse {
    /// Service status
    #[schema(example = "healthy")]
    pub status: String,
    /// Service version
    #[schema(example = "0.1.0")]
    pub version: String,
    /// Uptime in seconds
    pub uptime: u64,
    /// Number of arrays being monitored
    pub arrays_monitored: usize,
}

/// Request to start a scrub operation
#[derive(Debug, Deserialize, ToSchema)]
pub struct ScrubRequest {
    /// Whether to repair errors found during scrub
    pub repair: bool,
}

/// Response from scrub operation
#[derive(Debug, Serialize, ToSchema)]
pub struct ScrubResponse {
    /// Whether the scrub was started successfully
    pub started: bool,
    /// Estimated duration in seconds (if available)
    pub estimated_duration: Option<u64>,
}