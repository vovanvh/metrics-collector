# Adding New Metrics - Step-by-Step Guide

This guide provides detailed instructions for extending the Metrics Collector with new metric types. The extensible architecture makes adding new metrics straightforward.

## Table of Contents

1. [Quick Overview](#quick-overview)
2. [Prerequisites](#prerequisites)
3. [Step-by-Step Tutorial](#step-by-step-tutorial)
4. [Example: Network I/O Metric](#example-network-io-metric)
5. [Example: Temperature Monitoring](#example-temperature-monitoring)
6. [Best Practices](#best-practices)
7. [Testing](#testing)
8. [Deployment](#deployment)
9. [Troubleshooting](#troubleshooting)

---

## Quick Overview

Adding a new metric involves:

1. **Create** a new Rust file in `src/metrics/`
2. **Implement** the `MetricCollector` trait
3. **Register** the collector in `create_all_collectors()`
4. **Configure** the metric in MongoDB
5. **Build** and deploy the updated binary

**Estimated time:** 30-60 minutes for a simple metric

---

## Prerequisites

### Knowledge Required

- Basic Rust programming
- Understanding of async/await
- Familiarity with the system metric you want to collect
- Basic MongoDB knowledge

### Tools Needed

- Rust toolchain (1.70+)
- Access to MongoDB
- Test environment for validation
- Code editor with Rust support

### Reading the Existing Code

Before starting, review these files to understand the pattern:

```bash
# Study the trait definition
cat src/metrics/mod.rs

# Study a simple example (load average)
cat src/metrics/load_average.rs

# Study a complex example (Docker)
cat src/metrics/docker.rs
```

---

## Step-by-Step Tutorial

### Step 1: Create Metric File

Create a new file in `src/metrics/` directory:

```bash
# Choose a descriptive name (snake_case)
# Example: network_io.rs, temperature.rs, process_monitor.rs
touch src/metrics/network_io.rs
```

### Step 2: Implement Basic Structure

Open the new file and add the basic structure:

```rust
// Network I/O metric collector
//
// Collects network interface statistics including bytes sent/received,
// packets sent/received, and error counts.

use async_trait::async_trait;
use bson::{doc, Document};
use chrono::Utc;
use std::error::Error;
use tracing::debug;

use super::MetricCollector;

/// Network I/O metric collector
///
/// [Add detailed description of what this metric collects,
///  why it's useful, and any platform-specific notes]
pub struct NetworkCollector {
    // Add any state needed for collection
    // Most collectors don't need state, but you might need:
    // - API clients
    // - Previous values for delta calculations
    // - Configuration options
}

impl NetworkCollector {
    /// Creates a new NetworkCollector instance
    pub fn new() -> Self {
        NetworkCollector {
            // Initialize any fields
        }
    }
}

#[async_trait]
impl MetricCollector for NetworkCollector {
    /// Returns the metric name
    /// This name must match the configuration in MongoDB
    fn name(&self) -> &str {
        "NetworkIO"  // Use PascalCase, must be unique
    }

    /// Collects current network I/O metrics
    ///
    /// # Returns BSON Document Structure
    /// Document the structure in a code comment
    ///
    /// # Errors
    /// Document what errors might occur
    async fn collect(&self, node_id: &str) -> Result<Document, Box<dyn Error + Send + Sync>> {
        debug!("Collecting network I/O metrics");

        // TODO: Implement collection logic

        // Create BSON document with metric data
        let doc = doc! {
            // Always include these standard fields:
            "node": node_id,
            "timestamp": Utc::now(),

            // Add your metric-specific fields:
            // "field_name": value,
        };

        debug!("Network I/O: [log key metrics for debugging]");

        Ok(doc)
    }
}

impl Default for NetworkCollector {
    fn default() -> Self {
        Self::new()
    }
}
```

### Step 3: Implement Collection Logic

Fill in the `collect()` method with actual metric collection:

```rust
async fn collect(&self, node_id: &str) -> Result<Document, Box<dyn Error + Send + Sync>> {
    debug!("Collecting network I/O metrics");

    // 1. Gather raw data
    //    Use appropriate crate or system call
    //    Examples:
    //    - sysinfo crate for system metrics
    //    - std::fs for reading /proc files
    //    - External API clients
    let raw_data = gather_network_stats()?;

    // 2. Process and calculate derived values
    let bytes_sent = raw_data.bytes_sent;
    let bytes_received = raw_data.bytes_received;
    let total_traffic_mb = (bytes_sent + bytes_received) / (1024 * 1024);

    // 3. Create BSON document
    let doc = doc! {
        // Standard fields (required)
        "node": node_id,
        "timestamp": Utc::now(),

        // Metric-specific fields
        "bytes_sent": bytes_sent as i64,
        "bytes_received": bytes_received as i64,
        "total_traffic_mb": total_traffic_mb as i64,

        // More fields as needed...
    };

    // 4. Log summary for debugging
    debug!(
        "Network: sent={} MB, received={} MB",
        bytes_sent / (1024 * 1024),
        bytes_received / (1024 * 1024)
    );

    Ok(doc)
}
```

### Step 4: Add Dependencies (if needed)

If your metric needs external crates, add them to `Cargo.toml`:

```toml
[dependencies]
# Example: for system network stats
sysinfo = "0.30"

# Example: for reading /proc files
procfs = "0.16"

# Example: for HTTP API calls
reqwest = { version = "0.11", features = ["json"] }
```

### Step 5: Register the Collector

Edit `src/metrics/mod.rs` to register your new metric:

```rust
// 1. Add module declaration at the top
pub mod load_average;
pub mod memory;
pub mod disk;
pub mod docker;
pub mod network_io;  // <-- Add this line

// 2. Add to create_all_collectors() function
pub fn create_all_collectors() -> Vec<Box<dyn MetricCollector>> {
    vec![
        Box::new(load_average::LoadAverageCollector::new()),
        Box::new(memory::MemoryCollector::new()),
        Box::new(disk::DiskCollector::new()),
        Box::new(docker::DockerCollector::new()),
        Box::new(network_io::NetworkCollector::new()),  // <-- Add this line
    ]
}
```

### Step 6: Add MongoDB Configuration

Connect to MongoDB and add configuration for your new metric:

```javascript
// Connect to MongoDB
mongosh "mongodb://your-mongodb-host:27017"

// Switch to monitoring database
use monitoring

// Update the settings document to include your new metric
db.MonitoringSettings.updateOne(
  { "key": "1111-1111" },
  {
    $set: {
      "metric_settings.NetworkIO": {
        "timeout": 10,  // Collect every 10 seconds
        "collection": "network_io_metrics"  // Collection name
      }
    }
  }
)

// Verify the update
db.MonitoringSettings.findOne({ "key": "1111-1111" })
```

**Configuration Guidelines:**

| Setting | Description | Recommendations |
|---------|-------------|-----------------|
| `timeout` | Collection interval in seconds | 5-30s for system metrics, 60-300s for slow/expensive metrics |
| `collection` | MongoDB collection name | Use lowercase with underscores, suffix with `_metrics` |

### Step 7: Build and Test

```bash
# Build the project
cargo build --release

# Run tests
cargo test

# Test locally (will try to connect to MongoDB)
cargo run -- --mongodb "mongodb://localhost:27017" --key "1111-1111"

# Watch the logs
# You should see messages like:
# "Collecting network I/O metrics"
# "Scheduling metric 'NetworkIO' with interval of 10s"
```

### Step 8: Verify Data in MongoDB

```javascript
// Connect to MongoDB
mongosh "mongodb://your-mongodb-host:27017"

use monitoring

// Check that data is being stored
db.network_io_metrics.find({ "node": "1111-1111" }).sort({ timestamp: -1 }).limit(5)

// Should show recent documents like:
// {
//   "node": "1111-1111",
//   "timestamp": ISODate("2024-01-15T10:30:00Z"),
//   "bytes_sent": 1234567,
//   ...
// }

// Count documents (should increase over time)
db.network_io_metrics.countDocuments({ "node": "1111-1111" })
```

---

## Example: Network I/O Metric

Here's a complete example of a network I/O metric collector:

```rust
// src/metrics/network_io.rs

// Network I/O metric collector
//
// Collects network interface statistics for all active interfaces.
// Tracks bytes sent/received, packets, and errors.

use async_trait::async_trait;
use bson::{doc, Document};
use chrono::Utc;
use std::error::Error;
use sysinfo::{Networks, System};
use tracing::debug;

use super::MetricCollector;

/// Network I/O metric collector
///
/// Collects statistics for all network interfaces including:
/// - Bytes transmitted and received
/// - Packets transmitted and received
/// - Error counts
///
/// # Platform Support
/// - Linux: Full support via /proc/net/dev
/// - macOS: Full support via netstat
/// - Windows: Full support via GetIfTable
pub struct NetworkCollector;

impl NetworkCollector {
    /// Creates a new NetworkCollector instance
    pub fn new() -> Self {
        NetworkCollector
    }

    /// Converts bytes to megabytes
    fn bytes_to_mb(bytes: u64) -> f64 {
        bytes as f64 / (1024.0 * 1024.0)
    }
}

#[async_trait]
impl MetricCollector for NetworkCollector {
    /// Returns the metric name
    fn name(&self) -> &str {
        "NetworkIO"
    }

    /// Collects current network I/O metrics
    ///
    /// # Returns BSON Document Structure
    /// ```json
    /// {
    ///   "node": "1111-1111",
    ///   "timestamp": "2024-01-15T10:30:00Z",
    ///   "interfaces": [
    ///     {
    ///       "name": "eth0",
    ///       "received_mb": 1024.5,
    ///       "transmitted_mb": 512.3,
    ///       "packets_received": 1000000,
    ///       "packets_transmitted": 500000,
    ///       "errors_received": 0,
    ///       "errors_transmitted": 0
    ///     }
    ///   ]
    /// }
    /// ```
    async fn collect(&self, node_id: &str) -> Result<Document, Box<dyn Error + Send + Sync>> {
        debug!("Collecting network I/O metrics");

        // Get network information
        // Creates a new instance to get fresh data
        let networks = Networks::new_with_refreshed_list();

        // Build array of interface statistics
        let mut interface_array = Vec::new();
        let mut total_rx = 0u64;
        let mut total_tx = 0u64;

        for (interface_name, network) in networks.iter() {
            // Get statistics for this interface
            let received = network.total_received();
            let transmitted = network.total_transmitted();
            let packets_rx = network.total_packets_received();
            let packets_tx = network.total_packets_transmitted();
            let errors_rx = network.total_errors_on_received();
            let errors_tx = network.total_errors_on_transmitted();

            // Track totals
            total_rx += received;
            total_tx += transmitted;

            // Create interface document
            let interface_doc = doc! {
                // Interface name (e.g., "eth0", "wlan0")
                "name": interface_name,

                // Bytes received (total since boot)
                "received_mb": Self::bytes_to_mb(received),

                // Bytes transmitted (total since boot)
                "transmitted_mb": Self::bytes_to_mb(transmitted),

                // Packet counts
                "packets_received": packets_rx as i64,
                "packets_transmitted": packets_tx as i64,

                // Error counts (should be 0 or very low)
                "errors_received": errors_rx as i64,
                "errors_transmitted": errors_tx as i64,
            };

            debug!(
                "Interface {}: RX={:.1}MB, TX={:.1}MB",
                interface_name,
                Self::bytes_to_mb(received),
                Self::bytes_to_mb(transmitted)
            );

            interface_array.push(interface_doc);
        }

        // Create main document
        let doc = doc! {
            // Node identifier
            "node": node_id,

            // Timestamp
            "timestamp": Utc::now(),

            // Total traffic across all interfaces
            "total_received_mb": Self::bytes_to_mb(total_rx),
            "total_transmitted_mb": Self::bytes_to_mb(total_tx),

            // Per-interface details
            "interfaces": interface_array,
        };

        debug!(
            "Network: Total RX={:.1}MB, TX={:.1}MB across {} interface(s)",
            Self::bytes_to_mb(total_rx),
            Self::bytes_to_mb(total_tx),
            networks.iter().count()
        );

        Ok(doc)
    }
}

impl Default for NetworkCollector {
    fn default() -> Self {
        Self::new()
    }
}
```

---

## Example: Temperature Monitoring

Example of a more specialized metric (CPU temperature):

```rust
// src/metrics/temperature.rs

// Temperature metric collector
//
// Monitors CPU and other hardware temperature sensors.
// Useful for detecting overheating and thermal throttling.

use async_trait::async_trait;
use bson::{doc, Document};
use chrono::Utc;
use std::error::Error;
use sysinfo::{Components, System};
use tracing::{debug, warn};

use super::MetricCollector;

/// Temperature metric collector
///
/// Collects temperature readings from all available sensors.
///
/// # Platform Support
/// - Linux: Requires lm-sensors package and appropriate drivers
/// - macOS: Limited support (CPU temp only on some models)
/// - Windows: Limited support (varies by hardware)
///
/// # Note
/// This metric may not work on all systems. If no sensors are
/// available, it will return an empty sensors array.
pub struct TemperatureCollector;

impl TemperatureCollector {
    pub fn new() -> Self {
        TemperatureCollector
    }
}

#[async_trait]
impl MetricCollector for TemperatureCollector {
    fn name(&self) -> &str {
        "Temperature"
    }

    async fn collect(&self, node_id: &str) -> Result<Document, Box<dyn Error + Send + Sync>> {
        debug!("Collecting temperature metrics");

        // Get component information (includes temperature sensors)
        let components = Components::new_with_refreshed_list();

        // Build array of sensor readings
        let mut sensor_array = Vec::new();
        let mut max_temp = 0.0f32;

        for component in components.iter() {
            // Get temperature reading
            let temp = component.temperature();
            let critical = component.critical();

            // Track maximum temperature
            if temp > max_temp {
                max_temp = temp;
            }

            // Create sensor document
            let sensor_doc = doc! {
                // Sensor name/label
                "label": component.label(),

                // Current temperature in Celsius
                "temperature_c": temp as f64,

                // Critical threshold (if available)
                "critical_c": critical.map(|c| c as f64),

                // Percentage of critical threshold
                "percent_of_critical": critical.map(|c| (temp / c * 100.0) as f64),
            };

            debug!(
                "Sensor {}: {:.1}°C{}",
                component.label(),
                temp,
                critical.map(|c| format!(" (critical: {:.1}°C)", c)).unwrap_or_default()
            );

            sensor_array.push(sensor_doc);
        }

        // Warn if no sensors found
        if sensor_array.is_empty() {
            warn!("No temperature sensors detected. This is normal on some systems.");
        }

        // Create main document
        let doc = doc! {
            "node": node_id,
            "timestamp": Utc::now(),

            // Highest temperature across all sensors
            "max_temperature_c": max_temp as f64,

            // Number of sensors detected
            "sensor_count": sensor_array.len() as i32,

            // Individual sensor readings
            "sensors": sensor_array,
        };

        debug!(
            "Temperature: {} sensor(s), max={:.1}°C",
            components.iter().count(),
            max_temp
        );

        Ok(doc)
    }
}

impl Default for TemperatureCollector {
    fn default() -> Self {
        Self::new()
    }
}
```

---

## Best Practices

### Code Quality

1. **Always include comprehensive comments**
   ```rust
   // Good: Explains what and why
   // Calculate memory usage percentage
   // This helps identify when the system is under memory pressure
   let memory_percent = (used as f64 / total as f64) * 100.0;

   // Bad: No context
   let memory_percent = (used as f64 / total as f64) * 100.0;
   ```

2. **Use descriptive variable names**
   ```rust
   // Good
   let bytes_received_mb = bytes_to_mb(network.received());

   // Bad
   let rx = network.received() / (1024 * 1024);
   ```

3. **Handle errors gracefully**
   ```rust
   // Good: Specific error messages
   let data = read_sensor()
       .map_err(|e| format!("Failed to read temperature sensor: {}", e))?;

   // Bad: Generic error
   let data = read_sensor()?;
   ```

### Document Structure

1. **Always include standard fields**
   ```rust
   let doc = doc! {
       "node": node_id,        // Required
       "timestamp": Utc::now(), // Required
       // ... your fields
   };
   ```

2. **Use appropriate data types**
   - Integers: Use `i32` or `i64` (BSON doesn't support `u64`)
   - Floats: Use `f64` (BSON doesn't support `f32` directly)
   - Strings: Use `&str` or `String`
   - Arrays: Use `Vec<Document>` for nested documents

3. **Include units in field names**
   ```rust
   // Good: Clear units
   "memory_used_mb": 1024,
   "disk_space_gb": 500.0,
   "timeout_seconds": 30,

   // Bad: Ambiguous
   "memory_used": 1024,  // Bytes? MB? GB?
   "disk_space": 500,    // What unit?
   ```

### Performance

1. **Avoid blocking operations in async functions**
   ```rust
   // Good: Use tokio::task::spawn_blocking for CPU-heavy work
   let result = tokio::task::spawn_blocking(|| {
       expensive_calculation()
   }).await?;

   // Bad: Blocks the async runtime
   let result = expensive_calculation();
   ```

2. **Minimize allocations**
   ```rust
   // Good: Reuse capacity
   let mut interfaces = Vec::with_capacity(10);

   // Acceptable for small collections
   let mut interfaces = Vec::new();
   ```

3. **Choose appropriate intervals**
   - Fast metrics (< 1ms): 5-10 seconds
   - Moderate metrics (< 10ms): 10-30 seconds
   - Slow metrics (> 100ms): 60+ seconds

### Error Handling

1. **Provide helpful error messages**
   ```rust
   // Good: Explains the problem and potential solution
   if !docker_available() {
       return Err("Docker daemon is not running. \
                   Start Docker or disable this metric."
                   .into());
   }
   ```

2. **Log at appropriate levels**
   ```rust
   debug!("Collecting metric...");      // Normal operation
   info!("Metric collection started");  // Important events
   warn!("Docker unavailable");          // Degraded functionality
   error!("Failed to connect to DB");   // Serious problems
   ```

### Testing

1. **Test on target platform**
   - Metrics may behave differently on different OSes
   - Test with and without optional dependencies (e.g., Docker)

2. **Verify data in MongoDB**
   - Check document structure matches your schema
   - Verify data types are correct
   - Ensure timestamps are in UTC

3. **Test error scenarios**
   - What happens if Docker is stopped?
   - What if a file doesn't exist?
   - How does it handle permission denied?

---

## Testing

### Unit Testing

Add tests to your metric file:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_network_collector_basic() {
        let collector = NetworkCollector::new();

        // Test name
        assert_eq!(collector.name(), "NetworkIO");

        // Test collection (may fail on systems without network)
        match collector.collect("test-node").await {
            Ok(doc) => {
                // Verify required fields exist
                assert!(doc.contains_key("node"));
                assert!(doc.contains_key("timestamp"));
                assert!(doc.contains_key("interfaces"));
            }
            Err(e) => {
                // Collection may fail in CI environments
                println!("Collection failed (expected in some envs): {}", e);
            }
        }
    }

    #[test]
    fn test_bytes_to_mb() {
        assert_eq!(
            NetworkCollector::bytes_to_mb(1048576),
            1.0
        );
        assert_eq!(
            NetworkCollector::bytes_to_mb(2097152),
            2.0
        );
    }
}
```

Run tests:
```bash
cargo test
cargo test -- --nocapture  # See println! output
```

### Integration Testing

Test end-to-end with MongoDB:

```bash
# Start local MongoDB (if using Docker)
docker run -d -p 27017:27017 --name test-mongo mongo:latest

# Add test configuration
mongosh --eval '
use monitoring
db.MonitoringSettings.insertOne({
  "key": "test-node",
  "metric_settings": {
    "NetworkIO": {
      "timeout": 5,
      "collection": "network_io_test"
    }
  }
})
'

# Run the application
cargo run -- --mongodb "mongodb://localhost:27017" --key "test-node"

# Let it run for 30 seconds, then check MongoDB
mongosh --eval '
use monitoring
db.network_io_test.find().sort({timestamp: -1}).limit(5)
'

# Cleanup
docker stop test-mongo
docker rm test-mongo
```

---

## Deployment

### Build and Deploy

```bash
# Build release binary
cargo build --release

# Copy to server
scp target/release/metrics-collector user@server:/tmp/

# On server:
sudo systemctl stop metrics-collector
sudo cp /tmp/metrics-collector /opt/metrics-collector/
sudo systemctl start metrics-collector
sudo systemctl status metrics-collector
```

### Verify Deployment

```bash
# Check logs for your new metric
sudo journalctl -u metrics-collector -f | grep "NetworkIO"

# Should see:
# "Scheduling metric 'NetworkIO' with interval of 10s"
# "Collecting network I/O metrics"
```

### Update Documentation

Update this file with your new metric:

```markdown
## Available Metrics

- LoadAverage: System load averages
- Memory: RAM and swap usage
- DiskSpace: Disk usage per filesystem
- DockerStats: Container resource usage
- NetworkIO: Network interface statistics  <-- Add your metric
```

---

## Troubleshooting

### Metric Not Appearing

**Symptom:** No log messages for your metric

**Checklist:**
1. ✓ Added to `create_all_collectors()`?
2. ✓ Module declared in `metrics/mod.rs`?
3. ✓ Configuration in MongoDB?
4. ✓ Metric name matches in code and config?
5. ✓ Binary rebuilt after changes?

### Metric Failing to Collect

**Symptom:** Error messages in logs

**Debug steps:**
```bash
# Run with debug logging
RUST_LOG=debug ./metrics-collector --mongodb "..." --key "..."

# Check specific error
sudo journalctl -u metrics-collector | grep "Failed to collect"
```

**Common issues:**
- Missing system files (e.g., `/proc/` files)
- Permission denied
- Required service not running (e.g., Docker)
- Platform not supported

### Data Not in MongoDB

**Symptom:** No documents in collection

**Debug steps:**
```javascript
// 1. Check configuration exists
db.MonitoringSettings.findOne({ "key": "your-key" })

// 2. Check collection name matches
db.MonitoringSettings.findOne(
  { "key": "your-key" },
  { "metric_settings.NetworkIO": 1 }
)

// 3. Try different collection name
db.getCollectionNames()

// 4. Check for any recent documents
db.network_io_metrics.find().sort({timestamp: -1}).limit(1)
```

### Wrong Data Type Errors

**Symptom:** MongoDB insert errors about types

**Fix:** Use correct BSON types:
```rust
// Wrong: u64 (not supported)
"value": some_u64_value,

// Right: Convert to i64
"value": some_u64_value as i64,

// Wrong: f32 (may cause issues)
"value": 3.14f32,

// Right: Use f64
"value": 3.14f64,
```

---

## Advanced Topics

### Stateful Metrics

If you need to track state between collections (e.g., for calculating rates):

```rust
pub struct NetworkCollector {
    // Store previous values
    last_values: Arc<Mutex<HashMap<String, NetworkStats>>>,
}

async fn collect(&self, node_id: &str) -> Result<Document, Box<dyn Error + Send + Sync>> {
    // Get current values
    let current = get_current_stats();

    // Get previous values (with lock)
    let mut last = self.last_values.lock().await;

    // Calculate rate
    let rate = if let Some(prev) = last.get("interface") {
        (current.bytes - prev.bytes) / interval_seconds
    } else {
        0
    };

    // Update stored values
    last.insert("interface".to_string(), current.clone());

    // Return document with rates
    Ok(doc! { "rate_mbps": rate })
}
```

### External API Metrics

For metrics that call external APIs:

```rust
use reqwest;

async fn collect(&self, node_id: &str) -> Result<Document, Box<dyn Error + Send + Sync>> {
    // Make HTTP request (with timeout)
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()?;

    let response = client
        .get("https://api.example.com/metrics")
        .send()
        .await?
        .json::<ApiResponse>()
        .await?;

    // Convert API response to BSON document
    Ok(doc! {
        "node": node_id,
        "timestamp": Utc::now(),
        "api_value": response.value,
    })
}
```

---

## Summary Checklist

Before deploying your new metric:

- [ ] Code implements `MetricCollector` trait
- [ ] Comprehensive comments added
- [ ] Error handling implemented
- [ ] Debug logging included
- [ ] Registered in `create_all_collectors()`
- [ ] Module declared in `metrics/mod.rs`
- [ ] Dependencies added to `Cargo.toml`
- [ ] Configuration added to MongoDB
- [ ] Unit tests written and passing
- [ ] Integration tests performed
- [ ] Documentation updated
- [ ] Code reviewed
- [ ] Tested on target platform
- [ ] Verified data in MongoDB

---

## Getting Help

- Review existing metrics for examples
- Check architecture documentation for design patterns
- Test in isolation before adding to main collector
- Use `RUST_LOG=debug` for detailed logging
- Ask for code review before deployment

Happy metric collecting!
