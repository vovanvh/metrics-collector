# Architecture Documentation - Metrics Collector

This document provides a comprehensive overview of the Metrics Collector's architecture, design decisions, and implementation details.

## Table of Contents

1. [System Overview](#system-overview)
2. [Project Structure](#project-structure)
3. [Core Architecture](#core-architecture)
4. [Component Details](#component-details)
5. [Data Flow](#data-flow)
6. [Design Patterns](#design-patterns)
7. [Technology Stack](#technology-stack)
8. [Performance Considerations](#performance-considerations)
9. [Security](#security)
10. [Extensibility](#extensibility)

---

## System Overview

### Purpose

The Metrics Collector is a lightweight, extensible server monitoring tool that:
- Collects system metrics (CPU, memory, disk, Docker)
- Buffers raw samples in memory and writes one aggregated document per minute to MongoDB
- Supports multiple servers with individual configurations
- Runs as a systemd service for reliability

### Key Features

- **Async/Concurrent**: Uses Tokio for efficient concurrent metric collection
- **Aggregated Storage**: Buffers 60-second windows; stores avg/min/max instead of raw samples (metrics only)
- **Unaggregated Log/Event Storage**: Process, Docker event/log, and system event collectors write every tick as its own document — no numeric fields to average
- **Extensible**: Easy to add new metric types via trait system
- **Configurable**: MongoDB-based configuration with live reload after every flush
- **Reliable**: Automatic restart, graceful error handling

### High-Level Architecture

```
┌──────────────────────────────────────────────────────────────────┐
│                        Metrics Collector                         │
│                                                                  │
│  ┌───────────┐  ┌───────────┐  ┌───────────┐  ┌────────────┐  │
│  │   Load    │  │  Memory   │  │   Disk    │  │   Docker   │  │
│  │ Collector │  │ Collector │  │ Collector │  │ Collector  │  │
│  └─────┬─────┘  └─────┬─────┘  └─────┬─────┘  └─────┬──────┘  │
│        │collect        │              │               │collect   │
│        ▼ every 5s      ▼ every 5s     ▼ every 5s      ▼ every 20s│
│  ┌───────────┐  ┌───────────┐  ┌───────────┐  ┌────────────┐  │
│  │  Metric   │  │  Metric   │  │  Metric   │  │   Docker   │  │
│  │  Buffer   │  │  Buffer   │  │  Buffer   │  │   Buffer   │  │
│  └─────┬─────┘  └─────┬─────┘  └─────┬─────┘  └─────┬──────┘  │
│        │flush every 60s (store_timeout)               │         │
│        └──────────────────┬──────────────────────────┘          │
│                           ▼                                      │
│                   ┌──────────────┐                               │
│                   │   Storage    │                               │
│                   │   Manager   │                                │
│                   └──────┬───────┘                               │
│                          │ after store: reload settings          │
└──────────────────────────┼───────────────────────────────────────┘
                           │
                   ┌───────▼────────┐
                   │    MongoDB     │
                   │  ┌──────────┐  │
                   │  │Settings  │  │
                   │  └──────────┘  │
                   │  ┌──────────┐  │
                   │  │ Metrics  │  │
                   │  │Collections│ │
                   │  └──────────┘  │
                   └────────────────┘
```

> This diagram shows the 4 aggregated metric collectors. The 5 log/event collectors (`ProcessCPUSnapshot`, `ProcessRAMSnapshot`, `DockerEvents`, `DockerLogs`, `SystemEvents`) follow the same collect → store → reload-settings shape, but skip the buffer box entirely — each tick is written straight to MongoDB via `run_log_task` (see [Scheduler Module](#scheduler-module-schedulerrs)).

---

## Project Structure

```
metrics-collector/
├── Cargo.toml                    # Dependencies and build configuration
├── metrics-collector.service     # SystemD service file
│
├── src/
│   ├── main.rs                  # Application entry point
│   ├── config.rs                # MongoDB configuration management
│   ├── storage.rs               # MongoDB storage operations
│   ├── aggregator.rs            # In-memory buffering and aggregation
│   ├── scheduler.rs             # Tokio-based task scheduler
│   │
│   └── metrics/                 # Metric collectors module
│       ├── mod.rs              # MetricCollector trait definition
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
    ├── deployment.md           # Deployment guide
    ├── architecture.md         # This file
    ├── adding-new-metrics.md   # Guide for extending metrics
    ├── rust-intro-guide.md     # Rust beginner's guide
    └── rust-cheatsheet.md      # Quick reference
```

### File Responsibilities

| File | Purpose | Key Components |
|------|---------|----------------|
| `main.rs` | Application initialization, CLI parsing | `main()`, `init_logging()`, `parse_arguments()` |
| `config.rs` | MongoDB connection, settings load/reload | `ConfigManager`, `MonitoringSettings` |
| `storage.rs` | Metric persistence to MongoDB | `MetricStorage`, `store_metric_safe()` |
| `aggregator.rs` | In-memory buffering, avg/min/max computation | `MetricBuffer`, `DockerMetricBuffer` |
| `scheduler.rs` | Task scheduling with Tokio, collection routing | `MetricScheduler`, `run_standard_task()`, `run_docker_task()`, `run_log_task()`, `collection_for()`, `collect_timeout_for()` |
| `metrics/mod.rs` | Metric trait and collector factory | `MetricCollector` trait, `create_all_collectors()` |
| `metrics/*.rs` | Individual metric implementations | Collector structs implementing `MetricCollector` |

---

## Core Architecture

### 1. Trait-Based Extensibility

The system uses Rust's trait system to provide a pluggable architecture:

```rust
#[async_trait]
pub trait MetricCollector: Send + Sync {
    fn name(&self) -> &str;
    async fn collect(&self, node_id: &str) -> Result<Document, Box<dyn Error + Send + Sync>>;
}
```

**Benefits:**
- Type safety: Compiler ensures all metrics implement required methods
- Polymorphism: Scheduler handles all metrics uniformly
- Extensibility: Add new metrics without changing existing code

### 2. Dual-Timer Aggregation Loop

Each metric runs a `tokio::select!` loop with two independent timers:

```rust
loop {
    // collect_timer fires every collect_timeout seconds
    let mut collect_timer = interval(Duration::from_secs(settings.collect_timeout));
    // flush_sleep fires after store_timeout seconds
    let flush_sleep = tokio::time::sleep(Duration::from_secs(settings.store_timeout));
    tokio::pin!(flush_sleep);

    loop {
        select! {
            _ = collect_timer.tick() => {
                // Collect sample and push to buffer
                buffer.push(&doc);
            }
            _ = &mut flush_sleep => { break; }  // Time to flush
        }
    }

    // Write aggregated document to MongoDB
    // Then reload settings from MongoDB
}
```

**Benefits:**
- Decoupled: collection frequency and storage frequency are independent
- Dynamic: timeouts reload from MongoDB after each flush
- Efficient: ~60 raw samples are never written to the database

### 3. In-Memory Aggregation

`MetricBuffer` accumulates numeric fields across samples and produces one document per window:

- **Aggregated fields** (`load_1min`, `available_mb`, etc.) → `{ "avg": …, "min": …, "max": … }`
- **Passthrough fields** (`cpu_cores`, `total_mb`, `swap_total_mb`) → plain value (constant, no aggregation needed)
- **Nested-array metrics** (DiskSpace, DockerStats) → last raw sample returned on flush

`DockerMetricBuffer` matches containers by name across samples and aggregates `cpu_percent`, `memory_used_mb`, `memory_percent` per container. Cumulative counters (`network_rx_mb`, etc.) are taken as last-sample values.

### 4. MongoDB-Based Configuration with Live Reload

Configuration is stored in MongoDB:

```javascript
{
  "key": "0001-0001",
  "collect_timeout": 5,          // seconds between samples (most metrics)
  "collect_docker_timeout": 20,  // seconds between Docker samples
  "store_timeout": 60            // seconds between flushes to MongoDB
}
```

After every successful flush, each task re-reads this document from MongoDB. If any timeout value changes, the new value takes effect on the next window — no restart required.

---

## Component Details

### Main Module (`main.rs`)

**Responsibilities:**
- Parse command-line arguments
- Initialize logging subsystem
- Connect to MongoDB
- Load initial configuration
- Create and start scheduler

**Key Functions:**

```rust
#[tokio::main]
async fn main() -> Result<()> {
    init_logging();
    let args = parse_arguments()?;
    let config_manager = ConfigManager::new(&args.mongodb_uri, ...).await?;
    let settings = config_manager.load_settings(&args.config_key).await?;
    let storage = MetricStorage::new(config_manager.client(), ...);
    let scheduler = MetricScheduler::new(config_manager, storage, args.config_key);
    scheduler.start(collectors, settings).await;
}
```

---

### Configuration Module (`config.rs`)

**Responsibilities:**
- Establish MongoDB connection
- Fetch monitoring settings at startup (`load_settings`)
- Re-fetch settings after every flush (`reload_settings`)

**Key Types:**

```rust
pub struct MonitoringSettings {
    pub key: String,
    pub collect_timeout: u64,         // interval for LoadAverage, Memory, DiskSpace
    pub collect_docker_timeout: u64,  // interval for DockerStats
    pub store_timeout: u64,           // window length / flush interval
}

pub struct ConfigManager {
    client: Client,
    database_name: String,
}
```

**Key Methods:**

```rust
// Called once at startup
async fn load_settings(&self, key: &str) -> Result<MonitoringSettings>

// Called after every successful flush
async fn reload_settings(&self, key: &str) -> Result<MonitoringSettings>
```

**MongoDB Query:**
```javascript
db.MonitoringSettings.findOne({ "key": "0001-0001" })
```

---

### Aggregator Module (`aggregator.rs`)

**Responsibilities:**
- Buffer raw samples in memory during the collection window
- Produce an aggregated BSON document on flush

#### `MetricBuffer` (LoadAverage, Memory, DiskSpace)

```rust
pub struct MetricBuffer {
    samples: Vec<HashMap<String, f64>>,  // per-tick numeric snapshots
    last_raw: Option<Document>,           // fallback for nested-array metrics
}
```

`push(&Document)`:
- Extracts top-level numeric BSON fields, skips `node` and `timestamp`
- Stores a clone of the raw document in `last_raw` (for DiskSpace fallback)

`flush(&str) -> Option<Document>`:
- ≥2 samples with numeric fields → aggregated document with avg/min/max per field
- 0 numeric samples → returns `last_raw` with updated timestamp (DiskSpace path)
- Never collected → returns `None`

**Passthrough fields** (plain values, not aggregated):
```rust
const PASSTHROUGH_FIELDS: &[&str] = &["cpu_cores", "total_mb", "swap_total_mb"];
```
These preserve their original BSON types: `cpu_cores` → `Int32`, `total_mb` / `swap_total_mb` → `Int64`.

#### `DockerMetricBuffer`

```rust
pub struct DockerMetricBuffer {
    container_samples: HashMap<String, Vec<ContainerSample>>,  // keyed by container name
    last_raw: Option<Document>,
}
```

Aggregates per container across samples:
- `cpu_percent`, `memory_used_mb`, `memory_percent` → `{ avg, min, max }`
- `memory_limit_mb` → first sample value (constant per container)
- `network_rx_mb`, `network_tx_mb`, `block_read_mb`, `block_write_mb` → last sample value (cumulative counters)

---

### Storage Module (`storage.rs`)

**Responsibilities:**
- Insert aggregated metric documents into MongoDB collections
- Handle storage errors gracefully with one retry

Collection names are hardcoded in `scheduler.rs` via `collection_for()`:

| Metric | Collection | Written by |
|--------|-----------|-----------|
| LoadAverage | `load_average_metrics` | `run_standard_task` (aggregated) |
| Memory | `memory_metrics` | `run_standard_task` (aggregated) |
| DiskSpace | `disk_metrics` | `run_standard_task` (last-sample fallback) |
| DockerStats | `docker_metrics` | `run_docker_task` (aggregated) |
| ProcessCPUSnapshot | `process_cpu_logs` | `run_log_task` (every tick) |
| ProcessRAMSnapshot | `process_ram_logs` | `run_log_task` (every tick) |
| DockerEvents | `docker_event_logs` | `run_log_task` (every tick) |
| DockerLogs | `docker_container_logs` | `run_log_task` (every tick) |
| SystemEvents | `system_event_logs` | `run_log_task` (every tick) |

Anything not in this list falls through to `unknown_metrics` — this should never happen for a registered collector; if it does, `collection_for()` is missing an arm for it.

---

### Scheduler Module (`scheduler.rs`)

**Responsibilities:**
- Spawn independent tasks for each metric
- Run dual-timer collect/flush loops
- Reload settings from MongoDB after each flush

```rust
pub struct MetricScheduler {
    config_manager: Arc<ConfigManager>,
    storage: Arc<MetricStorage>,
    node_id: String,
}
```

Three task variants, dispatched in `start()` based on the collector's name:
- `run_standard_task` — LoadAverage, Memory, DiskSpace: uses `MetricBuffer`, buffers and flushes every `store_timeout`
- `run_docker_task` — DockerStats: uses `DockerMetricBuffer`, buffers and flushes every `store_timeout`
- `run_log_task` — ProcessCPUSnapshot, ProcessRAMSnapshot, DockerEvents, DockerLogs, SystemEvents (selected via `is_log_metric()`): **no buffering** — each collected document is written to MongoDB immediately

`run_standard_task` and `run_docker_task` follow the same outer loop:
1. Create `collect_timer` and `flush_sleep` from current settings
2. Inner `select!` loop: collect until flush deadline
3. Flush buffer → store to MongoDB → reload settings
4. Repeat with updated settings

`run_log_task` follows a similar outer loop, but there's no buffer to flush — instead of accumulating samples, `storage.store_metric_safe()` is called directly inside the `select!` arm on every `collect_timer` tick. Settings are still reloaded on the same `store_timeout` cadence via a parallel `reload_sleep` timer, so `collect_timeout` changes take effect without a restart, same as the other task types.

**Which timeout applies to which collector** is resolved by `collect_timeout_for()`, the single source of truth used by both the startup log line and `run_log_task`:
```rust
fn collect_timeout_for(metric_name: &str, settings: &MonitoringSettings) -> u64 {
    match metric_name {
        "DockerStats" | "DockerEvents" | "DockerLogs" => settings.collect_docker_timeout,
        _ => settings.collect_timeout,
    }
}
```
Anything that talks to the Docker daemon (`DockerStats`, `DockerEvents`, `DockerLogs`) shares `collect_docker_timeout` so they don't hit it at inconsistent rates; everything else uses `collect_timeout`.

---

### Individual Metric Collectors

#### Load Average (`load_average.rs`)

**Data Source:** `/proc/loadavg` (Linux), `sysctl` (macOS)

**Raw document (per collect_timeout):**
```json
{
  "node": "0001-0001", "timestamp": "...",
  "load_1min": 1.5, "load_5min": 1.2, "load_15min": 0.9, "cpu_cores": 8
}
```

**Stored document (per store_timeout):**
```json
{
  "node": "0001-0001", "timestamp": "...", "sample_count": 12,
  "cpu_cores": 8,
  "load_1min":  { "avg": 1.42, "min": 0.80, "max": 2.30 },
  "load_5min":  { "avg": 1.18, "min": 0.90, "max": 1.50 },
  "load_15min": { "avg": 0.95, "min": 0.85, "max": 1.10 }
}
```

#### Memory (`memory.rs`)

**Data Source:** `/proc/meminfo` (Linux), `vm_stat` (macOS)

**Raw document fields collected:** `total_mb`, `swap_total_mb`, `available_mb`, `used_percent`, `swap_used_percent`

*(Fields removed: `used_mb`, `free_mb`, `swap_used_mb`, `swap_free_mb` — derivable from retained fields)*

**Stored document (per store_timeout):**
```json
{
  "node": "0001-0001", "timestamp": "...", "sample_count": 12,
  "total_mb": 24048, "swap_total_mb": 0,
  "available_mb":      { "avg": 19200.0, "min": 18000.0, "max": 21000.0 },
  "used_percent":      { "avg": 20.2,    "min": 12.8,    "max": 25.1    },
  "swap_used_percent": { "avg": 0.0,     "min": 0.0,     "max": 0.0     }
}
```

#### Disk Space (`disk.rs`)

**Data Source:** `statvfs()` system call

Disk documents contain a nested `disks` array. The aggregator finds no top-level numeric fields and falls back to storing the last raw sample of the window with an updated timestamp.

**Stored document (per store_timeout, last-sample passthrough):**
```json
{
  "node": "0001-0001", "timestamp": "...",
  "disks": [
    { "mount_point": "/", "filesystem": "ext4",
      "total_gb": 500.0, "used_gb": 250.0, "available_gb": 250.0, "used_percent": 50.0 }
  ]
}
```

#### Docker Stats (`docker.rs`)

**Data Source:** Docker Engine API
**Collect interval:** `collect_docker_timeout` (default 20s → 3 samples per window)

**Stored document (per store_timeout):**
```json
{
  "node": "0001-0001", "timestamp": "...", "sample_count": 3,
  "containers": [
    {
      "id": "531c5b818fe7", "name": "krys-kafka-ui-prod",
      "memory_limit_mb": 24048.26,
      "cpu_percent":    { "avg": 0.38, "min": 0.21, "max": 0.54 },
      "memory_used_mb": { "avg": 710.1, "min": 705.2, "max": 714.8 },
      "memory_percent": { "avg": 2.95,  "min": 2.93,  "max": 2.97  },
      "network_rx_mb": 56.87,
      "network_tx_mb": 50.69,
      "block_read_mb":  86.54,
      "block_write_mb":  0.10
    }
  ]
}
```

> `network_rx_mb`, `network_tx_mb`, `block_read_mb`, `block_write_mb` are **cumulative totals since container start** — they only ever increase. The last sample in the window is stored.

---

### Log/Event Collectors

Unlike the four metrics above, these five collectors produce documents with no top-level numeric fields — just `node`, `timestamp`, and a nested array. Run via `run_log_task` (see [Scheduler Module](#scheduler-module-schedulerrs)): every tick is written as its own document, not aggregated.

#### Process CPU / RAM Snapshots (`processes_cpu.rs`, `processes_ram.rs`)

**Data Source:** `sysinfo` process list. Filtered to drop noise: `ProcessCPUSnapshot` keeps processes above 1% CPU, `ProcessRAMSnapshot` keeps processes above 1% of total system RAM. Top 10 each.

```json
{
  "node": "0001-0001", "timestamp": "...",
  "processes": [
    { "pid": 4821, "name": "java", "cpu_percent": 187.3, "memory_mb": 2048.5, "memory_percent": 8.5, "status": "Run" }
  ]
}
```

#### Docker Events (`docker_events.rs`)

**Data Source:** Docker Engine events API. Container lifecycle transitions (start, stop, die, OOM-kill, restart).

```json
{
  "node": "0001-0001", "timestamp": "...",
  "events": [
    { "event_time": "...", "container_id": "531c5b818fe7", "container_name": "my-app", "action": "die", "exit_code": 137 }
  ]
}
```

#### Docker Container Logs (`docker_logs.rs`)

**Data Source:** Docker Engine logs API. stdout/stderr lines batched per container per tick.

```json
{
  "node": "0001-0001", "timestamp": "...",
  "containers": [
    { "container_id": "531c5b818fe7", "container_name": "my-app",
      "log_lines": [ { "stream": "stderr", "time": "...", "message": "OutOfMemoryError: Java heap space" } ],
      "truncated": false }
  ]
}
```

#### System Events (`system_events.rs`)

**Data Source:** `journalctl --output=json -p err` (Linux/systemd only). Kernel and systemd error-level events since the last poll. Returns an empty `events` array on non-Linux platforms or if `journalctl` is unavailable, rather than failing.

```json
{
  "node": "0001-0001", "timestamp": "...",
  "events": [
    { "event_time": "...", "priority": "err", "unit": "docker.service", "message": "...", "hostname": "node-01" }
  ]
}
```

---

## Data Flow

### Application Startup Flow

```
1. main()
   │
   ├─> init_logging()
   ├─> parse_arguments()
   ├─> ConfigManager::new()              [Connect to MongoDB]
   ├─> config_manager.load_settings()   [Read MonitoringSettings document]
   ├─> MetricStorage::new()
   ├─> create_all_collectors()
   ├─> MetricScheduler::new(config_manager, storage, node_id)
   └─> scheduler.start(collectors, settings)
       └─> (runs forever)
```

### Metric Collection Flow (Per Metric Task)

**`run_standard_task` / `run_docker_task`** (LoadAverage, Memory, DiskSpace, DockerStats):

```
Outer loop (runs forever, settings may change each iteration):
   │
   ├─> Create collect_timer(collect_timeout) and flush_sleep(store_timeout)
   │
   │   Inner select! loop:
   │   ├─> collect_timer.tick() → collector.collect() → buffer.push()
   │   ├─> collect_timer.tick() → collector.collect() → buffer.push()
   │   ├─> ... (repeats until flush_sleep fires)
   │   └─> flush_sleep fires → break inner loop
   │
   ├─> buffer.flush() → aggregated BSON document
   ├─> storage.store_metric_safe(collection, document)
   ├─> config_manager.reload_settings()  [re-read MongoDB after each store]
   └─> update settings locals, loop back
```

**`run_log_task`** (ProcessCPUSnapshot, ProcessRAMSnapshot, DockerEvents, DockerLogs, SystemEvents) — no buffer, writes every tick:

```
Outer loop (runs forever, settings may change each iteration):
   │
   ├─> Create collect_timer(collect_timeout_for(...)) and reload_sleep(store_timeout)
   │
   │   Inner select! loop:
   │   ├─> collect_timer.tick() → collector.collect() → storage.store_metric_safe() [immediately]
   │   ├─> collect_timer.tick() → collector.collect() → storage.store_metric_safe() [immediately]
   │   ├─> ... (repeats until reload_sleep fires)
   │   └─> reload_sleep fires → break inner loop
   │
   ├─> config_manager.reload_settings()  [re-read MongoDB on the same cadence as a flush]
   └─> update settings locals, loop back
```

### Error Handling Flow

```
Error Occurs
   │
   ├─> Collection Error
   │   ├─> Log error with context
   │   ├─> For Docker: Log hint about Docker daemon
   │   └─> Continue — task keeps running, sample is skipped
   │
   ├─> Storage Error
   │   ├─> Log error
   │   ├─> Retry once (with 100ms delay)
   │   └─> If still fails: log and continue
   │
   ├─> Settings Reload Error
   │   ├─> Log warning
   │   └─> Keep using current settings — no crash
   │
   └─> Fatal Error (startup)
       ├─> Log error with full context
       └─> Exit application
```

---

## Design Patterns

### 1. Dual-Timer Pattern with `tokio::select!` and `tokio::pin!`

```rust
let flush_sleep = tokio::time::sleep(Duration::from_secs(store_timeout));
tokio::pin!(flush_sleep);  // Required: Sleep is !Unpin

loop {
    select! {
        _ = collect_timer.tick() => { /* collect */ }
        _ = &mut flush_sleep    => { break; }
    }
}
```

`tokio::pin!` is required because `Sleep` is `!Unpin` and cannot be polled by value in a loop.

### 2. Trait Object Pattern

```rust
// Box<dyn MetricCollector> enables heterogeneous collections
let collectors: Vec<Box<dyn MetricCollector>> = create_all_collectors();
// Scheduler handles all types uniformly via the trait
```

### 3. Shared State Pattern (Arc)

```rust
let config_manager = Arc::new(config_manager);
let storage = Arc::new(storage);

for collector in collectors {
    let config_mgr = Arc::clone(&config_manager);
    let storage    = Arc::clone(&storage);
    tokio::spawn(async move { run_standard_task(..., config_mgr, storage, ...).await });
}
```

### 4. Buffer + Flush Pattern

Raw samples accumulate in memory; only the aggregated summary is written to MongoDB. This reduces database write volume from `(60s / collect_timeout)` inserts per minute down to 1.

---

## Technology Stack

| Crate | Version | Purpose |
|-------|---------|---------|
| `tokio` | 1.35 | Async runtime, task scheduling, timers |
| `mongodb` | 2.8 | MongoDB driver |
| `sysinfo` | 0.30 | System information (CPU, memory, disk) |
| `bollard` | 0.16 | Docker API client |
| `serde` | 1.0 | Serialization/deserialization |
| `bson` | 2.9 | BSON format (MongoDB documents) |
| `chrono` | 0.4 | Date/time handling |
| `tracing` | 0.1 | Structured logging |
| `anyhow` | 1.0 | Error handling with context |
| `async-trait` | 0.1 | Async methods in traits |

---

## Performance Considerations

### Memory Usage

**Typical memory footprint:** 10-20 MB

- Each metric buffer holds at most `store_timeout / collect_timeout` samples (default: 12 for standard metrics, 3 for Docker)
- Samples are small `HashMap<String, f64>` entries, not full BSON documents
- Buffer is cleared on each flush

### Write Volume

**Aggregated metrics** (LoadAverage, Memory, DiskSpace, DockerStats): before aggregation, 1 insert per metric per `collect_timeout` seconds = ~720 inserts/hour for load average alone. After aggregation: 1 insert per metric per `store_timeout` seconds = 60 inserts/hour for all four metrics combined.

**Log/event collectors** (ProcessCPUSnapshot, ProcessRAMSnapshot, DockerEvents, DockerLogs, SystemEvents): **not aggregated** — each writes 1 document per `collect_timeout_for()` tick. With the defaults (5s for host-level, 20s for Docker-facing), that's ~720 inserts/hour each for the three host/system collectors and ~180 inserts/hour each for the two Docker-facing ones — roughly 2,700 inserts/hour combined. This is why these collections need a short TTL (tracked in MC-6) rather than the 30-day retention used for metrics.

### CPU Usage

**Typical CPU usage:** < 1% on modern hardware

- Async/await prevents blocking
- `select!` uses efficient OS-level waiting
- No busy-waiting

---

## Security

### Process Security

- Runs as dedicated non-root user
- SystemD hardening options enabled
- Resource limits prevent runaway memory usage

### MongoDB Security

- Supports authenticated connections
- Credentials are masked in all log output
- Settings are reloaded from MongoDB after each flush — malformed documents are logged and current settings are retained (no crash)

### Docker Socket Access

- Requires membership in `docker` group
- Read-only queries only (stats)
- If Docker is unavailable, the DockerStats task logs a warning and continues

---

## Extensibility

### Adding New Metrics

See `docs/adding-new-metrics.md` for a complete guide.

**Summary:**
1. Create new file in `src/metrics/`
2. Implement `MetricCollector` trait
3. Add to `create_all_collectors()` in `src/metrics/mod.rs`
4. Add collection name mapping in `collection_for()` in `src/scheduler.rs`
5. If the metric has constant fields, add them to `PASSTHROUGH_FIELDS` in `src/aggregator.rs`
6. If the document has no top-level numeric fields (an events/log-style collector), add its name to `is_log_metric()` in `src/scheduler.rs` so `run_log_task` writes every tick instead of `run_standard_task` silently keeping only the last tick before each flush
7. Build and deploy

No MongoDB configuration changes needed — collection name and timing are resolved from the three shared timeout settings.

For deployment instructions, see `docs/deployment.md`.
