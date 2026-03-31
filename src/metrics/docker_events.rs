// Docker events metric collector
//
// Polls the Docker event stream each interval to record container lifecycle
// events: start, stop, die, restart, OOM-kill, etc.

use async_trait::async_trait;
use bollard::system::EventsOptions;
use bollard::Docker;
use bson::{doc, Document};
use chrono::{DateTime, TimeZone, Utc};
use futures_util::stream::StreamExt;
use std::collections::HashMap;
use std::error::Error;
use tokio::sync::Mutex;
use tracing::{debug, warn};

use super::MetricCollector;

/// Docker lifecycle event collector
///
/// On each tick, fetches all Docker events in the window
/// [last_poll_time, now] using the bollard `events()` API.
/// Stores one document per interval (even if the events array is empty)
/// so that the absence of events is also recorded.
pub struct DockerEventsCollector {
    docker: Docker,
    /// Tracks the end time of the previous poll window
    last_poll: Mutex<Option<DateTime<Utc>>>,
}

impl DockerEventsCollector {
    pub fn new() -> Self {
        let docker = Docker::connect_with_socket_defaults().unwrap_or_else(|_| {
            Docker::connect_with_local_defaults().expect("Failed to connect to Docker daemon")
        });
        DockerEventsCollector {
            docker,
            last_poll: Mutex::new(None),
        }
    }
}

#[async_trait]
impl MetricCollector for DockerEventsCollector {
    fn name(&self) -> &str {
        "DockerEvents"
    }

    async fn collect(&self, node_id: &str) -> Result<Document, Box<dyn Error + Send + Sync>> {
        debug!("Collecting Docker events");

        let now = Utc::now();
        let mut last_poll = self.last_poll.lock().await;
        // On first run, look back 60 seconds
        let since = last_poll.unwrap_or_else(|| now - chrono::Duration::seconds(60));
        *last_poll = Some(now);
        drop(last_poll);

        let options = EventsOptions {
            since: Some(since.timestamp().to_string()),
            until: Some(now.timestamp().to_string()),
            filters: HashMap::<String, Vec<String>>::new(),
        };

        let mut events_stream = self.docker.events(Some(options));
        let mut events: Vec<Document> = Vec::new();

        while let Some(event_result) = events_stream.next().await {
            match event_result {
                Ok(event) => {
                    let event_time = event
                        .time
                        .and_then(|t| Utc.timestamp_opt(t, 0).single())
                        .unwrap_or(now)
                        .to_rfc3339();

                    let container_id = event
                        .actor
                        .as_ref()
                        .and_then(|a| a.id.clone())
                        .unwrap_or_default();

                    let container_name = event
                        .actor
                        .as_ref()
                        .and_then(|a| a.attributes.as_ref())
                        .and_then(|attrs| attrs.get("name"))
                        .cloned()
                        .unwrap_or_else(|| "unknown".to_string());

                    let action = event.action.unwrap_or_else(|| "unknown".to_string());

                    let exit_code: Option<i32> = event
                        .actor
                        .as_ref()
                        .and_then(|a| a.attributes.as_ref())
                        .and_then(|attrs| attrs.get("exitCode"))
                        .and_then(|code| code.parse().ok());

                    let short_id = &container_id[..12.min(container_id.len())];

                    let mut event_doc = doc! {
                        "event_time": event_time,
                        "container_id": short_id,
                        "container_name": container_name,
                        "action": action,
                    };

                    if let Some(code) = exit_code {
                        event_doc.insert("exit_code", code);
                    }

                    events.push(event_doc);
                }
                Err(e) => {
                    warn!("Error reading Docker event: {}", e);
                }
            }
        }

        debug!("Collected {} Docker event(s)", events.len());

        let doc = doc! {
            "node": node_id,
            "timestamp": Utc::now(),
            "events": events,
        };

        Ok(doc)
    }
}

impl Default for DockerEventsCollector {
    fn default() -> Self {
        Self::new()
    }
}
