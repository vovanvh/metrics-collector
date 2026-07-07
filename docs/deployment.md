# Deployment Guide - Metrics Collector

This guide provides detailed instructions for deploying the Metrics Collector on your servers.

## Table of Contents

1. [Prerequisites](#prerequisites)
2. [Building the Application](#building-the-application)
3. [MongoDB Setup](#mongodb-setup)
4. [Installation](#installation)
5. [SystemD Service Setup (Linux)](#systemd-service-setup-linux)
6. [launchd Service Setup (macOS)](#launchd-service-setup-macos)
7. [Windows Service Setup](#windows-service-setup)
8. [Verification](#verification)
9. [Troubleshooting](#troubleshooting)
10. [Uninstallation](#uninstallation)

---

## Prerequisites

### Required Software

- **Operating System**: Linux (Ubuntu 20.04+, CentOS 8+, or similar), macOS (12+), or Windows (10/Server 2016+)
- **Rust**: Version 1.70 or higher (for building)
- **MongoDB**: Version 4.4 or higher (accessible from your server)
- **Docker** (optional): If you want to monitor Docker containers
- **Service manager**: SystemD on Linux (standard on modern distros), launchd on macOS (built in), [NSSM](https://nssm.cc/) on Windows (the binary itself doesn't implement the Windows Service Control API, so `sc.exe create` alone won't work — see [Windows Service Setup](#windows-service-setup))

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

// Aggregated metrics (avg/min/max per store_timeout window)
db.createCollection("load_average_metrics")
db.createCollection("memory_metrics")
db.createCollection("disk_metrics")
db.createCollection("docker_metrics")

// Unaggregated log/event snapshots (one document per collect_timeout tick)
db.createCollection("process_cpu_logs")
db.createCollection("process_ram_logs")
db.createCollection("docker_event_logs")
db.createCollection("docker_container_logs")
db.createCollection("system_event_logs")
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
// Create compound indexes for efficient time-series queries — metrics
db.load_average_metrics.createIndex({ "node": 1, "timestamp": -1 })
db.memory_metrics.createIndex({ "node": 1, "timestamp": -1 })
db.disk_metrics.createIndex({ "node": 1, "timestamp": -1 })
db.docker_metrics.createIndex({ "node": 1, "timestamp": -1 })

// ...and for the log/event collections
db.process_cpu_logs.createIndex({ "node": 1, "timestamp": -1 })
db.process_ram_logs.createIndex({ "node": 1, "timestamp": -1 })
db.docker_event_logs.createIndex({ "node": 1, "timestamp": -1 })
db.docker_container_logs.createIndex({ "node": 1, "timestamp": -1 })
db.system_event_logs.createIndex({ "node": 1, "timestamp": -1 })

// Optional: TTL index to auto-delete old data (e.g., after 30 days for metrics)
db.load_average_metrics.createIndex({ "timestamp": 1 }, { expireAfterSeconds: 2592000 })
db.memory_metrics.createIndex({ "timestamp": 1 }, { expireAfterSeconds: 2592000 })
db.disk_metrics.createIndex({ "timestamp": 1 }, { expireAfterSeconds: 2592000 })
db.docker_metrics.createIndex({ "timestamp": 1 }, { expireAfterSeconds: 2592000 })

// Log/event collections are only useful for root-cause analysis right after
// an anomaly fires — a much shorter TTL (e.g. 1 hour) is appropriate
db.process_cpu_logs.createIndex({ "timestamp": 1 }, { expireAfterSeconds: 3600 })
db.process_ram_logs.createIndex({ "timestamp": 1 }, { expireAfterSeconds: 3600 })
db.docker_event_logs.createIndex({ "timestamp": 1 }, { expireAfterSeconds: 3600 })
db.docker_container_logs.createIndex({ "timestamp": 1 }, { expireAfterSeconds: 3600 })
db.system_event_logs.createIndex({ "timestamp": 1 }, { expireAfterSeconds: 3600 })
```

> These TTL indexes are set up manually for now — `metrics-collector` doesn't create them itself yet (tracked as MC-6).

Alternatively, run the application with `--create-indexes` on first start and it will create the compound `(node, timestamp)` indexes automatically for all 9 collections (metrics and logs) — but not the TTL ones above, those must be created manually.

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

## SystemD Service Setup (Linux)

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

## launchd Service Setup (macOS)

macOS doesn't use systemd — the equivalent is **launchd**. To run the collector as a background service that starts automatically on boot (even before anyone logs in) and restarts if it crashes, install it as a **LaunchDaemon**.

> If you'd rather have it start only when a specific user logs in (not on boot), install the same plist under `~/Library/LaunchAgents/` instead of `/Library/LaunchDaemons/` and drop the `UserName`/`GroupName` keys — see the note at the end of this section.

### 1. Build the Binary

If you're building directly on the Mac:

```bash
cd /path/to/metrics-collector
cargo build --release
# Binary at: target/release/metrics-collector
```

### 2. Create a Dedicated User (optional, recommended)

```bash
# macOS system users need a free UID in the system range (typically < 500)
sudo dscl . -create /Users/_metricscollector
sudo dscl . -create /Users/_metricscollector UserShell /usr/bin/false
sudo dscl . -create /Users/_metricscollector RealName "Metrics Collector"
sudo dscl . -create /Users/_metricscollector UniqueID 399
sudo dscl . -create /Users/_metricscollector PrimaryGroupID 20
sudo dscl . -create /Users/_metricscollector NFSHomeDirectory /var/empty
```

(Skip this and drop the `UserName`/`GroupName` keys from the plist below to run as root instead — simpler, but less isolated.)

### 3. Install the Binary

```bash
sudo mkdir -p /usr/local/opt/metrics-collector
sudo cp target/release/metrics-collector /usr/local/opt/metrics-collector/
sudo chown -R _metricscollector:staff /usr/local/opt/metrics-collector
sudo chmod 755 /usr/local/opt/metrics-collector/metrics-collector
```

### 4. Create the launchd Plist

Create `/Library/LaunchDaemons/com.metrics-collector.plist`:

```bash
sudo nano /Library/LaunchDaemons/com.metrics-collector.plist
```

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.metrics-collector</string>

    <key>ProgramArguments</key>
    <array>
        <string>/usr/local/opt/metrics-collector/metrics-collector</string>
        <string>--mongodb</string>
        <string>mongodb://YOUR_MONGODB_HOST:27017</string>
        <string>--key</string>
        <string>YOUR_NODE_KEY</string>
        <string>--database</string>
        <string>monitoring</string>
    </array>

    <!-- Start on boot -->
    <key>RunAtLoad</key>
    <true/>

    <!-- Restart automatically if it crashes or is killed -->
    <key>KeepAlive</key>
    <true/>

    <!-- Run as the dedicated non-root user (remove these two keys to run as root) -->
    <key>UserName</key>
    <string>_metricscollector</string>
    <key>GroupName</key>
    <string>staff</string>

    <key>StandardOutPath</key>
    <string>/var/log/metrics-collector.log</string>
    <key>StandardErrorPath</key>
    <string>/var/log/metrics-collector.error.log</string>

    <key>EnvironmentVariables</key>
    <dict>
        <key>RUST_LOG</key>
        <string>info</string>
    </dict>
</dict>
</plist>
```

For MongoDB with authentication, add the credentials into the `--mongodb` argument string as usual:
`mongodb://username:password@host:27017/monitoring?authSource=admin`

### 5. Set Permissions and Load the Service

```bash
sudo chown root:wheel /Library/LaunchDaemons/com.metrics-collector.plist
sudo chmod 644 /Library/LaunchDaemons/com.metrics-collector.plist

# Pre-create log files owned by the service user (launchd won't chown them for you)
sudo touch /var/log/metrics-collector.log /var/log/metrics-collector.error.log
sudo chown _metricscollector:staff /var/log/metrics-collector.log /var/log/metrics-collector.error.log

# Modern macOS (12+): bootstrap into the system domain and enable
sudo launchctl bootstrap system /Library/LaunchDaemons/com.metrics-collector.plist
sudo launchctl enable system/com.metrics-collector

# Older macOS, if bootstrap isn't available:
# sudo launchctl load -w /Library/LaunchDaemons/com.metrics-collector.plist
```

### 6. Verify It's Running

```bash
sudo launchctl print system/com.metrics-collector | head -20
tail -f /var/log/metrics-collector.log
```

You should see `state = running` and a PID in the `print` output.

### Managing the Service

```bash
# Stop (without disabling boot start)
sudo launchctl kickstart -k system/com.metrics-collector   # restart
sudo launchctl bootout system/com.metrics-collector          # stop + unload

# Reload after editing the plist
sudo launchctl bootout system/com.metrics-collector
sudo launchctl bootstrap system /Library/LaunchDaemons/com.metrics-collector.plist
```

### Running Per-User Instead of at Boot

To start only when a specific user logs in rather than at system boot:

1. Save the plist to `~/Library/LaunchAgents/com.metrics-collector.plist` instead.
2. Remove the `UserName` and `GroupName` keys (the agent already runs as the logged-in user).
3. Load it without `sudo` and in the `gui/<uid>` domain:
   ```bash
   launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/com.metrics-collector.plist
   ```

---

## Windows Service Setup

`metrics-collector` is a plain console binary — it doesn't implement the Windows Service Control API (`StartServiceCtrlDispatcher`), so registering it directly with `sc.exe create` will fail (error 1053, "the service did not respond in a timely fashion"). The standard fix is a lightweight wrapper that speaks the Service Control API on the binary's behalf and just launches/monitors the real executable: **[NSSM](https://nssm.cc/)** (the Non-Sucking Service Manager).

> Prefer not to install a third-party tool? See [Alternative: Task Scheduler](#alternative-task-scheduler-no-third-party-tool) below — simpler, but not a true service (won't show up in `services.msc`).

### 1. Build the Binary

**Natively on Windows:**
```powershell
cargo build --release
# Binary at: target\release\metrics-collector.exe
```

**Cross-compiled from Linux/macOS:**
```bash
rustup target add x86_64-pc-windows-gnu
cargo build --release --target x86_64-pc-windows-gnu
# Binary at: target/x86_64-pc-windows-gnu/release/metrics-collector.exe

# Copy to the Windows machine (e.g. via scp, or any file share)
scp target/x86_64-pc-windows-gnu/release/metrics-collector.exe user@windows-host:/path/
```

### 2. Install the Binary

```powershell
New-Item -ItemType Directory -Force -Path "C:\Program Files\metrics-collector"
Copy-Item target\release\metrics-collector.exe "C:\Program Files\metrics-collector\"
```

### 3. Install NSSM

```powershell
# Via winget
winget install NSSM.NSSM

# Or download the binary directly from https://nssm.cc/download and put nssm.exe on your PATH
```

### 4. Register the Service

Run as Administrator:

```powershell
nssm install metrics-collector "C:\Program Files\metrics-collector\metrics-collector.exe"
nssm set metrics-collector AppParameters '--mongodb "mongodb://YOUR_MONGODB_HOST:27017" --key "YOUR_NODE_KEY" --database "monitoring"'
nssm set metrics-collector AppDirectory "C:\Program Files\metrics-collector"

# Start on boot, restart automatically if it crashes
nssm set metrics-collector Start SERVICE_AUTO_START
nssm set metrics-collector AppExit Default Restart
nssm set metrics-collector AppRestartDelay 5000

# Redirect stdout/stderr to log files (NSSM handles rotation via AppRotateFiles if desired)
nssm set metrics-collector AppStdout "C:\Program Files\metrics-collector\metrics-collector.log"
nssm set metrics-collector AppStderr "C:\Program Files\metrics-collector\metrics-collector.error.log"
```

For MongoDB with authentication, put the credentials in the `AppParameters` connection string as usual: `mongodb://username:password@host:27017/monitoring?authSource=admin`.

### 5. Start and Verify

```powershell
nssm start metrics-collector

# Check status
Get-Service metrics-collector
nssm status metrics-collector

# Tail the log
Get-Content "C:\Program Files\metrics-collector\metrics-collector.log" -Wait -Tail 20
```

### Managing the Service

```powershell
nssm stop metrics-collector
nssm restart metrics-collector
nssm edit metrics-collector   # opens a GUI to change any setting above
```

Since it's registered as a real Windows service, the standard tools also work: `services.msc`, `Start-Service metrics-collector`, `Stop-Service metrics-collector`, `sc query metrics-collector`.

### Alternative: Task Scheduler (no third-party tool)

If you'd rather not install NSSM, Task Scheduler can run the binary at boot and restart it on failure, though it won't behave like a real service (no `services.msc` entry, no dependency ordering):

```powershell
$action = New-ScheduledTaskAction -Execute "C:\Program Files\metrics-collector\metrics-collector.exe" `
    -Argument '--mongodb "mongodb://YOUR_MONGODB_HOST:27017" --key "YOUR_NODE_KEY" --database "monitoring"'
$trigger = New-ScheduledTaskTrigger -AtStartup
$settings = New-ScheduledTaskSettingsSet -RestartCount 999 -RestartInterval (New-TimeSpan -Minutes 1) `
    -ExecutionTimeLimit (New-TimeSpan -Days 0)
$principal = New-ScheduledTaskPrincipal -UserId "SYSTEM" -LogonType ServiceAccount -RunLevel Highest

Register-ScheduledTask -TaskName "metrics-collector" -Action $action -Trigger $trigger `
    -Settings $settings -Principal $principal

Start-ScheduledTask -TaskName "metrics-collector"
```

---

## Verification

### 1. Check Service Status

**Linux:**
```bash
sudo systemctl status metrics-collector
sudo journalctl -u metrics-collector -f
```

**macOS:**
```bash
sudo launchctl print system/com.metrics-collector | head -20
tail -f /var/log/metrics-collector.log
```

**Windows:**
```powershell
Get-Service metrics-collector
Get-Content "C:\Program Files\metrics-collector\metrics-collector.log" -Wait -Tail 20
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

// Log/event collections — one document per collect_timeout (or collect_docker_timeout
// for the two Docker-facing ones) tick, not per store_timeout window
db.process_cpu_logs.find({ "node": "0001-0001" }).sort({ timestamp: -1 }).limit(2)
db.process_ram_logs.find({ "node": "0001-0001" }).sort({ timestamp: -1 }).limit(2)
db.docker_event_logs.find({ "node": "0001-0001" }).sort({ timestamp: -1 }).limit(2)
db.docker_container_logs.find({ "node": "0001-0001" }).sort({ timestamp: -1 }).limit(2)
db.system_event_logs.find({ "node": "0001-0001" }).sort({ timestamp: -1 }).limit(2)
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

**Linux:**
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

**macOS:**
```bash
sudo launchctl bootout system/com.metrics-collector
sudo rm /Library/LaunchDaemons/com.metrics-collector.plist
sudo rm -rf /usr/local/opt/metrics-collector
sudo rm /var/log/metrics-collector.log /var/log/metrics-collector.error.log
sudo dscl . -delete /Users/_metricscollector

# Remove MongoDB data (optional)
# mongosh "mongodb://your-mongodb-host:27017"
# use monitoring
# db.dropDatabase()
```

**Windows (NSSM):**
```powershell
nssm stop metrics-collector
nssm remove metrics-collector confirm
Remove-Item -Recurse -Force "C:\Program Files\metrics-collector"

# Remove MongoDB data (optional)
# mongosh "mongodb://your-mongodb-host:27017"
# use monitoring
# db.dropDatabase()
```

**Windows (Task Scheduler alternative):**
```powershell
Stop-ScheduledTask -TaskName "metrics-collector"
Unregister-ScheduledTask -TaskName "metrics-collector" -Confirm:$false
Remove-Item -Recurse -Force "C:\Program Files\metrics-collector"
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
