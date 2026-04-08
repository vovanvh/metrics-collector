# Deployment Guide - Metrics Collector

This guide provides detailed instructions for deploying the Metrics Collector on your servers.

## Table of Contents

1. [Prerequisites](#prerequisites)
2. [Building the Application](#building-the-application)
3. [MongoDB Setup](#mongodb-setup)
4. [Installation](#installation)
5. [SystemD Service Setup](#systemd-service-setup)
6. [Verification](#verification)
7. [Troubleshooting](#troubleshooting)
8. [Uninstallation](#uninstallation)

---

## Prerequisites

### Required Software

- **Operating System**: Linux (Ubuntu 20.04+, CentOS 8+, or similar)
- **Rust**: Version 1.70 or higher (for building)
- **MongoDB**: Version 4.4 or higher (accessible from your server)
- **Docker** (optional): If you want to monitor Docker containers
- **SystemD**: For service management (standard on modern Linux)

### Required Permissions

- Root access or sudo privileges for installation
- Permission to access Docker socket (if monitoring Docker)
- Network access to MongoDB server

---

## Building the Application

### Option 1: Build on Target Server

```bash
# Install Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env

# Clone or copy the project to your server
cd /path/to/metrics-collector

# Build in release mode (optimized binary)
cargo build --release

# The binary will be at: target/release/metrics-collector
```

### Option 2: Cross-Compile from Development Machine

```bash
# On your development machine
cd /path/to/metrics-collector

# Build for Linux (if you're on macOS/Windows)
# First, add the target
rustup target add x86_64-unknown-linux-musl

# Build
cargo build --release --target x86_64-unknown-linux-musl

# Copy binary to server
scp target/x86_64-unknown-linux-musl/release/metrics-collector \
    user@your-server:/tmp/
```

### Binary Size Optimization

The release build is already optimized, but you can further reduce size:

```bash
# Install UPX (optional)
sudo apt-get install upx

# Compress the binary (reduces size by ~60%)
upx --best --lzma target/release/metrics-collector
```

---

## MongoDB Setup

### 1. Prepare MongoDB Database

Connect to your MongoDB instance and create the database:

```javascript
// Connect to MongoDB
mongosh "mongodb://your-mongodb-host:27017"

// Switch to monitoring database
use monitoring

// Collections are created automatically by the application
// You can create them manually if preferred:
db.createCollection("MonitoringSettings")
db.createCollection("load_average_metrics")
db.createCollection("memory_metrics")
db.createCollection("disk_metrics")
db.createCollection("docker_metrics")
```

### 2. Create Configuration Document

Insert a configuration document for your server:

```javascript
use monitoring

db.MonitoringSettings.insertOne({
  "key": "0001-0001",
  "collect_timeout": 5,           // seconds between samples for most metrics
  "collect_docker_timeout": 20,   // seconds between Docker samples
  "store_timeout": 60             // seconds per aggregation window (flush interval)
})

// Verify the document was created
db.MonitoringSettings.findOne({ "key": "0001-0001" })
```

**Configuration fields:**

| Field | Description | Default |
|-------|-------------|---------|
| `key` | Unique identifier for this server/node | — |
| `collect_timeout` | Seconds between raw samples for LoadAverage, Memory, DiskSpace | `5` |
| `collect_docker_timeout` | Seconds between raw samples for DockerStats | `20` |
| `store_timeout` | Length of each aggregation window in seconds | `60` |

> **Live reload:** The application re-reads this document from MongoDB after every flush. Change any value and the new setting takes effect after the current window completes — no restart needed.

### 3. Create Indexes (Recommended for Production)

```javascript
// Create compound indexes for efficient time-series queries
db.load_average_metrics.createIndex({ "node": 1, "timestamp": -1 })
db.memory_metrics.createIndex({ "node": 1, "timestamp": -1 })
db.disk_metrics.createIndex({ "node": 1, "timestamp": -1 })
db.docker_metrics.createIndex({ "node": 1, "timestamp": -1 })

// Optional: TTL index to auto-delete old data (e.g., after 30 days)
db.load_average_metrics.createIndex(
  { "timestamp": 1 },
  { expireAfterSeconds: 2592000 }
)
db.memory_metrics.createIndex(
  { "timestamp": 1 },
  { expireAfterSeconds: 2592000 }
)
db.disk_metrics.createIndex(
  { "timestamp": 1 },
  { expireAfterSeconds: 2592000 }
)
db.docker_metrics.createIndex(
  { "timestamp": 1 },
  { expireAfterSeconds: 2592000 }
)
```

Alternatively, run the application with `--create-indexes` on first start and it will create the compound `(node, timestamp)` indexes automatically.

---

## Installation

### 1. Create Dedicated User (Security Best Practice)

```bash
# Create a system user for running the service
sudo useradd -r -s /bin/false -m metrics-collector

# Optional: Add user to docker group (if monitoring Docker)
sudo usermod -aG docker metrics-collector
```

### 2. Create Installation Directory

```bash
sudo mkdir -p /opt/metrics-collector

sudo cp target/release/metrics-collector /opt/metrics-collector/

sudo chown -R metrics-collector:metrics-collector /opt/metrics-collector

sudo chmod 755 /opt/metrics-collector/metrics-collector
```

### 3. Verify Binary Works

```bash
sudo -u metrics-collector /opt/metrics-collector/metrics-collector \
  --mongodb "mongodb://your-mongodb-host:27017" \
  --key "0001-0001"
# Press Ctrl+C after a few seconds
```

---

## SystemD Service Setup

### 1. Copy Service File

```bash
sudo cp metrics-collector.service /etc/systemd/system/
sudo chmod 644 /etc/systemd/system/metrics-collector.service
```

### 2. Configure Service File

```bash
sudo nano /etc/systemd/system/metrics-collector.service
```

Update `ExecStart`:

```ini
ExecStart=/opt/metrics-collector/metrics-collector \
    --mongodb "mongodb://YOUR_MONGODB_HOST:27017" \
    --key "YOUR_NODE_KEY" \
    --database "monitoring"
```

For MongoDB with authentication:

```ini
ExecStart=/opt/metrics-collector/metrics-collector \
    --mongodb "mongodb://username:password@host:27017/monitoring?authSource=admin" \
    --key "0001-0001"
```

### 3. Enable and Start Service

```bash
sudo systemctl daemon-reload
sudo systemctl enable metrics-collector
sudo systemctl start metrics-collector
sudo systemctl status metrics-collector
```

Expected output:
```
● metrics-collector.service - Metrics Collector - Server Monitoring Tool
     Loaded: loaded (/etc/systemd/system/metrics-collector.service; enabled)
     Active: active (running) since Mon 2024-01-15 10:30:00 UTC; 5s ago
   Main PID: 12345 (metrics-collect)
     Memory: 12.5M
        CPU: 100ms
```

---

## Verification

### 1. Check Service Status

```bash
sudo systemctl status metrics-collector
sudo journalctl -u metrics-collector -f
```

### 2. Verify Data in MongoDB

Wait ~65 seconds after startup, then check:

```javascript
mongosh "mongodb://your-mongodb-host:27017"
use monitoring

// Load average — fields should be {avg, min, max} objects
db.load_average_metrics.find({ "node": "0001-0001" }).sort({ timestamp: -1 }).limit(2)
// Expected shape:
// { "cpu_cores": 8, "load_1min": { "avg": 0.5, "min": 0.1, "max": 1.2 }, ... }

// Memory — total_mb and swap_total_mb are plain values; others are {avg, min, max}
db.memory_metrics.find({ "node": "0001-0001" }).sort({ timestamp: -1 }).limit(2)

// Disk — unchanged nested structure
db.disk_metrics.find({ "node": "0001-0001" }).sort({ timestamp: -1 }).limit(2)

// Docker — per-container cpu/memory are {avg, min, max}; network/block are plain
db.docker_metrics.find({ "node": "0001-0001" }).sort({ timestamp: -1 }).limit(2)

// Confirm each collection grows by exactly 1 document per store_timeout seconds
db.load_average_metrics.countDocuments({ "node": "0001-0001" })
```

### 3. Test Settings Reload

Change a timeout in MongoDB and confirm it takes effect after the next flush:

```javascript
// Lower collect_timeout to 10 seconds
db.MonitoringSettings.updateOne(
  { "key": "0001-0001" },
  { $set: { "collect_timeout": 10 } }
)
```

After the current 60-second window ends, the new value will be active. No restart needed.

### 4. Test Automatic Restart

```bash
sudo pkill metrics-collector
sleep 5
sudo systemctl status metrics-collector
# Should show: active (running)
```

---

## Troubleshooting

### Service Won't Start

```bash
sudo journalctl -u metrics-collector -n 50 --no-pager
```

**Common issues:**

1. **MongoDB connection failed**
   - Verify MongoDB is running: `systemctl status mongod`
   - Check network: `telnet mongodb-host 27017`
   - Verify credentials in connection string

2. **Settings not found**
   - The settings document must use the new three-field format
   - Check: `db.MonitoringSettings.findOne({ "key": "your-key" })`
   - Must have `collect_timeout`, `collect_docker_timeout`, `store_timeout` fields

3. **Permission denied**
   - Check binary permissions: `ls -l /opt/metrics-collector/`
   - Verify user exists: `id metrics-collector`
   - Check Docker socket: `ls -l /var/run/docker.sock`

### Docker Stats Not Working

```bash
sudo systemctl status docker
ls -l /var/run/docker.sock
sudo usermod -aG docker metrics-collector
sudo systemctl restart metrics-collector
sudo -u metrics-collector docker ps
```

### No Data Appearing in MongoDB

The application buffers samples for `store_timeout` seconds before writing. With the default 60-second window, the first document appears after ~65 seconds. If no data appears after 2 minutes:

```bash
# Check logs for flush messages
sudo journalctl -u metrics-collector | grep -E "flush|store|sample"

# Verify settings document has correct fields
# (old metric_settings format is no longer supported)
db.MonitoringSettings.findOne({ "key": "your-key" })
```

### Logs Not Appearing

```bash
sudo systemctl status systemd-journald
sudo journalctl -u metrics-collector --no-pager
sudo mkdir -p /var/log/journal
sudo systemctl restart systemd-journald
```

---

## Migrating from Old Settings Format

If you have an existing settings document with `metric_settings`, migrate it:

```javascript
use monitoring

db.MonitoringSettings.updateOne(
  { "key": "your-key" },
  {
    $set: {
      "collect_timeout": 5,
      "collect_docker_timeout": 20,
      "store_timeout": 60
    },
    $unset: { "metric_settings": "" }
  }
)
```

Then restart the service.

---

## Uninstallation

```bash
sudo systemctl stop metrics-collector
sudo systemctl disable metrics-collector
sudo rm /etc/systemd/system/metrics-collector.service
sudo systemctl daemon-reload
sudo rm -rf /opt/metrics-collector
sudo userdel -r metrics-collector

# Remove MongoDB data (optional)
# mongosh "mongodb://your-mongodb-host:27017"
# use monitoring
# db.dropDatabase()
```

---

## Multiple Servers Setup

1. **Create a settings document per server:**
   ```javascript
   db.MonitoringSettings.insertOne({
     "key": "server-01",
     "collect_timeout": 5,
     "collect_docker_timeout": 20,
     "store_timeout": 60
   })

   db.MonitoringSettings.insertOne({
     "key": "server-02",
     "collect_timeout": 5,
     "collect_docker_timeout": 20,
     "store_timeout": 60
   })
   ```

2. **Deploy binary to each server**

3. **Configure each service with the correct key:**
   - Server 1: `--key "server-01"`
   - Server 2: `--key "server-02"`

4. **Query metrics by node:**
   ```javascript
   db.load_average_metrics.find({ "node": "server-01" })
   db.load_average_metrics.find({})  // All servers
   ```

---

## Production Recommendations

1. **Use MongoDB Authentication** — don't run without auth in production
2. **Set TTL indexes** — keep database size manageable
3. **Monitor the Monitor** — set alerts if the service stops
4. **Regular Backups** — back up MongoDB data regularly
5. **Security** — run as non-root (already configured), restrict MongoDB network access

---

## Support

For issues, questions, or contributions, please refer to:
- Architecture documentation: `docs/architecture.md`
- Adding metrics guide: `docs/adding-new-metrics.md`
- Source code comments for implementation details
