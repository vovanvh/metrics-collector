// Process snapshot metric collector
//
// Captures the top CPU-consuming host processes to answer the question:
// "Which process was eating CPU/memory when the metric changed?"

use async_trait::async_trait;
use bson::{doc, Document};
use chrono::Utc;
use std::error::Error;
use sysinfo::System;
use tracing::debug;

use super::MetricCollector;

/// Host process snapshot collector
///
/// Refreshes the process list each interval, sorts by CPU usage descending,
/// and stores the top 20 processes. Covers non-Docker, kernel, and system
/// service processes that the Docker stats collector cannot see.
pub struct ProcessSnapshotCollector;

impl ProcessSnapshotCollector {
    pub fn new() -> Self {
        ProcessSnapshotCollector
    }
}

#[async_trait]
impl MetricCollector for ProcessSnapshotCollector {
    fn name(&self) -> &str {
        "ProcessSnapshot"
    }

    async fn collect(&self, node_id: &str) -> Result<Document, Box<dyn Error + Send + Sync>> {
        debug!("Collecting process snapshot");

        let mut sys = System::new();
        sys.refresh_processes();

        // Collect all processes, sort by CPU descending, take top 20
        let mut processes: Vec<_> = sys.processes().values().collect();
        processes.sort_by(|a, b| {
            b.cpu_usage()
                .partial_cmp(&a.cpu_usage())
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let top_processes: Vec<Document> = processes
            .iter()
            .take(20)
            .map(|p| {
                doc! {
                    "pid": p.pid().as_u32() as i64,
                    "name": p.name().to_string(),
                    "cpu_percent": p.cpu_usage() as f64,
                    "memory_mb": p.memory() as f64 / (1024.0 * 1024.0),
                    "status": format!("{:?}", p.status()),
                }
            })
            .collect();

        debug!(
            "Collected snapshot of {} process(es) (top 20 by CPU)",
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

impl Default for ProcessSnapshotCollector {
    fn default() -> Self {
        Self::new()
    }
}
