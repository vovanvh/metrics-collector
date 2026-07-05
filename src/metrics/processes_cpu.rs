// Process CPU snapshot metric collector
//
// Captures the top CPU-consuming host processes to answer the question:
// "Which process was eating CPU when the metric changed?"

use async_trait::async_trait;
use bson::{doc, Document};
use chrono::Utc;
use std::error::Error;
use sysinfo::System;
use tracing::debug;

use super::MetricCollector;

/// Processes below this CPU usage are considered noise and dropped —
/// they add no diagnostic value for root-cause analysis.
const CPU_THRESHOLD_PERCENT: f64 = 1.0;

/// Maximum number of processes stored per snapshot.
const MAX_PROCESSES: usize = 10;

/// Host process CPU snapshot collector
///
/// Refreshes the process list each interval, filters out processes using
/// less than `CPU_THRESHOLD_PERCENT` CPU, sorts by CPU usage descending,
/// and stores at most `MAX_PROCESSES`. Covers non-Docker, kernel, and system
/// service processes that the Docker stats collector cannot see.
pub struct ProcessCPUSnapshotCollector;

impl ProcessCPUSnapshotCollector {
    pub fn new() -> Self {
        ProcessCPUSnapshotCollector
    }
}

#[async_trait]
impl MetricCollector for ProcessCPUSnapshotCollector {
    fn name(&self) -> &str {
        "ProcessCPUSnapshot"
    }

    async fn collect(&self, node_id: &str) -> Result<Document, Box<dyn Error + Send + Sync>> {
        debug!("Collecting process CPU snapshot");

        let mut sys = System::new();
        sys.refresh_memory();
        sys.refresh_processes();

        let total_memory = sys.total_memory();

        let mut processes: Vec<_> = sys
            .processes()
            .values()
            .filter(|p| p.cpu_usage() as f64 > CPU_THRESHOLD_PERCENT)
            .collect();

        processes.sort_by(|a, b| {
            b.cpu_usage()
                .partial_cmp(&a.cpu_usage())
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let top_processes: Vec<Document> = processes
            .iter()
            .take(MAX_PROCESSES)
            .map(|p| {
                doc! {
                    "pid": p.pid().as_u32() as i64,
                    "name": p.name().to_string(),
                    "cpu_percent": p.cpu_usage() as f64,
                    "memory_mb": p.memory() as f64 / (1024.0 * 1024.0),
                    "memory_percent": calculate_percentage(p.memory(), total_memory),
                    "status": format!("{:?}", p.status()),
                }
            })
            .collect();

        debug!(
            "Collected {} process(es) above {}% CPU (of {} total)",
            top_processes.len(),
            CPU_THRESHOLD_PERCENT,
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

impl Default for ProcessCPUSnapshotCollector {
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
