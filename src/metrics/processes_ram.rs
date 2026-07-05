// Process RAM snapshot metric collector
//
// Captures the top memory-consuming host processes to answer the question:
// "Which process was eating RAM when the metric changed?"

use async_trait::async_trait;
use bson::{doc, Document};
use chrono::Utc;
use std::error::Error;
use sysinfo::System;
use tracing::debug;

use super::MetricCollector;

/// Processes using less than this percentage of total system RAM are
/// considered noise and dropped — they add no diagnostic value for
/// root-cause analysis.
const MEMORY_THRESHOLD_PERCENT: f64 = 1.0;

/// Maximum number of processes stored per snapshot.
const MAX_PROCESSES: usize = 10;

/// Host process RAM snapshot collector
///
/// Refreshes the process list each interval, filters out processes using
/// less than `MEMORY_THRESHOLD_PERCENT` of total system RAM, sorts by
/// memory usage descending, and stores at most `MAX_PROCESSES`. Covers
/// non-Docker, kernel, and system service processes that the Docker stats
/// collector cannot see.
pub struct ProcessRAMSnapshotCollector;

impl ProcessRAMSnapshotCollector {
    pub fn new() -> Self {
        ProcessRAMSnapshotCollector
    }
}

#[async_trait]
impl MetricCollector for ProcessRAMSnapshotCollector {
    fn name(&self) -> &str {
        "ProcessRAMSnapshot"
    }

    async fn collect(&self, node_id: &str) -> Result<Document, Box<dyn Error + Send + Sync>> {
        debug!("Collecting process RAM snapshot");

        let mut sys = System::new();
        sys.refresh_memory();
        sys.refresh_processes();

        let total_memory = sys.total_memory();

        let mut processes: Vec<_> = sys
            .processes()
            .values()
            .filter(|p| calculate_percentage(p.memory(), total_memory) > MEMORY_THRESHOLD_PERCENT)
            .collect();

        processes.sort_by(|a, b| b.memory().cmp(&a.memory()));

        let top_processes: Vec<Document> = processes
            .iter()
            .take(MAX_PROCESSES)
            .map(|p| {
                doc! {
                    "pid": p.pid().as_u32() as i64,
                    "name": p.name().to_string(),
                    "memory_mb": p.memory() as f64 / (1024.0 * 1024.0),
                    "memory_percent": calculate_percentage(p.memory(), total_memory),
                    "cpu_percent": p.cpu_usage() as f64,
                    "status": format!("{:?}", p.status()),
                }
            })
            .collect();

        debug!(
            "Collected {} process(es) above {}% RAM (of {} total)",
            top_processes.len(),
            MEMORY_THRESHOLD_PERCENT,
            sys.processes().len()
        );

        let doc = doc! {
            "node": node_id,
            "timestamp": Utc::now(),
            "processes": top_processes,
        };

        Ok(doc)
    }
}

impl Default for ProcessRAMSnapshotCollector {
    fn default() -> Self {
        Self::new()
    }
}

fn calculate_percentage(used: u64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        (used as f64 / total as f64) * 100.0
    }
}
