# Metrics Collector

A production-ready, extensible server monitoring tool written in Rust that collects system metrics and stores aggregated documents in MongoDB.

## Features

- **Multiple Metric Types**
  - Load Average (1min, 5min, 15min) with avg/min/max per window
  - Memory Usage (RAM and swap) with avg/min/max per window
  - Disk Space (all mounted filesystems, last-sample per window)
  - Docker Container Stats (CPU and memory aggregated, I/O last-sample)

- **60-Second Aggregation Windows** (metrics only)
  - Buffers raw samples in memory; writes one document per minute per metric
  - Each numeric field stored as `{ "avg": …, "min": …, "max": … }`
  - Constant fields (cpu_cores, total_mb, etc.) stored as plain values

- **Log & Event Snapshots** (unaggregated, short-retention)
  - Host process snapshots by CPU and by RAM usage (top 10, filtered to >1% each)
  - Docker container lifecycle events (start, stop, die, OOM-kill, restart)
  - Docker container stdout/stderr log lines, batched per interval
  - Kernel/systemd error events via `journalctl` (Linux only)
  - No averaging — each collected tick is written as its own document, since there's no numeric field to aggregate

- **Live Configuration Reload**
  - Settings re-read from MongoDB after every flush — no restart needed
  - Three shared timeout values control all metrics

- **Extensible Architecture**
  - Trait-based design for adding new metric types
  - Well-documented extension guide

- **Production Ready**
  - SystemD service integration with automatic restart
  - Structured logging (JSON for systemd, pretty for terminal)
  - Resource limits and security hardening

- **High Performance**
  - Async/concurrent execution with Tokio
  - Dual-timer `select!` loop per metric task
  - Minimal resource usage (<1% CPU, <20MB RAM)

## Quick Start

### Prerequisites

- Rust 1.70+ (for building)
- MongoDB 4.4+
- Linux with systemd (for production deployment)
- Docker (optional, for container monitoring)

### Build

```bash
cargo build --release
```

Binary location: `target/release/metrics-collector`

### Configure MongoDB

```javascript
mongosh "mongodb://localhost:27017"
use monitoring

db.MonitoringSettings.insertOne({
  "key": "0001-0001",
  "collect_timeout": 5,           // seconds between samples (LoadAverage, Memory, DiskSpace)
  "collect_docker_timeout": 20,   // seconds between Docker samples
  "store_timeout": 60             // aggregation window length in seconds
})
```

### Run

```bash
./target/release/metrics-collector \
  --mongodb "mongodb://localhost:27017" \
  --key "0001-0001"
```

The first aggregated documents appear after ~65 seconds (one full window).

## Documentation

- **[Deployment Guide](docs/deployment.md)** — Building, MongoDB setup, SystemD service, troubleshooting
- **[Architecture](docs/architecture.md)** — System design, aggregation pipeline, data flow, design patterns
- **[Adding New Metrics](docs/adding-new-metrics.md)** — Step-by-step tutorial with code examples
- **[Rust Intro Guide](docs/rust-intro-guide.md)** — Learn Rust through this project
- **[Rust Cheatsheet](docs/rust-cheatsheet.md)** — Quick reference with project-specific patterns

## Project Structure

```
metrics-collector/
├── Cargo.toml                    # Dependencies and build configuration
├── metrics-collector.service     # SystemD service file
├── README.md                     # This file
│
├── src/
│   ├── main.rs                  # Application entry point
│   ├── config.rs                # MongoDB configuration management + live reload
│   ├── storage.rs               # MongoDB storage operations
│   ├── aggregator.rs            # In-memory buffering and avg/min/max aggregation
│   ├── scheduler.rs             # Dual-timer task scheduler
│   │
│   └── metrics/                 # Metric collectors
│       ├── mod.rs              # MetricCollector trait + factory
│       ├── load_average.rs     # Load average metric
│       ├── memory.rs           # Memory usage metric
│       ├── disk.rs             # Disk space metric
│       ├── docker.rs           # Docker stats metric
│       ├── processes_cpu.rs    # Top host processes by CPU (log, unaggregated)
│       ├── processes_ram.rs    # Top host processes by RAM (log, unaggregated)
│       ├── docker_events.rs    # Docker lifecycle events (log, unaggregated)
│       ├── docker_logs.rs      # Docker container stdout/stderr (log, unaggregated)
│       └── system_events.rs    # Kernel/systemd error events (log, unaggregated)
│
└── docs/
    ├── deployment.md
    ├── architecture.md
    ├── adding-new-metrics.md
    ├── rust-intro-guide.md
    └── rust-cheatsheet.md
```

## Usage

### Command-Line Options

```bash
metrics-collector --mongodb <URI> --key <KEY> [OPTIONS]
```

