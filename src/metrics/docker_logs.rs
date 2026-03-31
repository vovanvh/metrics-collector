// Docker logs metric collector
//
// Fetches stdout/stderr from all running containers each interval.
// Stored alongside metrics to answer: "What was the application logging
// when the metric spike occurred?"

use async_trait::async_trait;
use bollard::container::{LogOutput, LogsOptions};
use bollard::Docker;
use bson::{doc, Document};
use chrono::{DateTime, Utc};
use futures_util::stream::StreamExt;
use std::error::Error;
use tokio::sync::Mutex;
use tracing::{debug, warn};

use super::MetricCollector;

/// Maximum total log lines stored per interval across all containers.
/// Prevents document bloat from noisy containers.
const MAX_LOG_LINES: usize = 500;

/// Docker container log collector
///
/// Lists all running containers each interval, fetches logs since the last
/// poll for each one, and batches the result into a single document.
pub struct DockerLogsCollector {
    docker: Docker,
    /// Tracks the end time of the previous poll window
    last_poll: Mutex<Option<DateTime<Utc>>>,
}

impl DockerLogsCollector {
    pub fn new() -> Self {
        let docker = Docker::connect_with_socket_defaults().unwrap_or_else(|_| {
            Docker::connect_with_local_defaults().expect("Failed to connect to Docker daemon")
        });
        DockerLogsCollector {
            docker,
            last_poll: Mutex::new(None),
        }
    }
}

#[async_trait]
impl MetricCollector for DockerLogsCollector {
    fn name(&self) -> &str {
        "DockerLogs"
    }

    async fn collect(&self, node_id: &str) -> Result<Document, Box<dyn Error + Send + Sync>> {
        debug!("Collecting Docker logs");

        let now = Utc::now();
        let mut last_poll = self.last_poll.lock().await;
        // On first run, look back 60 seconds
        let since = last_poll.unwrap_or_else(|| now - chrono::Duration::seconds(60));
        *last_poll = Some(now);
        drop(last_poll);

        let since_unix = since.timestamp();

        // List all running containers (same as docker.rs)
        let containers = match self.docker.list_containers::<String>(None).await {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to list Docker containers: {}", e);
                return Err(Box::new(e));
            }
        };

        let mut total_lines = 0usize;
        let mut container_docs: Vec<Document> = Vec::new();

        for container in containers {
            if total_lines >= MAX_LOG_LINES {
                break;
            }

            let container_id = container.id.clone().unwrap_or_default();
            let container_name = container
                .names
                .and_then(|names| names.first().map(|n| n.trim_start_matches('/').to_string()))
                .unwrap_or_else(|| "unknown".to_string());

            let remaining = MAX_LOG_LINES - total_lines;

            let options = LogsOptions::<String> {
                follow: false,
                stdout: false,
                stderr: true,
                since: since_unix,
                until: 0,
                timestamps: true,
                tail: remaining.to_string(),
            };

            let mut logs_stream = self.docker.logs(&container_id, Some(options));
            let mut log_lines: Vec<Document> = Vec::new();
            let mut truncated = false;

            while let Some(log_result) = logs_stream.next().await {
                match log_result {
                    Ok(log_output) => {
                        if total_lines >= MAX_LOG_LINES {
                            truncated = true;
                            break;
                        }

                        let (stream, message_bytes) = match log_output {
                            LogOutput::StdOut { message } => ("stdout", message),
                            LogOutput::StdErr { message } => ("stderr", message),
                            LogOutput::Console { message } => ("console", message),
                            LogOutput::StdIn { message } => ("stdin", message),
                        };

                        let raw = String::from_utf8_lossy(&message_bytes);
                        let raw = raw.trim();
                        if raw.is_empty() {
                            continue;
                        }

                        // Docker prepends an RFC3339 timestamp when timestamps=true:
                        // "2024-01-15T10:30:00.000000000Z actual message"
                        let (time_str, msg) = if let Some(space_pos) = raw.find(' ') {
                            let prefix = &raw[..space_pos];
                            if prefix.contains('T') && prefix.contains('Z') {
                                (prefix.to_string(), raw[space_pos + 1..].to_string())
                            } else {
                                (now.to_rfc3339(), raw.to_string())
                            }
                        } else {
                            (now.to_rfc3339(), raw.to_string())
                        };

                        log_lines.push(doc! {
                            "stream": stream,
                            "time": time_str,
                            "message": msg,
                        });

                        total_lines += 1;
                    }
                    Err(e) => {
                        warn!("Error reading logs for container {}: {}", container_name, e);
                        break;
                    }
                }
            }

            let short_id = &container_id[..12.min(container_id.len())];
            container_docs.push(doc! {
                "container_id": short_id,
                "container_name": container_name,
                "log_lines": log_lines,
                "truncated": truncated,
            });
        }

        debug!(
            "Collected {} log line(s) across {} container(s)",
            total_lines,
            container_docs.len()
        );

        let doc = doc! {
            "node": node_id,
            "timestamp": Utc::now(),
            "containers": container_docs,
        };

        Ok(doc)
    }
}

impl Default for DockerLogsCollector {
    fn default() -> Self {
        Self::new()
    }
}
