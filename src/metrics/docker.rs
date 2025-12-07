// Docker stats metric collector
//
// Collects resource usage statistics for all running Docker containers
// Focuses on CPU and memory consumption per container

use async_trait::async_trait;
use bollard::container::StatsOptions;
use bollard::Docker;
use bson::{doc, Document};
use chrono::Utc;
use futures_util::stream::StreamExt;
use std::error::Error;
use tracing::{debug, warn};

use super::MetricCollector;

/// Docker container stats collector
///
/// Collects resource usage metrics for all running Docker containers.
/// Particularly focused on CPU and RAM consumption per container.
///
/// # What is Collected
/// For each running container:
/// - Container ID and name
/// - CPU usage percentage
/// - Memory usage (current, limit, percentage)
/// - Network I/O (bytes sent/received)
/// - Block I/O (bytes read/written)
///
/// # Requirements
/// - Docker daemon must be running
/// - User must have permissions to access Docker socket
/// - Default socket: unix:///var/run/docker.sock (Linux/macOS)
/// - Default socket: npipe:////./pipe/docker_engine (Windows)
///
/// # Platform Support
/// - Linux: Full support
/// - macOS: Full support (Docker Desktop)
/// - Windows: Full support (Docker Desktop)
pub struct DockerCollector {
    /// Docker client instance
    /// Uses default connection (Unix socket on Linux/macOS)
    docker: Docker,
}

impl DockerCollector {
    /// Creates a new DockerCollector instance
    ///
    /// Attempts to connect to Docker using the default socket.
    /// Falls back to environment variables if default connection fails.
    pub fn new() -> Self {
        // Try to connect to Docker using default socket
        // On Linux/macOS: /var/run/docker.sock
        // On Windows: npipe:////./pipe/docker_engine
        let docker = Docker::connect_with_socket_defaults()
            .unwrap_or_else(|_| {
                // Fallback: try to connect using environment variables
                // Checks DOCKER_HOST, DOCKER_CERT_PATH, DOCKER_TLS_VERIFY
                Docker::connect_with_local_defaults()
                    .expect("Failed to connect to Docker daemon")
            });

        DockerCollector { docker }
    }

    /// Converts bytes to megabytes for more readable storage
    fn bytes_to_mb(bytes: u64) -> f64 {
        bytes as f64 / (1024.0 * 1024.0)
    }

    /// Calculates CPU usage percentage from Docker stats
    ///
    /// Docker provides cumulative CPU usage in nanoseconds.
    /// We calculate the percentage based on system CPU stats.
    ///
    /// # Formula
    /// cpu_percent = (cpu_delta / system_cpu_delta) * num_cpus * 100.0
    fn calculate_cpu_percent(stats: &bollard::container::Stats) -> f64 {
        // Get CPU usage values
        let cpu_total = stats
            .cpu_stats
            .cpu_usage
            .total_usage as f64;

        let precpu_total = stats
            .precpu_stats
            .cpu_usage
            .total_usage as f64;

        let system_cpu = stats.cpu_stats.system_cpu_usage.unwrap_or(0) as f64;
        let presystem_cpu = stats.precpu_stats.system_cpu_usage.unwrap_or(0) as f64;

        // Calculate deltas
        let cpu_delta = cpu_total - precpu_total;
        let system_delta = system_cpu - presystem_cpu;

        // Avoid division by zero
        if system_delta <= 0.0 || cpu_delta <= 0.0 {
            return 0.0;
        }

        // Get number of CPUs
        let num_cpus = stats
            .cpu_stats
            .online_cpus
            .unwrap_or_else(|| num_cpus::get() as u64) as f64;

        // Calculate percentage
        (cpu_delta / system_delta) * num_cpus * 100.0
    }
}

#[async_trait]
impl MetricCollector for DockerCollector {
    /// Returns the metric name
    fn name(&self) -> &str {
        "DockerStats"
    }

