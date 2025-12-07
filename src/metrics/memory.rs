// Memory metric collector
//
// Collects system memory usage metrics including RAM and swap
// Provides detailed information about total, used, available, and free memory

use async_trait::async_trait;
use bson::{doc, Document};
use chrono::Utc;
use std::error::Error;
use sysinfo::System;
use tracing::debug;

use super::MetricCollector;

/// Memory usage metric collector
///
/// Collects comprehensive memory statistics including:
/// - Total physical RAM
/// - Used RAM
/// - Available RAM (includes buffers/cache that can be freed)
/// - Free RAM (completely unused)
/// - Swap space (total, used, free)
///
/// # Platform Support
/// - Linux: Full support via /proc/meminfo
/// - macOS: Full support via vm_stat
/// - Windows: Full support via GlobalMemoryStatusEx
pub struct MemoryCollector {
    /// System information provider
    system: System,
}

impl MemoryCollector {
    /// Creates a new MemoryCollector instance
    pub fn new() -> Self {
        MemoryCollector {
            system: System::new(),
        }
    }

    /// Converts bytes to megabytes for more readable storage
    ///
    /// # Arguments
    /// * `bytes` - Value in bytes
    ///
    /// # Returns
    /// Value in megabytes (MB) as i64
    fn bytes_to_mb(bytes: u64) -> i64 {
        (bytes / (1024 * 1024)) as i64
    }

    /// Calculates percentage of memory used
    ///
    /// # Arguments
    /// * `used` - Used memory in bytes
    /// * `total` - Total memory in bytes
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
impl MetricCollector for MemoryCollector {
    /// Returns the metric name
    fn name(&self) -> &str {
        "Memory"
    }

    /// Collects current memory usage metrics
    ///
    /// # Returns BSON Document Structure
    /// ```json
    /// {
    ///   "node": "1111-1111",
    ///   "timestamp": "2024-01-15T10:30:00Z",
    ///   "total_mb": 16384,
    ///   "used_mb": 8192,
    ///   "available_mb": 8192,
    ///   "free_mb": 4096,
    ///   "used_percent": 50.0,
    ///   "swap_total_mb": 8192,
    ///   "swap_used_mb": 1024,
    ///   "swap_free_mb": 7168,
    ///   "swap_used_percent": 12.5
    /// }
    /// ```
    ///
    /// # Errors
    /// Returns error if system information cannot be retrieved (rare)
    async fn collect(&self, node_id: &str) -> Result<Document, Box<dyn Error + Send + Sync>> {
        debug!("Collecting memory metrics");

        // Refresh memory information
        // Note: We create a new System instance each time to get fresh data
        let mut sys = System::new();
        sys.refresh_memory();

        // Get memory statistics (in bytes)
        let total_memory = sys.total_memory();
        let used_memory = sys.used_memory();
        let available_memory = sys.available_memory();
        let free_memory = sys.free_memory();

        // Get swap statistics (in bytes)
        let total_swap = sys.total_swap();
        let used_swap = sys.used_swap();
        let free_swap = sys.free_swap();

        // Calculate percentages
        let used_percent = Self::calculate_percentage(used_memory, total_memory);
        let swap_used_percent = Self::calculate_percentage(used_swap, total_swap);

        // Create BSON document with memory data
        let doc = doc! {
            // Node identifier (from configuration key)
            "node": node_id,

            // Timestamp when metric was collected (UTC)
            "timestamp": Utc::now(),

            // Total physical RAM installed
            "total_mb": Self::bytes_to_mb(total_memory),

            // Memory currently in use
            // Includes application memory, kernel memory, and some caches
            "used_mb": Self::bytes_to_mb(used_memory),

            // Memory available for new applications
            // Includes free memory + reclaimable cache
            "available_mb": Self::bytes_to_mb(available_memory),

            // Memory completely unused
            // Typically lower than available_mb on Linux due to caching
            "free_mb": Self::bytes_to_mb(free_memory),

            // Percentage of total memory in use
            "used_percent": used_percent,

            // Total swap space configured
            "swap_total_mb": Self::bytes_to_mb(total_swap),

            // Swap space currently in use
            // High values indicate memory pressure
            "swap_used_mb": Self::bytes_to_mb(used_swap),

            // Swap space available
            "swap_free_mb": Self::bytes_to_mb(free_swap),

            // Percentage of swap in use
            // Values > 0 may indicate insufficient RAM
            "swap_used_percent": swap_used_percent,
        };

        debug!(
            "Memory: {}/{} MB ({:.1}%), Swap: {}/{} MB ({:.1}%)",
            Self::bytes_to_mb(used_memory),
            Self::bytes_to_mb(total_memory),
            used_percent,
            Self::bytes_to_mb(used_swap),
            Self::bytes_to_mb(total_swap),
            swap_used_percent
        );

        Ok(doc)
    }
}

impl Default for MemoryCollector {
    fn default() -> Self {
        Self::new()
    }
}
