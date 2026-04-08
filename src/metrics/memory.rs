// Memory metric collector
//
// Collects system memory usage metrics including RAM and swap.

use sysinfo::System;
use async_trait::async_trait;
use bson::{doc, Document};
use chrono::Utc;
use std::error::Error;
use tracing::debug;

use super::MetricCollector;

pub struct MemoryCollector {}

impl MemoryCollector {
    pub fn new() -> Self {
        MemoryCollector {}
    }

    fn bytes_to_mb(bytes: u64) -> i64 {
        (bytes / (1024 * 1024)) as i64
    }

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
    fn name(&self) -> &str {
        "Memory"
    }

    /// Collects current memory usage metrics
    ///
    /// # Returns BSON Document Structure
    /// ```json
    /// {
    ///   "node": "0001-0001",
    ///   "timestamp": "...",
    ///   "total_mb": 24048,
    ///   "swap_total_mb": 0,
    ///   "available_mb": 21317,
    ///   "used_percent": 11.35,
    ///   "swap_used_percent": 0.0
    /// }
    /// ```
    async fn collect(&self, node_id: &str) -> Result<Document, Box<dyn Error + Send + Sync>> {
        debug!("Collecting memory metrics");

        let mut sys = System::new();
        sys.refresh_memory();

        let total_memory     = sys.total_memory();
        let available_memory = sys.available_memory();
        let used_memory      = sys.used_memory();
        let total_swap       = sys.total_swap();
        let used_swap        = sys.used_swap();

        let used_percent      = Self::calculate_percentage(used_memory, total_memory);
        let swap_used_percent = Self::calculate_percentage(used_swap, total_swap);

        let doc = doc! {
            "node":             node_id,
            "timestamp":        Utc::now(),
            "total_mb":         Self::bytes_to_mb(total_memory),
            "swap_total_mb":    Self::bytes_to_mb(total_swap),
            "available_mb":     Self::bytes_to_mb(available_memory),
            "used_percent":     used_percent,
            "swap_used_percent": swap_used_percent,
        };

        debug!(
            "Memory: available={} MB, used={:.1}%, swap={:.1}%",
            Self::bytes_to_mb(available_memory),
            used_percent,
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