| Flag | Required | Description |
|------|----------|-------------|
| `--mongodb <URI>` | Yes | MongoDB connection string |
| `--key <KEY>` | Yes | Node identifier (matches `key` in MonitoringSettings) |
| `--database <NAME>` | No | Database name (default: `monitoring`) |
| `--create-indexes` | No | Create `(node, timestamp)` indexes on startup |

### Examples

```bash
# Basic
metrics-collector --mongodb "mongodb://localhost:27017" --key "server-01"

# With authentication
metrics-collector \
  --mongodb "mongodb://user:pass@host:27017/monitoring?authSource=admin" \
  --key "server-01"

# Custom database
metrics-collector \
  --mongodb "mongodb://localhost:27017" \
  --key "server-01" \
  --database "prod_monitoring"

# Create indexes on first run
metrics-collector \
  --mongodb "mongodb://localhost:27017" \
  --key "server-01" \
  --create-indexes
```

### Environment Variables

```bash
RUST_LOG=debug metrics-collector --mongodb "..." --key "..."
```

## Stored Document Formats

### load_average_metrics (one per 60s)
```json
{
  "node": "0001-0001",
  "timestamp": "2026-04-08T12:01:00Z",
  "sample_count": 12,
  "cpu_cores": 8,
  "load_1min":  { "avg": 1.42, "min": 0.80, "max": 2.30 },
  "load_5min":  { "avg": 1.18, "min": 0.90, "max": 1.50 },
  "load_15min": { "avg": 0.95, "min": 0.85, "max": 1.10 }
}
```

### memory_metrics (one per 60s)
```json
{
  "node": "0001-0001",
  "timestamp": "2026-04-08T12:01:00Z",
  "sample_count": 12,
  "total_mb": 24048,
  "swap_total_mb": 0,
  "available_mb":      { "avg": 19200.0, "min": 18000.0, "max": 21000.0 },
  "used_percent":      { "avg": 20.2,    "min": 12.8,    "max": 25.1    },
  "swap_used_percent": { "avg": 0.0,     "min": 0.0,     "max": 0.0     }
}
```

### disk_metrics (one per 60s, last sample of window)
```json
{
  "node": "0001-0001",
  "timestamp": "2026-04-08T12:01:00Z",
  "disks": [
    { "mount_point": "/", "filesystem": "ext4",
      "total_gb": 500.0, "used_gb": 250.0, "available_gb": 250.0, "used_percent": 50.0 }
  ]
}
```

### docker_metrics (one per 60s, 3 samples aggregated)
```json
{
  "node": "0001-0001",
  "timestamp": "2026-04-08T12:01:00Z",
  "sample_count": 3,
  "containers": [
    {
      "id": "531c5b818fe7", "name": "my-app",
      "memory_limit_mb": 2048.0,
      "cpu_percent":    { "avg": 25.1, "min": 18.0, "max": 42.5 },
      "memory_used_mb": { "avg": 512.0, "min": 498.0, "max": 530.0 },
      "memory_percent": { "avg": 25.0, "min": 24.3, "max": 25.9 },
      "network_rx_mb": 56.87,
      "network_tx_mb": 50.69,
      "block_read_mb": 86.54,
      "block_write_mb": 0.10
    }
  ]
}
```

> `network_rx_mb`, `network_tx_mb`, `block_read_mb`, `block_write_mb` are **cumulative totals since container start**, not per-window rates. The last sample value is stored.

### process_cpu_logs (one per collect_timeout tick)
```json
{
  "node": "0001-0001",
  "timestamp": "2026-04-08T12:00:05Z",
  "processes": [
    { "pid": 4821, "name": "java", "cpu_percent": 187.3, "memory_mb": 2048.5, "memory_percent": 8.5, "status": "Run" }
  ]
}
```
Top 10 processes above 1% CPU. No aggregation — one document per tick, not per minute.

### process_ram_logs (one per collect_timeout tick)
```json
{
  "node": "0001-0001",
  "timestamp": "2026-04-08T12:00:05Z",
  "processes": [
    { "pid": 4821, "name": "java", "memory_mb": 2048.5, "memory_percent": 8.5, "cpu_percent": 187.3, "status": "Run" }
  ]
}
```
Top 10 processes above 1% of total system RAM. Same shape as `process_cpu_logs`, sorted by memory instead.

### docker_event_logs (one per collect_docker_timeout tick)
```json
{
  "node": "0001-0001",
  "timestamp": "2026-04-08T12:00:05Z",
  "events": [
    { "event_time": "2026-04-08T12:00:03Z", "container_id": "531c5b818fe7", "container_name": "my-app", "action": "die", "exit_code": 137 }
  ]
}
```

