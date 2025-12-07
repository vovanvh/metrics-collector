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
cd /path/to/rust-project

# Build in release mode (optimized binary)
cargo build --release

# The binary will be at: target/release/metrics-collector
```

### Option 2: Cross-Compile from Development Machine

```bash
# On your development machine
cd /path/to/rust-project

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

// The collections will be created automatically by the application
// But you can create them manually if needed:
db.createCollection("MonitoringSettings")
db.createCollection("load_average_metrics")
db.createCollection("memory_metrics")
db.createCollection("disk_metrics")
db.createCollection("docker_metrics")
```

### 2. Create Configuration Document

Insert a configuration document for your server:

```javascript
// Still in MongoDB shell
use monitoring

// Insert configuration for node "1111-1111"
db.MonitoringSettings.insertOne({
  "key": "1111-1111",
  "metric_settings": {
    "LoadAverage": {
      "timeout": 5,
      "collection": "load_average_metrics"
    },
    "Memory": {
      "timeout": 10,
      "collection": "memory_metrics"
    },
    "DiskSpace": {
      "timeout": 30,
      "collection": "disk_metrics"
    },
    "DockerStats": {
      "timeout": 15,
      "collection": "docker_metrics"
    }
  }
})

// Verify the document was created
db.MonitoringSettings.findOne({ "key": "1111-1111" })
```

### 3. Create Indexes (Recommended for Production)

```javascript
// Create indexes for better query performance
// These make queries by node and time much faster

db.load_average_metrics.createIndex({ "node": 1, "timestamp": -1 })
db.memory_metrics.createIndex({ "node": 1, "timestamp": -1 })
db.disk_metrics.createIndex({ "node": 1, "timestamp": -1 })
db.docker_metrics.createIndex({ "node": 1, "timestamp": -1 })

