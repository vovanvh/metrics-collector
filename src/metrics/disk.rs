// Disk space metric collector
//
// Collects disk usage metrics for all mounted filesystems
// Provides information about total, used, and available space

use async_trait::async_trait;
use bson::{doc, Document};
use chrono::Utc;
use std::error::Error;
use sysinfo::Disks;
use tracing::debug;

use super::MetricCollector;

/// Disk space metric collector
///
/// Collects disk usage statistics for all mounted filesystems.
/// Each mounted disk is reported as a separate entry in the document.
///
/// # What is Collected
/// - Mount point (e.g., "/", "/home", "/mnt/data")
/// - Filesystem type (e.g., "ext4", "xfs", "apfs")
/// - Total space
/// - Used space
/// - Available space
/// - Usage percentage
///
/// # Platform Support
/// - Linux: Full support via statvfs
/// - macOS: Full support
/// - Windows: Full support (drive letters)
pub struct DiskCollector;

impl DiskCollector {
    /// Creates a new DiskCollector instance
    pub fn new() -> Self {
        DiskCollector
    }

    /// Converts bytes to gigabytes for more readable storage
    ///
    /// # Arguments
    /// * `bytes` - Value in bytes
    ///
    /// # Returns
    /// Value in gigabytes (GB) as f64
    fn bytes_to_gb(bytes: u64) -> f64 {
        bytes as f64 / (1024.0 * 1024.0 * 1024.0)
    }

    /// Calculates percentage of disk used
    ///
    /// # Arguments
    /// * `used` - Used space in bytes
    /// * `total` - Total space in bytes
    ///
    /// # Returns
    /// Percentage (0.0 - 100.0)
    fn calculate_percentage(used: u64, total: u64) -> f64 {
        if total == 0 {
            0.0
        } else {
            (used as f64 / total as f64) * 100.0
        }
    }
}

#[async_trait]
impl MetricCollector for DiskCollector {
    /// Returns the metric name
    fn name(&self) -> &str {
        "DiskSpace"
    }

    /// Collects current disk usage metrics for all mounted filesystems
    ///
    /// # Returns BSON Document Structure
    /// ```json
    /// {
    ///   "node": "1111-1111",
    ///   "timestamp": "2024-01-15T10:30:00Z",
    ///   "disks": [
    ///     {
    ///       "mount_point": "/",
    ///       "filesystem": "ext4",
    ///       "total_gb": 500.0,
    ///       "used_gb": 250.0,
    ///       "available_gb": 250.0,
    ///       "used_percent": 50.0
    ///     },
    ///     {
    ///       "mount_point": "/mnt/data",
    ///       "filesystem": "xfs",
    ///       "total_gb": 1000.0,
    ///       "used_gb": 750.0,
    ///       "available_gb": 250.0,
    ///       "used_percent": 75.0
    ///     }
    ///   ]
    /// }
    /// ```
    ///
    /// # Errors
    /// Returns error if disk information cannot be retrieved (rare)
    async fn collect(&self, node_id: &str) -> Result<Document, Box<dyn Error + Send + Sync>> {
        debug!("Collecting disk space metrics");

        // Get all disk information
        // This queries the OS for all mounted filesystems
        let disks = Disks::new_with_refreshed_list();

        // Build array of disk information
        let mut disk_array = Vec::new();

        for disk in disks.list() {
            // Get disk statistics
            let mount_point = disk.mount_point().to_string_lossy().to_string();
            let filesystem = disk.file_system().to_string_lossy().to_string();
            let total_space = disk.total_space();
            let available_space = disk.available_space();

            // Calculate used space
            // used = total - available
            let used_space = total_space.saturating_sub(available_space);

            // Calculate usage percentage
            let used_percent = Self::calculate_percentage(used_space, total_space);

            // Create disk info document
            let disk_doc = doc! {
                // Where this disk is mounted (e.g., "/", "/home")
                "mount_point": mount_point.clone(),

                // Filesystem type (e.g., "ext4", "xfs", "apfs")
                "filesystem": filesystem,

                // Total capacity of the disk
                "total_gb": Self::bytes_to_gb(total_space),

                // Space currently in use
                "used_gb": Self::bytes_to_gb(used_space),

                // Space available for new files
                // Note: May be less than (total - used) due to reserved blocks
                "available_gb": Self::bytes_to_gb(available_space),

                // Percentage of disk space used
                "used_percent": used_percent,
            };

            debug!(
                "Disk {}: {:.1}/{:.1} GB ({:.1}%)",
                mount_point,
                Self::bytes_to_gb(used_space),
                Self::bytes_to_gb(total_space),
                used_percent
            );

            disk_array.push(disk_doc);
        }

        // Create main document with array of all disks
        let doc = doc! {
            // Node identifier (from configuration key)
            "node": node_id,

            // Timestamp when metric was collected (UTC)
            "timestamp": Utc::now(),

            // Array of disk information for all mounted filesystems
            // Each element contains info about one disk/partition
            "disks": disk_array,
        };

        debug!("Collected information for {} disk(s)", disks.list().len());

        Ok(doc)
    }
}

impl Default for DiskCollector {
    fn default() -> Self {
        Self::new()
    }
}