### docker_container_logs (one per collect_docker_timeout tick)
```json
{
  "node": "0001-0001",
  "timestamp": "2026-04-08T12:00:05Z",
  "containers": [
    {
      "container_id": "531c5b818fe7",
      "container_name": "my-app",
      "log_lines": [ { "stream": "stderr", "time": "2026-04-08T12:00:04Z", "message": "OutOfMemoryError: Java heap space" } ],
      "truncated": false
    }
  ]
}
```

### system_event_logs (one per collect_timeout tick, Linux only)
```json
{
  "node": "0001-0001",
  "timestamp": "2026-04-08T12:00:05Z",
  "events": [
    { "event_time": "2026-04-08T12:00:03Z", "priority": "err", "unit": "docker.service", "message": "...", "hostname": "node-01" }
  ]
}
```
Parsed from `journalctl --output=json`. Empty `events` array on non-Linux platforms.

## Configuration

### Settings Document

```javascript
{
  "key": "0001-0001",
  "collect_timeout": 5,          // seconds between raw samples (LoadAverage, Memory, DiskSpace)
  "collect_docker_timeout": 20,  // seconds between raw Docker samples
  "store_timeout": 60            // aggregation window length — how often to write to MongoDB
}
```

### Live Reload

Settings are re-read from MongoDB after **every flush** (every `store_timeout` seconds). Update any value and it takes effect after the current window completes:

```javascript
// Example: slow down collection to save resources
db.MonitoringSettings.updateOne(
  { "key": "0001-0001" },
  { $set: { "collect_timeout": 10, "store_timeout": 120 } }
)
// No restart needed — takes effect after the next flush
```

## Querying Data

```javascript
// Latest load average document
db.load_average_metrics.find({ "node": "0001-0001" }).sort({ timestamp: -1 }).limit(1).pretty()

// Average load over the last hour
db.load_average_metrics.find({
  "node": "0001-0001",
  "timestamp": { $gte: new Date(Date.now() - 3600000) }
}).sort({ timestamp: -1 })

// Memory usage trends
db.memory_metrics.find({ "node": "0001-0001" }).sort({ timestamp: -1 }).limit(60)

// High-load windows (load_1min avg > 4 on an 8-core machine)
db.load_average_metrics.find({ "node": "0001-0001", "load_1min.avg": { $gt: 4 } })
```

## Adding New Metrics

1. Create a file in `src/metrics/` and implement `MetricCollector`
2. Add to `create_all_collectors()` in `src/metrics/mod.rs`
3. Add collection name to `collection_for()` in `src/scheduler.rs`
4. If the document has no top-level numeric fields to average (an events/log-style collector), add its name to `is_log_metric()` in `src/scheduler.rs` so it's written on every tick instead of being buffered and flushed once a minute
5. Rebuild and deploy

No MongoDB document changes needed. See [Adding New Metrics Guide](docs/adding-new-metrics.md).

## SystemD Service

```bash
# Install
sudo useradd -r -s /bin/false metrics-collector
sudo mkdir -p /opt/metrics-collector
sudo cp target/release/metrics-collector /opt/metrics-collector/
sudo chown -R metrics-collector:metrics-collector /opt/metrics-collector
sudo cp metrics-collector.service /etc/systemd/system/

# Configure (update ExecStart with your MongoDB URI and key)
sudo nano /etc/systemd/system/metrics-collector.service

# Start
sudo systemctl daemon-reload
sudo systemctl enable metrics-collector
sudo systemctl start metrics-collector
```

```bash
sudo systemctl status metrics-collector
sudo journalctl -u metrics-collector -f
sudo systemctl restart metrics-collector
```

## Development

```bash
cargo build           # Debug build
cargo build --release # Release build
cargo check           # Fast type check without producing binary
cargo test            # Run tests
cargo clippy          # Lint
cargo fmt             # Format
```

## Troubleshooting

**No data after startup:** The first document appears after one full `store_timeout` window (~65 seconds with defaults). Check logs for flush messages:
```bash
sudo journalctl -u metrics-collector | grep -E "flush|store|Reloading"
```

**Settings not loading:**
```javascript
// Verify the settings document has the new format (three flat fields)
db.MonitoringSettings.findOne({ "key": "your-key" })
```

**Docker stats failing:**
```bash
sudo usermod -aG docker metrics-collector
sudo systemctl restart metrics-collector
```

See [Deployment Guide](docs/deployment.md) for full troubleshooting steps.

## Security

- Runs as non-root user
- SystemD hardening options enabled
- MongoDB credentials masked in all log output
- Docker socket access: read-only stats queries only

## License

[Choose your license — MIT, Apache 2.0, etc.]

---

**Built with Rust** 🦀
