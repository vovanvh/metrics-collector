// Scheduler module - manages periodic metric collection and aggregated storage
//
// Each metric runs a dual-timer loop:
//   - collect_timer: fires every collect_timeout seconds, pushes sample to buffer
//   - flush_sleep:   fires after store_timeout seconds, writes aggregated doc to MongoDB
//
// After each successful flush, settings are reloaded from MongoDB so that
// timeout changes take effect on the next window.

use std::sync::Arc;
use std::time::Duration;
use tokio::time::interval;
use tokio::select;
use tracing::{error, info, warn};

use crate::aggregator::{DockerMetricBuffer, MetricBuffer};
use crate::config::{ConfigManager, MonitoringSettings};
use crate::metrics::MetricCollector;
use crate::storage::MetricStorage;

/// Maps a metric name to its hardcoded MongoDB collection name.
fn collection_for(metric_name: &str) -> &'static str {
    match metric_name {
        "LoadAverage"        => "load_average_metrics",
        "Memory"             => "memory_metrics",
        "DiskSpace"          => "disk_metrics",
        "DockerStats"        => "docker_metrics",
        "ProcessCPUSnapshot" => "process_cpu_logs",
        "ProcessRAMSnapshot" => "process_ram_logs",
        "DockerEvents"       => "docker_event_logs",
        "DockerLogs"         => "docker_container_logs",
        "SystemEvents"       => "system_event_logs",
        _                    => "unknown_metrics",
    }
}

/// Metrics that are unaggregatable log/event snapshots — no numeric fields to
/// average, so each collected document is written as-is instead of being
/// buffered and flushed once per `store_timeout` window.
fn is_log_metric(metric_name: &str) -> bool {
    matches!(
        metric_name,
        "ProcessCPUSnapshot" | "ProcessRAMSnapshot" | "DockerEvents" | "DockerLogs" | "SystemEvents"
    )
}

/// Returns the collection interval (seconds) that applies to a given metric.
/// Anything that talks to the Docker daemon (stats, events, container logs)
/// shares `collect_docker_timeout` so they don't hit it at different rates;
/// everything else uses the general `collect_timeout`.
fn collect_timeout_for(metric_name: &str, settings: &MonitoringSettings) -> u64 {
    match metric_name {
        "DockerStats" | "DockerEvents" | "DockerLogs" => settings.collect_docker_timeout,
        _ => settings.collect_timeout,
    }
}

pub struct MetricScheduler {
    config_manager: Arc<ConfigManager>,
    storage: Arc<MetricStorage>,
    node_id: String,
}

impl MetricScheduler {
    pub fn new(
        config_manager: ConfigManager,
        storage: MetricStorage,
        node_id: String,
    ) -> Self {
        MetricScheduler {
            config_manager: Arc::new(config_manager),
            storage: Arc::new(storage),
            node_id,
        }
    }

    /// Starts all metric collection tasks. Runs until all tasks stop (should be forever).
    pub async fn start(self, collectors: Vec<Box<dyn MetricCollector>>, initial_settings: MonitoringSettings) {
        info!("Starting metric scheduler for node: {}", self.node_id);

        let mut handles = Vec::new();

        for collector in collectors {
            let metric_name = collector.name().to_string();
            let storage      = Arc::clone(&self.storage);
            let config_mgr   = Arc::clone(&self.config_manager);
            let node_id      = self.node_id.clone();
            let settings     = initial_settings.clone();

            info!(
                "Scheduling '{}' → collection '{}' (collect: {}s, store: {}s)",
                metric_name,
                collection_for(&metric_name),
                collect_timeout_for(&metric_name, &settings),
                settings.store_timeout,
            );

            let handle = if metric_name == "DockerStats" {
                tokio::spawn(async move {
                    run_docker_task(collector, storage, config_mgr, node_id, settings).await;
                })
            } else if is_log_metric(&metric_name) {
                tokio::spawn(async move {
                    run_log_task(collector, storage, config_mgr, node_id, settings).await;
                })
            } else {
                tokio::spawn(async move {
                    run_standard_task(collector, storage, config_mgr, node_id, settings).await;
                })
            };

            handles.push(handle);
        }

        info!("Started {} metric collection task(s)", handles.len());

        for handle in handles {
            if let Err(e) = handle.await {
                error!("Metric collection task panicked: {}", e);
            }
        }

        error!("All metric collection tasks have stopped");
    }

    /// One-shot collection for all metrics (testing/manual use). Stores raw samples directly.
    #[allow(dead_code)]
    pub async fn collect_once(&self, collectors: Vec<Box<dyn MetricCollector>>) -> usize {
        info!("Running one-time metric collection");

        let mut success_count = 0;
        let total_count = collectors.len();

        for collector in collectors {
            let metric_name = collector.name();
            let collection  = collection_for(metric_name);

            match collector.collect(&self.node_id).await {
                Ok(document) => {
                    self.storage
                        .store_metric_safe(collection, metric_name, document)
                        .await;
                    success_count += 1;
                }
                Err(e) => {
                    error!("Failed to collect metric '{}': {}", metric_name, e);
                }
            }
        }

        info!("One-time collection complete: {}/{} metrics succeeded", success_count, total_count);
        success_count
    }
}

