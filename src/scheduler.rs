// Scheduler module - manages periodic metric collection tasks
//
// This module implements the core scheduling logic using Tokio tasks.
// Each metric collector runs on its own independent interval as specified
// in the MongoDB configuration.
//
// # Architecture
// - Uses Tokio's interval timer for periodic execution
// - Each metric runs in its own async task
// - Tasks run concurrently and independently
// - Failures in one metric don't affect others

use std::sync::Arc;
use std::time::Duration;
use tokio::time::interval;
use tracing::{error, info, warn};

use crate::config::MonitoringSettings;
use crate::metrics::MetricCollector;
use crate::storage::MetricStorage;

/// Metric scheduler that manages periodic collection tasks
///
/// The scheduler creates independent Tokio tasks for each metric type,
/// each running at its own configured interval. This approach provides:
/// - Independent execution (one metric failure doesn't affect others)
/// - Precise timing (each metric runs exactly at its configured interval)
/// - Efficient resource usage (async tasks are lightweight)
pub struct MetricScheduler {
    /// Configuration loaded from MongoDB
    settings: Arc<MonitoringSettings>,

    /// Storage manager for persisting metrics
    storage: Arc<MetricStorage>,

    /// Node identifier (from configuration key)
    node_id: String,
}

impl MetricScheduler {
    /// Creates a new MetricScheduler instance
    ///
    /// # Arguments
    /// * `settings` - Monitoring settings loaded from MongoDB
    /// * `storage` - Storage manager for persisting metrics
    /// * `node_id` - Node identifier (typically the configuration key)
    ///
    /// # Example
    /// ```
    /// let scheduler = MetricScheduler::new(settings, storage, "1111-1111");
    /// ```
    pub fn new(
        settings: MonitoringSettings,
        storage: MetricStorage,
        node_id: String,
    ) -> Self {
        MetricScheduler {
            settings: Arc::new(settings),
            storage: Arc::new(storage),
            node_id,
        }
    }

    /// Starts the scheduler and all metric collection tasks
    ///
    /// This method spawns independent Tokio tasks for each configured metric.
    /// Each task runs in an infinite loop, collecting and storing metrics
    /// at its configured interval.
    ///
    /// # Arguments
    /// * `collectors` - Vector of metric collectors to schedule
    ///
    /// # Behavior
    /// - Each metric runs independently in its own task
    /// - Tasks run forever until the program is terminated
    /// - If a metric has no configuration, it is skipped
    /// - Errors in collection/storage are logged but don't stop the task
    ///
    /// # Example
    /// ```
    /// let collectors = metrics::create_all_collectors();
    /// scheduler.start(collectors).await;
    /// ```
    pub async fn start(self, collectors: Vec<Box<dyn MetricCollector>>) {
        info!("Starting metric scheduler for node: {}", self.node_id);

        // Spawn a task for each metric collector
        let mut handles = Vec::new();

        for collector in collectors {
            let metric_name = collector.name().to_string();

            // Get settings for this metric
            let metric_settings = match self.settings.get_metric_settings(&metric_name) {
                Some(settings) => settings.clone(),
                None => {
                    warn!(
                        "No settings found for metric '{}', skipping",
                        metric_name
                    );
                    continue;
                }
            };

            info!(
                "Scheduling metric '{}' with interval of {}s, collection: '{}'",
                metric_name, metric_settings.timeout, metric_settings.collection
            );

            // Clone Arc references for this task
            let storage = Arc::clone(&self.storage);
            let node_id = self.node_id.clone();

            // Spawn independent task for this metric
            let handle = tokio::spawn(async move {
                // Run the metric collection loop
                Self::run_metric_task(
                    collector,
                    storage,
                    node_id,
                    metric_settings.timeout,
                    metric_settings.collection,
                )
                .await;
            });

            handles.push(handle);
        }

        info!(
            "Successfully started {} metric collection task(s)",
            handles.len()
        );

        // Wait for all tasks to complete (they run forever unless there's a critical error)
        for handle in handles {
            if let Err(e) = handle.await {
                error!("Metric collection task panicked: {}", e);
            }
        }

        // If we reach here, all tasks have stopped (shouldn't happen in normal operation)
        error!("All metric collection tasks have stopped");
    }

    /// Runs a single metric collection task in an infinite loop
    ///
    /// This is the core loop for each metric. It:
    /// 1. Waits for the next interval tick
    /// 2. Collects the metric data
    /// 3. Stores the metric in MongoDB
    /// 4. Repeats forever
    ///
    /// # Arguments
    /// * `collector` - The metric collector to run
    /// * `storage` - Storage manager for persisting metrics
    /// * `node_id` - Node identifier to include in metric documents
    /// * `interval_secs` - How often to collect this metric (in seconds)
    /// * `collection_name` - MongoDB collection name for storing this metric
    ///
    /// # Error Handling
    /// - Collection errors are logged but don't stop the task
    /// - Storage errors are logged but don't stop the task
    /// - The task continues running even if individual collections fail
    async fn run_metric_task(
        collector: Box<dyn MetricCollector>,
        storage: Arc<MetricStorage>,
        node_id: String,
        interval_secs: u64,
        collection_name: String,
    ) {
        let metric_name = collector.name();

        info!(
            "Starting collection loop for metric '{}' (every {}s)",
            metric_name, interval_secs
        );

        // Create interval timer
        // tick() waits for the next interval, starting immediately
        let mut interval_timer = interval(Duration::from_secs(interval_secs));

        loop {
            // Wait for the next tick
            interval_timer.tick().await;

            // Collect the metric
            match collector.collect(&node_id).await {
                Ok(document) => {
                    // Successfully collected, now store it
                    storage
                        .store_metric_safe(&collection_name, metric_name, document)
                        .await;
                }
                Err(e) => {
                    // Collection failed, log error and continue
                    error!(
                        "Failed to collect metric '{}': {}",
                        metric_name, e
                    );

                    // For Docker metrics, provide helpful hint if Docker is not available
                    if metric_name == "DockerStats" {
                        warn!(
                            "Docker may not be running or accessible. \
                             Ensure Docker daemon is running and this process has permission \
                             to access the Docker socket."
                        );
                    }
                }
            }
        }
    }

    /// Performs a one-time collection of all metrics (useful for testing)
    ///
    /// This method collects all metrics once without scheduling.
    /// Useful for:
    /// - Testing metric collectors
    /// - Manual metric collection
    /// - Debugging
    ///
    /// # Arguments
    /// * `collectors` - Vector of metric collectors to run
    ///
    /// # Returns
    /// Number of metrics successfully collected and stored
    pub async fn collect_once(&self, collectors: Vec<Box<dyn MetricCollector>>) -> usize {
        info!("Running one-time metric collection");

        let mut success_count = 0;
        let total_count = collectors.len();

        for collector in collectors {
            let metric_name = collector.name();

            // Get settings for this metric
            let metric_settings = match self.settings.get_metric_settings(metric_name) {
                Some(settings) => settings,
                None => {
                    warn!("No settings found for metric '{}', skipping", metric_name);
                    continue;
                }
            };

            info!("Collecting metric '{}'", metric_name);

            // Collect the metric
            match collector.collect(&self.node_id).await {
                Ok(document) => {
                    // Store it
                    self.storage
                        .store_metric_safe(&metric_settings.collection, metric_name, document)
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
