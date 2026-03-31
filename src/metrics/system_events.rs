// System events metric collector
//
// Reads kernel and systemd error events via journalctl each interval.
// Answers: "Did the OOM-killer fire? Did a service crash?"
// Linux/systemd only — gracefully returns empty events on other platforms.

use async_trait::async_trait;
use bson::{doc, Document};
use chrono::{DateTime, TimeZone, Utc};
use std::error::Error;
use std::process::Command;
use tokio::sync::Mutex;
use tracing::{debug, warn};

use super::MetricCollector;

/// System journal event collector
///
/// Runs `journalctl --since @<unix_ts> -p err --output=json --no-pager`
/// each interval and parses the JSON lines into a batch document.
/// If journalctl is not available (non-Linux, no systemd), logs a warning
/// and stores an empty events array rather than failing.
pub struct SystemEventsCollector {
    /// Tracks the end time of the previous poll window
    last_poll: Mutex<Option<DateTime<Utc>>>,
}

impl SystemEventsCollector {
    pub fn new() -> Self {
        SystemEventsCollector {
            last_poll: Mutex::new(None),
        }
    }
}

#[async_trait]
impl MetricCollector for SystemEventsCollector {
    fn name(&self) -> &str {
        "SystemEvents"
    }

    async fn collect(&self, node_id: &str) -> Result<Document, Box<dyn Error + Send + Sync>> {
        debug!("Collecting system events");

        let now = Utc::now();
        let mut last_poll = self.last_poll.lock().await;
        // On first run, look back 60 seconds
        let since = last_poll.unwrap_or_else(|| now - chrono::Duration::seconds(60));
        *last_poll = Some(now);
        drop(last_poll);

        let since_unix = since.timestamp();

        let events = match Command::new("journalctl")
            .args([
                &format!("--since=@{}", since_unix),
                "-p",
                "err",
                "--output=json",
                "--no-pager",
            ])
            .output()
        {
            Err(_) => {
                // journalctl not found — expected on macOS/Windows (no systemd)
                debug!("journalctl not available on this platform, skipping system events");
                Vec::new()
            }
            Ok(output) => {
                if !output.status.success() && output.stdout.is_empty() {
                    warn!("journalctl exited with status {}", output.status);
                    Vec::new()
                } else {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    parse_journalctl_json(&stdout, now)
                }
            }
        };

        debug!("Collected {} system event(s)", events.len());

        let doc = doc! {
            "node": node_id,
            "timestamp": Utc::now(),
            "events": events,
        };

        Ok(doc)
    }
}

/// Parses newline-delimited JSON output from `journalctl --output=json`.
///
/// Each line is a self-contained JSON object. Relevant fields:
/// - `__REALTIME_TIMESTAMP` — microseconds since epoch (string)
/// - `PRIORITY`             — syslog priority 0-7 (string)
/// - `_SYSTEMD_UNIT`        — originating systemd unit
/// - `MESSAGE`              — log message text
/// - `_HOSTNAME`            — source hostname
fn parse_journalctl_json(output: &str, fallback_time: DateTime<Utc>) -> Vec<Document> {
    let mut events = Vec::new();

    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let Ok(json) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };

        // __REALTIME_TIMESTAMP is microseconds since epoch as a decimal string
        let event_time = json["__REALTIME_TIMESTAMP"]
            .as_str()
            .and_then(|ts| ts.parse::<i64>().ok())
            .and_then(|us| {
                Utc.timestamp_opt(us / 1_000_000, ((us % 1_000_000) * 1_000) as u32)
                    .single()
            })
            .unwrap_or(fallback_time)
            .to_rfc3339();

        let priority = json["PRIORITY"]
            .as_str()
            .and_then(|p| p.parse::<i32>().ok())
            .unwrap_or(3);

        let unit = json["_SYSTEMD_UNIT"]
            .as_str()
            .or_else(|| json["UNIT"].as_str())
            .unwrap_or("unknown")
            .to_string();

        let message = json["MESSAGE"].as_str().unwrap_or("").to_string();

        let hostname = json["_HOSTNAME"].as_str().unwrap_or("unknown").to_string();

        events.push(doc! {
            "event_time": event_time,
            "priority": priority,
            "unit": unit,
            "message": message,
            "hostname": hostname,
        });
    }

    events
}

impl Default for SystemEventsCollector {
    fn default() -> Self {
        Self::new()
    }
}
