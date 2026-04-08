# Adding New Metrics - Step-by-Step Guide

This guide provides detailed instructions for extending the Metrics Collector with new metric types.

## Table of Contents

1. [Quick Overview](#quick-overview)
2. [How Aggregation Works](#how-aggregation-works)
3. [Step-by-Step Tutorial](#step-by-step-tutorial)
4. [Example: Network I/O Metric](#example-network-io-metric)
5. [Best Practices](#best-practices)
6. [Testing](#testing)
7. [Troubleshooting](#troubleshooting)

---

## Quick Overview

Adding a new metric involves **5 code changes** and **no MongoDB config changes**:

1. **Create** a new Rust file in `src/metrics/`
2. **Implement** the `MetricCollector` trait
3. **Register** the collector in `create_all_collectors()` (`src/metrics/mod.rs`)
4. **Register** the collection name in `collection_for()` (`src/scheduler.rs`)
5. **Build** and deploy the updated binary

The collection timing (how often to collect, how often to flush) comes from the shared `MonitoringSettings` document — no per-metric config is needed.

**Estimated time:** 30-60 minutes for a simple metric

---

## How Aggregation Works

Understanding this section will help you design your metric's document structure correctly.

### Flat numeric metrics (e.g. LoadAverage, Memory)

When your `collect()` returns a document with **top-level numeric fields** (Double, Int32, Int64), the `MetricBuffer` automatically aggregates them. Each field becomes:

```json
"my_field": { "avg": 1.42, "min": 0.80, "max": 2.30 }
```

**Exception — constant fields:** If a field has the same value in every sample (e.g. `cpu_cores`, `total_mb`), you should add it to `PASSTHROUGH_FIELDS` in `src/aggregator.rs` so it is stored as a plain value instead:

```json
"cpu_cores": 8
```

### Nested-array metrics (e.g. DiskSpace, DockerStats)

If your `collect()` document has **nested arrays** (like `disks: [...]` or `containers: [...]`) and no top-level numeric fields, the aggregator finds nothing to aggregate. It will instead return the **last raw sample** of the window with an updated timestamp.

For arrays where you want per-item aggregation (like Docker containers), you need a custom buffer — see `DockerMetricBuffer` in `src/aggregator.rs` as a reference.

### Collection timing

All metrics share the same `store_timeout` (flush interval) from the MongoDB settings document. Standard metrics use `collect_timeout` for their collection interval; Docker uses `collect_docker_timeout`. There is no per-metric timeout configuration.

---

## Step-by-Step Tutorial

### Step 1: Create the metric file

```bash
# Use snake_case naming
touch src/metrics/network_io.rs
```

### Step 2: Implement the basic structure

```rust
// src/metrics/network_io.rs

use async_trait::async_trait;
use bson::{doc, Document};
use chrono::Utc;
use std::error::Error;
use tracing::debug;

use super::MetricCollector;

pub struct NetworkCollector;

impl NetworkCollector {
    pub fn new() -> Self {
        NetworkCollector
    }
}

#[async_trait]
impl MetricCollector for NetworkCollector {
    fn name(&self) -> &str {
        "NetworkIO"  // PascalCase; used for logging
    }

    async fn collect(&self, node_id: &str) -> Result<Document, Box<dyn Error + Send + Sync>> {
        debug!("Collecting network I/O metrics");

        // TODO: gather data here

        let doc = doc! {
            "node":      node_id,
            "timestamp": Utc::now(),
            // add your fields here
        };

        Ok(doc)
    }
}

impl Default for NetworkCollector {
    fn default() -> Self {
        Self::new()
    }
}
```

### Step 3: Fill in the collection logic

```rust
async fn collect(&self, node_id: &str) -> Result<Document, Box<dyn Error + Send + Sync>> {
    debug!("Collecting network I/O metrics");

    let networks = sysinfo::Networks::new_with_refreshed_list();

    let mut total_rx: u64 = 0;
    let mut total_tx: u64 = 0;
    for (_, net) in networks.iter() {
        total_rx += net.total_received();
        total_tx += net.total_transmitted();
    }

    fn bytes_to_mb(b: u64) -> f64 { b as f64 / (1024.0 * 1024.0) }

    let doc = doc! {
        "node":            node_id,
        "timestamp":       Utc::now(),
        "total_rx_mb":     bytes_to_mb(total_rx),
        "total_tx_mb":     bytes_to_mb(total_tx),
    };

    debug!("Network: RX={:.1}MB TX={:.1}MB", bytes_to_mb(total_rx), bytes_to_mb(total_tx));
    Ok(doc)
}
```

The `MetricBuffer` will automatically produce `total_rx_mb: { avg, min, max }` and `total_tx_mb: { avg, min, max }` in the stored document.

### Step 4: Add dependencies (if needed)

```toml
# Cargo.toml
[dependencies]
# sysinfo is already present — use it for system-level metrics
# For HTTP APIs:
reqwest = { version = "0.12", features = ["json", "rustls-tls"], default-features = false }
```

### Step 5: Register in `create_all_collectors()`

Edit `src/metrics/mod.rs`:

```rust
pub mod load_average;
pub mod memory;
pub mod disk;
pub mod docker;
pub mod network_io;  // ← add

pub fn create_all_collectors() -> Vec<Box<dyn MetricCollector>> {
    vec![
        Box::new(load_average::LoadAverageCollector::new()),
        Box::new(memory::MemoryCollector::new()),
        Box::new(disk::DiskCollector::new()),
        Box::new(docker::DockerCollector::new()),
        Box::new(network_io::NetworkCollector::new()),  // ← add
    ]
}
```

### Step 6: Register the collection name in `collection_for()`

Edit `src/scheduler.rs`:

```rust
fn collection_for(metric_name: &str) -> &'static str {
    match metric_name {
        "LoadAverage" => "load_average_metrics",
        "Memory"      => "memory_metrics",
        "DiskSpace"   => "disk_metrics",
        "DockerStats" => "docker_metrics",
        "NetworkIO"   => "network_io_metrics",  // ← add
        _             => "unknown_metrics",
    }
}
```

That's it. No MongoDB document changes are needed.

### Step 7 (optional): Register constant fields

If any of your fields are constant across samples (e.g. interface names, hardware limits), add them to `PASSTHROUGH_FIELDS` in `src/aggregator.rs` so they are stored as plain values:

```rust
const PASSTHROUGH_FIELDS: &[&str] = &[
    "cpu_cores", "total_mb", "swap_total_mb",
    "my_constant_field",  // ← add yours here
];
```

Also update `bson_for_passthrough` if the field needs a specific BSON type (Int32/Int64 vs Double).

### Step 8: Build and verify

```bash
cargo build --release

# Test locally
cargo run -- --mongodb "mongodb://localhost:27017" --key "0001-0001"

# After ~65 seconds, check MongoDB
```

```javascript
db.network_io_metrics.find().sort({ timestamp: -1 }).limit(1).pretty()
// Expected:
// {
//   "node": "0001-0001",
//   "timestamp": ISODate("..."),
//   "sample_count": 12,
//   "total_rx_mb": { "avg": 0.5, "min": 0.1, "max": 1.2 },
//   "total_tx_mb": { "avg": 0.3, "min": 0.1, "max": 0.8 }
// }
```

---

## Example: Network I/O Metric

Complete implementation:

```rust
// src/metrics/network_io.rs

use async_trait::async_trait;
use bson::{doc, Document};
use chrono::Utc;
use std::error::Error;
use sysinfo::Networks;
use tracing::debug;

use super::MetricCollector;

/// Network I/O metric collector
///
/// Collects total bytes received and transmitted across all network interfaces.
/// Stored as avg/min/max per aggregation window.
///
/// Note: sysinfo returns cumulative totals since boot, so this metric
/// reflects the running total, not per-window deltas.
///
/// # Platform Support
/// - Linux: Full support via /proc/net/dev
/// - macOS: Full support via netstat
pub struct NetworkCollector;

impl NetworkCollector {
    pub fn new() -> Self { NetworkCollector }

    fn bytes_to_mb(bytes: u64) -> f64 {
        bytes as f64 / (1024.0 * 1024.0)
    }
}

#[async_trait]
impl MetricCollector for NetworkCollector {
    fn name(&self) -> &str { "NetworkIO" }

    /// Collects current network I/O metrics
    ///
    /// # Returns BSON Document Structure
    /// ```json
    /// {
    ///   "node": "0001-0001",
    ///   "timestamp": "...",
    ///   "total_rx_mb": 1024.5,
    ///   "total_tx_mb": 512.3,
    ///   "interfaces": [
    ///     { "name": "eth0", "rx_mb": 1000.0, "tx_mb": 500.0 }
    ///   ]
    /// }
    /// ```
    async fn collect(&self, node_id: &str) -> Result<Document, Box<dyn Error + Send + Sync>> {
        debug!("Collecting network I/O metrics");

        let networks = Networks::new_with_refreshed_list();
        let mut total_rx: u64 = 0;
        let mut total_tx: u64 = 0;
        let mut interface_array = Vec::new();

        for (name, net) in networks.iter() {
            let rx = net.total_received();
            let tx = net.total_transmitted();
            total_rx += rx;
            total_tx += tx;

            interface_array.push(doc! {
                "name":  name,
                "rx_mb": Self::bytes_to_mb(rx),
                "tx_mb": Self::bytes_to_mb(tx),
            });
        }

        let doc = doc! {
            "node":       node_id,
            "timestamp":  Utc::now(),
            "total_rx_mb": Self::bytes_to_mb(total_rx),
            "total_tx_mb": Self::bytes_to_mb(total_tx),
            "interfaces": interface_array,
        };

        debug!(
            "Network: total RX={:.1}MB TX={:.1}MB ({} interfaces)",
            Self::bytes_to_mb(total_rx),
            Self::bytes_to_mb(total_tx),
            networks.iter().count()
        );

        Ok(doc)
    }
}

impl Default for NetworkCollector {
    fn default() -> Self { Self::new() }
}
```

**What gets aggregated:**
- `total_rx_mb` → `{ avg, min, max }` (top-level numeric)
- `total_tx_mb` → `{ avg, min, max }` (top-level numeric)
- `interfaces` array → falls back to last-raw-sample (nested array, not aggregated)

---

## Best Practices

### Document Structure

1. **Always include `node` and `timestamp`:**
   ```rust
   let doc = doc! {
       "node":      node_id,
       "timestamp": Utc::now(),
       // your fields...
   };
   ```

2. **Use appropriate BSON types** (BSON doesn't have `u64`):
   ```rust
   "count": some_u64 as i64,    // not as u64
   "ratio": 3.14f64,             // f64, not f32
   ```

3. **Include units in field names:**
   ```rust
   "memory_used_mb": 1024,  // clear
   "memory_used": 1024,     // ambiguous
   ```

4. **Decide which fields are constant** and add them to `PASSTHROUGH_FIELDS`.

### Performance

- Prefer `collect_timeout` ≥ 5s for system-level metrics (they don't change faster than that)
- Docker API calls are slower (~50-200ms) — `collect_docker_timeout` = 20s is appropriate
- Avoid blocking operations; use `tokio::task::spawn_blocking` for CPU-heavy work

### Error Handling

```rust
// Provide helpful context
let data = read_sensor()
    .map_err(|e| format!("Failed to read temperature sensor: {}", e))?;

// Log at appropriate levels
debug!("Collecting...");         // Normal operation
info!("Metric started");         // Startup events
warn!("Docker unavailable");     // Degraded functionality
error!("Failed to connect");     // Serious problems
```

---

## Testing

### Unit Tests

Add tests to your metric file:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_network_collector() {
        let collector = NetworkCollector::new();
        assert_eq!(collector.name(), "NetworkIO");

        match collector.collect("test-node").await {
            Ok(doc) => {
                assert!(doc.contains_key("node"));
                assert!(doc.contains_key("timestamp"));
                assert!(doc.contains_key("total_rx_mb"));
                assert!(doc.contains_key("total_tx_mb"));
            }
            Err(e) => println!("Collection failed (expected in some envs): {}", e),
        }
    }
}
```

```bash
cargo test
```

### Integration Testing

```bash
# Start a local MongoDB
docker run -d -p 27017:27017 --name test-mongo mongo:latest

# Insert settings document
mongosh --eval '
use monitoring
db.MonitoringSettings.insertOne({
  "key": "test-node",
  "collect_timeout": 5,
  "collect_docker_timeout": 20,
  "store_timeout": 60
})
'

# Run the application
cargo run -- --mongodb "mongodb://localhost:27017" --key "test-node"

# After ~65 seconds, check MongoDB
mongosh --eval '
use monitoring
db.network_io_metrics.find().sort({timestamp: -1}).limit(1).pretty()
'

# Cleanup
docker stop test-mongo && docker rm test-mongo
```

---

## Troubleshooting

### Metric Not Appearing in Logs

1. ✓ Added to `create_all_collectors()` in `metrics/mod.rs`?
2. ✓ Module declared with `pub mod network_io;` in `metrics/mod.rs`?
3. ✓ Binary rebuilt after changes?

### Data Not in MongoDB

1. ✓ Collection name added to `collection_for()` in `scheduler.rs`?
2. ✓ Waited at least `store_timeout` seconds (default 60s)?
3. Check logs: `journalctl -u metrics-collector | grep NetworkIO`

### Fields Showing as `{ avg, min, max }` When They Should Be Plain

Add the field name to `PASSTHROUGH_FIELDS` in `src/aggregator.rs`.

### Fields Not Being Aggregated (All Showing as Last-Sample)

The metric's document probably has nested arrays. Top-level numeric fields aggregate automatically; nested structures fall back to last-raw-sample. See `DockerMetricBuffer` for how to implement custom per-item aggregation.

### Wrong Data Type Errors

```rust
// u64 is not a valid BSON type
"value": some_u64 as i64,  // correct

// f32 may cause precision issues in BSON
"value": 3.14f64,           // use f64
```

---

## Summary Checklist

Before deploying a new metric:

- [ ] File created in `src/metrics/`
- [ ] `MetricCollector` trait implemented with correct `name()` return value
- [ ] Registered in `create_all_collectors()` (`metrics/mod.rs`)
- [ ] Module declared with `pub mod ...` in `metrics/mod.rs`
- [ ] Collection name added to `collection_for()` (`scheduler.rs`)
- [ ] Constant fields (if any) added to `PASSTHROUGH_FIELDS` (`aggregator.rs`)
- [ ] Dependencies added to `Cargo.toml` (if needed)
- [ ] Unit tests written and passing (`cargo test`)
- [ ] Binary built and tested locally
- [ ] Verified stored documents have expected shape in MongoDB