/// Collection + aggregation loop for LoadAverage, Memory, DiskSpace.
async fn run_standard_task(
    collector: Box<dyn MetricCollector>,
    storage: Arc<MetricStorage>,
    config_manager: Arc<ConfigManager>,
    node_id: String,
    mut settings: MonitoringSettings,
) {
    let metric_name = collector.name();
    let collection  = collection_for(metric_name);
    let mut buffer  = MetricBuffer::new();

    info!("Starting collection loop for '{}'", metric_name);

    loop {
        let mut collect_timer = interval(Duration::from_secs(settings.collect_timeout));
        let flush_sleep = tokio::time::sleep(Duration::from_secs(settings.store_timeout));
        tokio::pin!(flush_sleep);

        // Inner loop: collect until flush deadline
        loop {
            select! {
                _ = collect_timer.tick() => {
                    match collector.collect(&node_id).await {
                        Ok(doc) => buffer.push(&doc),
                        Err(e)  => error!("Failed to collect '{}': {}", metric_name, e),
                    }
                }
                _ = &mut flush_sleep => { break; }
            }
        }

        // Flush buffer and store
        match buffer.flush(&node_id) {
            Some(doc) => {
                storage.store_metric_safe(collection, metric_name, doc).await;
                // Reload settings right after storing
                match config_manager.reload_settings(&node_id).await {
                    Ok(new)  => settings = new,
                    Err(e)   => warn!("Failed to reload settings for '{}': {}", metric_name, e),
                }
            }
            None => warn!("Not enough samples for '{}', skipping flush", metric_name),
        }
    }
}

/// Collection loop for log/event snapshots (ProcessCPUSnapshot, ProcessRAMSnapshot,
/// DockerEvents, DockerLogs, SystemEvents).
///
/// Unlike `run_standard_task`, there is no buffering or aggregation — these documents
/// have no numeric fields to average, so each collected tick is written to MongoDB
/// as its own document. Settings are still reloaded on the `store_timeout` cadence
/// to pick up `collect_timeout` changes without needing a restart.
async fn run_log_task(
    collector: Box<dyn MetricCollector>,
    storage: Arc<MetricStorage>,
    config_manager: Arc<ConfigManager>,
    node_id: String,
    mut settings: MonitoringSettings,
) {
    let metric_name = collector.name();
    let collection  = collection_for(metric_name);

    info!("Starting log collection loop for '{}'", metric_name);

    loop {
        let mut collect_timer = interval(Duration::from_secs(collect_timeout_for(metric_name, &settings)));
        let reload_sleep = tokio::time::sleep(Duration::from_secs(settings.store_timeout));
        tokio::pin!(reload_sleep);

        loop {
            select! {
                _ = collect_timer.tick() => {
                    match collector.collect(&node_id).await {
                        Ok(doc) => storage.store_metric_safe(collection, metric_name, doc).await,
                        Err(e)  => error!("Failed to collect '{}': {}", metric_name, e),
                    }
                }
                _ = &mut reload_sleep => { break; }
            }
        }

        match config_manager.reload_settings(&node_id).await {
            Ok(new) => settings = new,
            Err(e)  => warn!("Failed to reload settings for '{}': {}", metric_name, e),
        }
    }
}

/// Collection + aggregation loop for DockerStats.
async fn run_docker_task(
    collector: Box<dyn MetricCollector>,
    storage: Arc<MetricStorage>,
    config_manager: Arc<ConfigManager>,
    node_id: String,
    mut settings: MonitoringSettings,
) {
    let metric_name = collector.name();
    let collection  = collection_for(metric_name);
    let mut buffer  = DockerMetricBuffer::new();

    info!("Starting collection loop for '{}'", metric_name);

    loop {
        let mut collect_timer = interval(Duration::from_secs(settings.collect_docker_timeout));
        let flush_sleep = tokio::time::sleep(Duration::from_secs(settings.store_timeout));
        tokio::pin!(flush_sleep);

        loop {
            select! {
                _ = collect_timer.tick() => {
                    match collector.collect(&node_id).await {
                        Ok(doc) => buffer.push(&doc),
                        Err(e)  => {
                            error!("Failed to collect '{}': {}", metric_name, e);
                            warn!(
                                "Docker may not be running or accessible. \
                                 Ensure Docker daemon is running and this process has \
                                 permission to access the Docker socket."
                            );
                        }
                    }
                }
                _ = &mut flush_sleep => { break; }
            }
        }

        match buffer.flush(&node_id) {
            Some(doc) => {
                storage.store_metric_safe(collection, metric_name, doc).await;
                match config_manager.reload_settings(&node_id).await {
                    Ok(new)  => settings = new,
                    Err(e)   => warn!("Failed to reload settings for '{}': {}", metric_name, e),
                }
            }
            None => warn!("Not enough samples for '{}', skipping flush", metric_name),
        }
    }
}