// Optional: Create TTL index to auto-delete old data (e.g., after 30 days)
// This keeps your database size manageable
db.load_average_metrics.createIndex(
  { "timestamp": 1 },
  { expireAfterSeconds: 2592000 }  // 30 days
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

### Configuration Field Explanation

| Field | Description | Example |
|-------|-------------|---------|
| `key` | Unique identifier for this server/node | `"1111-1111"`, `"server-01"` |
| `timeout` | Collection interval in seconds | `5` = collect every 5 seconds |
| `collection` | MongoDB collection name for storing metrics | `"load_average_metrics"` |

---

## Installation

### 1. Create Dedicated User (Security Best Practice)

```bash
# Create a system user for running the service
# -r: system account
# -s /bin/false: no shell access (security)
# -m: create home directory
sudo useradd -r -s /bin/false -m metrics-collector

# Optional: Add user to docker group (if monitoring Docker)
sudo usermod -aG docker metrics-collector
```

### 2. Create Installation Directory

```bash
# Create directory for the application
sudo mkdir -p /opt/metrics-collector

# Copy the binary
sudo cp target/release/metrics-collector /opt/metrics-collector/

# Set ownership
sudo chown -R metrics-collector:metrics-collector /opt/metrics-collector

# Make binary executable
sudo chmod 755 /opt/metrics-collector/metrics-collector
```

### 3. Verify Binary Works

Test the binary before setting up the service:

```bash
# Test run (will fail to connect to MongoDB, but verifies binary works)
sudo -u metrics-collector /opt/metrics-collector/metrics-collector \
  --mongodb "mongodb://your-mongodb-host:27017" \
  --key "1111-1111"

# Press Ctrl+C to stop after a few seconds
# You should see log output indicating connection attempts
```

---

## SystemD Service Setup

### 1. Copy Service File

```bash
# Copy the systemd service file
sudo cp metrics-collector.service /etc/systemd/system/

# Set proper permissions
sudo chmod 644 /etc/systemd/system/metrics-collector.service
```

### 2. Configure Service File

Edit the service file to match your environment:

```bash
sudo nano /etc/systemd/system/metrics-collector.service
```

**Update these values:**

```ini
# MongoDB connection string
# Replace with your actual MongoDB URI
ExecStart=/opt/metrics-collector/metrics-collector \
    --mongodb "mongodb://YOUR_MONGODB_HOST:27017" \
    --key "YOUR_NODE_KEY" \
    --database "monitoring"

# Optional: Add --create-indexes on first run
# Remove this flag after first run to improve startup time
ExecStart=/opt/metrics-collector/metrics-collector \
    --mongodb "mongodb://YOUR_MONGODB_HOST:27017" \
    --key "YOUR_NODE_KEY" \
    --database "monitoring" \
    --create-indexes
```

**For MongoDB with authentication:**

```ini
ExecStart=/opt/metrics-collector/metrics-collector \
    --mongodb "mongodb://username:password@host:27017/monitoring?authSource=admin" \
    --key "1111-1111"
```

### 3. Enable and Start Service

```bash
# Reload systemd to recognize the new service
sudo systemctl daemon-reload

# Enable the service to start on boot
sudo systemctl enable metrics-collector

# Start the service now
sudo systemctl start metrics-collector

# Check service status
sudo systemctl status metrics-collector
```

Expected output:
```
● metrics-collector.service - Metrics Collector - Server Monitoring Tool
     Loaded: loaded (/etc/systemd/system/metrics-collector.service; enabled)
     Active: active (running) since Mon 2024-01-15 10:30:00 UTC; 5s ago
   Main PID: 12345 (metrics-collect)
      Tasks: 8
     Memory: 12.5M
        CPU: 100ms
     CGroup: /system.slice/metrics-collector.service
             └─12345 /opt/metrics-collector/metrics-collector --mongodb...
```

---

## Verification

### 1. Check Service Status

```bash
# View service status
sudo systemctl status metrics-collector

# View live logs
sudo journalctl -u metrics-collector -f

# View logs from the last hour
sudo journalctl -u metrics-collector --since "1 hour ago"

# View logs from today
sudo journalctl -u metrics-collector --since today
```

### 2. Verify Data in MongoDB

Connect to MongoDB and check if metrics are being stored:

```javascript
// Connect to MongoDB
mongosh "mongodb://your-mongodb-host:27017"

// Switch to monitoring database
use monitoring

// Check load average metrics (should have recent entries)
db.load_average_metrics.find({ "node": "1111-1111" }).sort({ timestamp: -1 }).limit(5)

// Check memory metrics
db.memory_metrics.find({ "node": "1111-1111" }).sort({ timestamp: -1 }).limit(5)

// Check disk metrics
db.disk_metrics.find({ "node": "1111-1111" }).sort({ timestamp: -1 }).limit(5)

// Check docker metrics (if Docker is running)
db.docker_metrics.find({ "node": "1111-1111" }).sort({ timestamp: -1 }).limit(5)

// Count total documents (should increase over time)
db.load_average_metrics.countDocuments({ "node": "1111-1111" })
```

### 3. Test Automatic Restart

Verify the service restarts automatically on failure:

```bash
# Kill the process
sudo pkill metrics-collector

# Wait a few seconds, then check status
sleep 5
sudo systemctl status metrics-collector

# Should show: active (running)
# And "Restart" count should have increased
```

---

## Troubleshooting

### Service Won't Start

**Check logs for errors:**
```bash
sudo journalctl -u metrics-collector -n 50 --no-pager
```

**Common issues:**

1. **MongoDB connection failed**
   - Verify MongoDB is running: `systemctl status mongod`
   - Check network connectivity: `telnet mongodb-host 27017`
   - Verify credentials in connection string
   - Check firewall: `sudo ufw status`

2. **Permission denied**
   - Check binary permissions: `ls -l /opt/metrics-collector/`
   - Verify user exists: `id metrics-collector`
   - Check Docker socket permissions: `ls -l /var/run/docker.sock`

3. **Settings not found**
   - Verify MongoDB has the configuration document
   - Check the key matches: `db.MonitoringSettings.findOne({ "key": "1111-1111" })`

### Docker Stats Not Working

If Docker statistics are not being collected:

```bash
# Verify Docker is running
sudo systemctl status docker

# Check Docker socket exists
ls -l /var/run/docker.sock

# Add user to docker group (if not already done)
sudo usermod -aG docker metrics-collector

# Restart service after group change
sudo systemctl restart metrics-collector

# Test Docker access manually
sudo -u metrics-collector docker ps
```

### High Memory Usage

If the service uses too much memory:

```bash
# Check current memory usage
systemctl status metrics-collector | grep Memory

# Lower the memory limit in service file
sudo nano /etc/systemd/system/metrics-collector.service
# Change: MemoryLimit=512M to MemoryLimit=256M

# Reload and restart
sudo systemctl daemon-reload
sudo systemctl restart metrics-collector
```

### Logs Not Appearing

```bash
# Check systemd journal is working
sudo systemctl status systemd-journald

# View all logs for the service
sudo journalctl -u metrics-collector --no-pager

# Enable persistent logging (survives reboots)
sudo mkdir -p /var/log/journal
sudo systemctl restart systemd-journald
```

---

## Uninstallation

To completely remove the metrics collector:

```bash
# Stop and disable the service
sudo systemctl stop metrics-collector
sudo systemctl disable metrics-collector

# Remove service file
sudo rm /etc/systemd/system/metrics-collector.service

# Reload systemd
sudo systemctl daemon-reload

# Remove application directory
sudo rm -rf /opt/metrics-collector

# Remove user (optional)
sudo userdel -r metrics-collector

# Remove MongoDB data (optional - only if you want to delete all metrics)
# mongosh "mongodb://your-mongodb-host:27017"
# use monitoring
# db.dropDatabase()
```

---

## Multiple Servers Setup

To deploy on multiple servers:

1. **Create unique keys for each server:**
   ```javascript
   db.MonitoringSettings.insertOne({
     "key": "server-01",
     "metric_settings": { /* same as before */ }
   })

   db.MonitoringSettings.insertOne({
     "key": "server-02",
     "metric_settings": { /* same as before */ }
   })
   ```

2. **Deploy binary to each server**

3. **Configure service file with correct key:**
   - Server 1: `--key "server-01"`
   - Server 2: `--key "server-02"`

4. **Start services on all servers**

5. **Query metrics by node:**
   ```javascript
   // Get metrics from specific server
   db.load_average_metrics.find({ "node": "server-01" })

   // Get metrics from all servers
   db.load_average_metrics.find({})
   ```

---

## Production Recommendations

1. **Use MongoDB Authentication**
   - Don't run MongoDB without authentication in production
   - Use strong passwords and user-specific permissions

2. **Set Up Log Rotation**
   - Systemd handles this automatically
   - Configure max log size: edit `/etc/systemd/journald.conf`

3. **Monitor the Monitor**
   - Set up alerts if the service stops
   - Use systemd email notifications or external monitoring

4. **Regular Backups**
   - Back up MongoDB data regularly
   - Export metrics periodically for long-term storage

5. **Resource Limits**
   - Set appropriate CPU and memory limits
   - Monitor resource usage and adjust as needed

6. **Security**
   - Run as non-root user (already configured)
   - Restrict network access to MongoDB
   - Use firewall rules to protect your servers

---

## Support

For issues, questions, or contributions, please refer to:
- Architecture documentation: `docs/architecture.md`
- Adding metrics guide: `docs/adding-new-metrics.md`
- Source code comments for implementation details
