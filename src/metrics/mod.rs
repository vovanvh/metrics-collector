// Metrics module - defines the extensible architecture for metric collection
//
// This module provides the core trait that all metric collectors must implement,
// enabling easy addition of new metric types without modifying the core scheduler.

use async_trait::async_trait;
use bson::Document;
use std::error::Error;

// Re-export all metric implementations
pub mod load_average;
pub mod memory;
pub mod disk;
pub mod docker;

/// Core trait that all metric collectors must implement.
///
/// This trait defines the interface for collecting and preparing metrics for storage.
/// Each metric type (load average, memory, disk, docker) implements this trait,
/// allowing the scheduler to handle all metrics uniformly.
///
/// # Design Philosophy
/// - **Async-first**: All operations are async to prevent blocking the Tokio runtime
/// - **Error handling**: Methods return Results to gracefully handle failures
/// - **Decoupled storage**: Collectors prepare data as BSON documents, storage is separate
#[async_trait]
pub trait MetricCollector: Send + Sync {
    /// Returns the human-readable name of this metric type.
    /// Used for logging and identification.
    ///
    /// # Example
    /// ```
    /// "LoadAverage", "Memory", "DiskSpace", "DockerStats"
    /// ```
    fn name(&self) -> &str;

    /// Collects the current metric data and returns it as a BSON document.
    ///
    /// This method performs the actual metric collection (reading system info,
    /// querying Docker API, etc.) and formats the data as a BSON document
    /// ready for MongoDB insertion.
    ///
    /// # Arguments
    /// * `node_id` - The node identifier (from the configuration key) to include in the document
    ///
    /// # Returns
    /// * `Ok(Document)` - BSON document containing the metric data with timestamp and node_id
    /// * `Err(Box<dyn Error>)` - If collection fails (e.g., Docker unavailable, permission denied)
    ///
    /// # Document Structure
    /// All metric documents should include at minimum:
    /// - `node`: String - The node identifier
    /// - `timestamp`: DateTime - When the metric was collected
    /// - Additional fields specific to the metric type
    async fn collect(&self, node_id: &str) -> Result<Document, Box<dyn Error + Send + Sync>>;
}

/// Helper function to create all metric collectors.
///
/// This function instantiates all available metric collectors and returns them
/// as trait objects. When adding a new metric type, add its instantiation here.
///
/// # Returns
/// Vector of boxed MetricCollector trait objects, one for each metric type
///
/// # Adding New Metrics
/// To add a new metric:
/// 1. Create a new module (e.g., `network.rs`)
/// 2. Implement the `MetricCollector` trait
/// 3. Add the module to the re-exports at the top of this file
/// 4. Add instantiation here: `Box::new(network::NetworkCollector::new())`
pub fn create_all_collectors() -> Vec<Box<dyn MetricCollector>> {
    vec![
        // Load average monitoring (1min, 5min, 15min averages)
        Box::new(load_average::LoadAverageCollector::new()),

        // Memory usage monitoring (total, used, available, swap)
        Box::new(memory::MemoryCollector::new()),

        // Disk space monitoring (total, used, free for all mounted filesystems)
        Box::new(disk::DiskCollector::new()),

        // Docker container stats (CPU, memory, network I/O per container)
        Box::new(docker::DockerCollector::new()),
    ]
}
