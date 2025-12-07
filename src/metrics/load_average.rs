// Load Average metric collector
//
// Collects system load average metrics (1min, 5min, 15min)
// These values indicate the average number of processes in the run queue
// or waiting for disk I/O over the specified time periods.

use async_trait::async_trait;
use bson::{doc, Document};
use chrono::Utc;
use std::error::Error;
use sysinfo::System;
use tracing::debug;

use super::MetricCollector;

/// Load Average metric collector
///
/// Collects CPU load average values for 1, 5, and 15 minute intervals.
///
/// # Interpretation
/// - Values represent the average number of processes waiting for CPU time
/// - Values < number of CPU cores = system not overloaded
/// - Values > number of CPU cores = system is experiencing high load
/// - Values significantly > cores = system is overloaded
///
/// # Platform Support
/// - Linux: Full support via /proc/loadavg
/// - macOS: Full support via sysctl
/// - Windows: Not available (returns 0.0)
pub struct LoadAverageCollector {
    /// System information provider
    system: System,
}

impl LoadAverageCollector {
    /// Creates a new LoadAverageCollector instance
    pub fn new() -> Self {
        LoadAverageCollector {
            system: System::new(),
        }
    }
}

#[async_trait]
impl MetricCollector for LoadAverageCollector {
    /// Returns the metric name
    fn name(&self) -> &str {
        "LoadAverage"
    }

    /// Collects current load average metrics
    ///
    /// # Returns BSON Document Structure
    /// ```json
    /// {
    ///   "node": "1111-1111",
    ///   "timestamp": "2024-01-15T10:30:00Z",
    ///   "load_1min": 1.5,
    ///   "load_5min": 1.2,
    ///   "load_15min": 0.9,
    ///   "cpu_cores": 8
    /// }
    /// ```
    ///
    /// # Errors
    /// Returns error if system information cannot be retrieved (rare)
    async fn collect(&self, node_id: &str) -> Result<Document, Box<dyn Error + Send + Sync>> {
        debug!("Collecting load average metrics");

        // Get load average values
        // Note: On Windows, these will be 0.0 as load average is not available
        let load_avg = System::load_average();

        // Get CPU count for context
        let cpu_count = num_cpus::get();

        // Create BSON document with load average data
        let doc = doc! {
            // Node identifier (from configuration key)
            "node": node_id,

            // Timestamp when metric was collected (UTC)
            "timestamp": Utc::now(),

            // Load average over 1 minute
            // Useful for detecting immediate spikes in system load
            "load_1min": load_avg.one,

            // Load average over 5 minutes
            // Useful for understanding recent trends
            "load_5min": load_avg.five,

            // Load average over 15 minutes
            // Useful for understanding longer-term system behavior
            "load_15min": load_avg.fifteen,

            // Number of CPU cores for context
            // Helps interpret whether load values are high or normal
            "cpu_cores": cpu_count as i32,
        };

        debug!(
            "Load average: 1min={:.2}, 5min={:.2}, 15min={:.2} (CPUs: {})",
            load_avg.one, load_avg.five, load_avg.fifteen, cpu_count
        );

        Ok(doc)
    }
}

impl Default for LoadAverageCollector {
    fn default() -> Self {
        Self::new()
    }
}
