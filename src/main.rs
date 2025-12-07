// Metrics Collector - Server Monitoring Tool
//
// A Rust-based monitoring tool that collects system metrics and stores them in MongoDB.
// Supports multiple metric types with independent collection intervals.
//
// # Features
// - Load average monitoring
// - Memory usage tracking
// - Disk space monitoring
// - Docker container statistics
// - Extensible architecture for adding new metrics
// - MongoDB-based configuration and storage
// - Systemd integration for production deployment
//
// # Usage
// metrics-collector --mongodb <connection-string> --key <config-key>
//
// Example:
// metrics-collector --mongodb "mongodb://localhost:27017" --key "1111-1111"

use anyhow::{Context, Result};
use std::env;
use tracing::{error, info};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

// Module declarations
mod config;
mod metrics;
mod scheduler;
mod storage;

// Re-export for convenience
use config::ConfigManager;
use metrics::create_all_collectors;
use scheduler::MetricScheduler;
use storage::MetricStorage;

/// Application entry point
///
/// This function:
/// 1. Parses command-line arguments
/// 2. Initializes logging
/// 3. Connects to MongoDB and loads configuration
/// 4. Creates metric collectors and storage
/// 5. Starts the scheduler (runs forever)
#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging subsystem
    // Logs are written to stdout/stderr and can be captured by systemd
    init_logging();

    info!("=== Metrics Collector Starting ===");
    info!("Version: {}", env!("CARGO_PKG_VERSION"));

    // Parse command-line arguments
    let args = parse_arguments()?;

    info!("MongoDB Connection: {}", mask_credentials(&args.mongodb_uri));
    info!("Configuration Key: {}", args.config_key);

    // Connect to MongoDB and load configuration
    info!("Connecting to MongoDB...");
    let config_manager = ConfigManager::new(&args.mongodb_uri, Some(&args.database_name))
        .await
        .context("Failed to connect to MongoDB")?;

    info!("Loading monitoring settings...");
    let settings = config_manager
        .load_settings(&args.config_key)
        .await
        .context("Failed to load monitoring settings from MongoDB")?;

    // Create storage manager
    let storage = MetricStorage::new(
        config_manager.client(),
        config_manager.database_name(),
    );

    // Create all metric collectors
    let collectors = create_all_collectors();
    info!("Created {} metric collector(s)", collectors.len());

    // Optionally create indexes for better query performance
    if args.create_indexes {
        info!("Creating database indexes for metric collections...");
        for (metric_name, metric_settings) in &settings.metric_settings {
            info!("Creating indexes for collection: {}", metric_settings.collection);
            if let Err(e) = storage.create_indexes(&metric_settings.collection).await {
                error!(
                    "Failed to create indexes for {}: {}",
                    metric_name, e
                );
            }
        }
    }

    // Create and start the scheduler
    let scheduler = MetricScheduler::new(
        settings,
        storage,
        args.config_key.clone(),
    );

    info!("=== Metrics Collector Started Successfully ===");
    info!("Node ID: {}", args.config_key);
    info!("Press Ctrl+C to stop");

    // Start the scheduler (runs forever)
    // Each metric will be collected at its configured interval
    scheduler.start(collectors).await;

    // If we reach here, something went wrong
    error!("Scheduler stopped unexpectedly");
    Ok(())
}

/// Application configuration parsed from command-line arguments
struct AppConfig {
    /// MongoDB connection URI
    mongodb_uri: String,

    /// Database name (defaults to "monitoring")
    database_name: String,

    /// Configuration key to identify this node's settings
    config_key: String,

    /// Whether to create database indexes on startup
    create_indexes: bool,
}

/// Parses command-line arguments
///
/// # Arguments (in order)
/// 1. --mongodb <uri> - MongoDB connection string (required)
/// 2. --key <key> - Configuration key (required)
/// 3. --database <name> - Database name (optional, defaults to "monitoring")
/// 4. --create-indexes - Create indexes on startup (optional)
///
/// # Examples
/// ```bash
/// metrics-collector --mongodb "mongodb://localhost:27017" --key "1111-1111"
/// metrics-collector --mongodb "mongodb://user:pass@host:27017" --key "server-1" --database "prod_monitoring"
/// metrics-collector --mongodb "mongodb://localhost:27017" --key "1111-1111" --create-indexes
/// ```
///
/// # Returns
/// * `Ok(AppConfig)` - Successfully parsed configuration
/// * `Err(anyhow::Error)` - Invalid arguments
fn parse_arguments() -> Result<AppConfig> {
    let args: Vec<String> = env::args().collect();

    // Helper function to find argument value
    let find_arg = |flag: &str| -> Option<String> {
        args.iter()
            .position(|arg| arg == flag)
            .and_then(|pos| args.get(pos + 1))
            .map(|s| s.to_string())
    };

    // Check for required arguments
    let mongodb_uri = find_arg("--mongodb")
        .context("Missing required argument: --mongodb <connection-string>")?;

    let config_key = find_arg("--key")
        .context("Missing required argument: --key <config-key>")?;

    // Optional arguments
    let database_name = find_arg("--database").unwrap_or_else(|| "monitoring".to_string());
    let create_indexes = args.contains(&"--create-indexes".to_string());

    Ok(AppConfig {
        mongodb_uri,
        database_name,
        config_key,
        create_indexes,
    })
}

/// Initializes the logging subsystem
///
/// Sets up structured logging with:
/// - Timestamp for each log entry
/// - Log level (INFO, WARN, ERROR, etc.)
/// - Target module name
/// - Colored output when running in terminal
/// - JSON output when running as systemd service
///
/// # Log Levels
/// Default: INFO
/// Can be overridden with RUST_LOG environment variable
///
/// # Examples
/// ```bash
/// RUST_LOG=debug metrics-collector ...  # Enable debug logging
/// RUST_LOG=warn metrics-collector ...   # Only warnings and errors
/// ```
fn init_logging() {
    // Determine if we're running under systemd
    // Systemd sets INVOCATION_ID environment variable
    let is_systemd = env::var("INVOCATION_ID").is_ok();

    // Create env filter
    // Default to INFO level, but allow override via RUST_LOG
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));

    if is_systemd {
        // When running under systemd, use JSON format for structured logging
        // This makes logs easier to parse and analyze
        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt::layer().json())
            .init();
    } else {
        // When running in terminal, use human-readable format with colors
        tracing_subscriber::registry()
            .with(env_filter)
            .with(
                fmt::layer()
                    .with_target(true)
                    .with_level(true)
                    .with_ansi(true),
            )
            .init();
    }
}

/// Masks sensitive information in MongoDB connection strings
///
/// Hides passwords in connection URIs for security when logging.
///
/// # Example
/// ```
/// mongodb://user:password@host:27017
/// becomes
/// mongodb://user:****@host:27017
/// ```
fn mask_credentials(uri: &str) -> String {
    // Simple regex-free approach
    if let Some(at_pos) = uri.find('@') {
        if let Some(colon_pos) = uri[..at_pos].rfind(':') {
            let mut masked = uri.to_string();
            masked.replace_range(colon_pos + 1..at_pos, "****");
            return masked;
        }
    }
    uri.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mask_credentials() {
        let uri = "mongodb://user:password@localhost:27017";
        let masked = mask_credentials(uri);
        assert_eq!(masked, "mongodb://user:****@localhost:27017");

        let uri_no_auth = "mongodb://localhost:27017";
        let masked = mask_credentials(uri_no_auth);
        assert_eq!(masked, "mongodb://localhost:27017");
    }
}