    /// Collects current Docker container statistics
    ///
    /// # Returns BSON Document Structure
    /// ```json
    /// {
    ///   "node": "1111-1111",
    ///   "timestamp": "2024-01-15T10:30:00Z",
    ///   "containers": [
    ///     {
    ///       "id": "abc123...",
    ///       "name": "my-app",
    ///       "cpu_percent": 25.5,
    ///       "memory_used_mb": 512.0,
    ///       "memory_limit_mb": 2048.0,
    ///       "memory_percent": 25.0,
    ///       "network_rx_mb": 10.5,
    ///       "network_tx_mb": 5.2,
    ///       "block_read_mb": 100.0,
    ///       "block_write_mb": 50.0
    ///     }
    ///   ]
    /// }
    /// ```
    ///
    /// # Errors
    /// Returns error if:
    /// - Docker daemon is not running
    /// - Permission denied to access Docker socket
    /// - Network error communicating with Docker
    ///
    /// # Behavior on Error
    /// If Docker is unavailable, returns an error rather than empty data.
    /// This allows the scheduler to log the issue and skip this metric.
    async fn collect(&self, node_id: &str) -> Result<Document, Box<dyn Error + Send + Sync>> {
        debug!("Collecting Docker container statistics");

        // List all running containers
        let containers = match self.docker.list_containers::<String>(None).await {
            Ok(containers) => containers,
            Err(e) => {
                warn!("Failed to list Docker containers: {}", e);
                return Err(Box::new(e));
            }
        };

        let container_count = containers.len();
        debug!("Found {} running container(s)", container_count);

        // Collect stats for each container
        let mut container_stats = Vec::new();

        for container in containers {
            let container_id = container.id.clone().unwrap_or_default();
            let container_name = container
                .names
                .and_then(|names| names.first().map(|n| n.trim_start_matches('/').to_string()))
                .unwrap_or_else(|| "unknown".to_string());

            debug!("Collecting stats for container: {}", container_name);

            // Get container stats (one-shot, not streaming)
            let stats_options = StatsOptions {
                stream: false, // Get single snapshot, not continuous stream
                ..Default::default()
            };

            let mut stats_stream = self.docker.stats(&container_id, Some(stats_options));

            // Get the first (and only) stats snapshot
            if let Some(stats_result) = stats_stream.next().await {
                match stats_result {
                    Ok(stats) => {
                        // Calculate CPU percentage
                        let cpu_percent = Self::calculate_cpu_percent(&stats);

                        // Get memory stats
                        let memory_used = stats.memory_stats.usage.unwrap_or(0);
                        let memory_limit = stats.memory_stats.limit.unwrap_or(1);
                        let memory_percent = if memory_limit > 0 {
                            (memory_used as f64 / memory_limit as f64) * 100.0
                        } else {
                            0.0
                        };

                        // Get network I/O stats
                        // Sum all network interfaces
                        let (network_rx, network_tx) = stats
                            .networks
                            .as_ref()
                            .map(|networks| {
                                networks.values().fold((0u64, 0u64), |(rx, tx), net| {
                                    (
                                        rx + net.rx_bytes,
                                        tx + net.tx_bytes,
                                    )
                                })
                            })
                            .unwrap_or((0, 0));

                        // Get block I/O stats
                        let (block_read, block_write) = stats
                            .blkio_stats
                            .io_service_bytes_recursive
                            .as_ref()
                            .map(|io_stats| {
                                io_stats.iter().fold((0u64, 0u64), |(read, write), stat| {
                                    match stat.op.as_str() {
                                        "read" | "Read" => (read + stat.value, write),
                                        "write" | "Write" => (read, write + stat.value),
                                        _ => (read, write),
                                    }
                                })
                            })
                            .unwrap_or((0, 0));

                        // Create container stats document
                        let container_doc = doc! {
                            // Container unique identifier (short format)
                            "id": &container_id[..12.min(container_id.len())],

                            // Container name (without leading slash)
                            "name": container_name.clone(),

                            // CPU usage as percentage of total system CPU
                            // e.g., 50% means using half of one CPU core
                            "cpu_percent": cpu_percent,

                            // Current memory usage in MB
                            "memory_used_mb": Self::bytes_to_mb(memory_used),

                            // Memory limit configured for container in MB
                            "memory_limit_mb": Self::bytes_to_mb(memory_limit),

                            // Memory usage as percentage of limit
                            "memory_percent": memory_percent,

                            // Total bytes received over network (all interfaces)
                            "network_rx_mb": Self::bytes_to_mb(network_rx),

                            // Total bytes transmitted over network (all interfaces)
                            "network_tx_mb": Self::bytes_to_mb(network_tx),

                            // Total bytes read from block devices
                            "block_read_mb": Self::bytes_to_mb(block_read),

                            // Total bytes written to block devices
                            "block_write_mb": Self::bytes_to_mb(block_write),
                        };

                        debug!(
                            "Container {}: CPU={:.1}%, Mem={:.1}/{:.1}MB ({:.1}%)",
                            container_name,
                            cpu_percent,
                            Self::bytes_to_mb(memory_used),
                            Self::bytes_to_mb(memory_limit),
                            memory_percent
                        );

                        container_stats.push(container_doc);
                    }
                    Err(e) => {
                        warn!("Failed to get stats for container {}: {}", container_name, e);
                        // Continue with other containers even if one fails
                    }
                }
            }
        }

        // Create main document with array of all container stats
        let doc = doc! {
            // Node identifier (from configuration key)
            "node": node_id,

            // Timestamp when metric was collected (UTC)
            "timestamp": Utc::now(),

            // Array of container statistics
            // One entry per running container
            "containers": container_stats,
        };

        debug!("Collected stats for {} container(s)", container_count);

        Ok(doc)
    }
}

impl Default for DockerCollector {
    fn default() -> Self {
        Self::new()
    }
}
