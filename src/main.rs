// Metrics Collector - Server Monitoring Tool
//
// Usage:
// metrics-collector --mongodb <connection-string> --key <config-key>
//
// Example:
// metrics-collector --mongodb "mongodb://localhost:27017" --key "0001-0001"

use anyhow::{Context, Result};
use std::env;
use tracing::{error, info};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

mod aggregator;
mod config;
mod metrics;
mod scheduler;
mod storage;

use config::ConfigManager;
use metrics::create_all_collectors;
use scheduler::MetricScheduler;
use storage::MetricStorage;

#[tokio::main]
async fn main() -> Result<()> {
    init_logging();

    info!("=== Metrics Collector Starting ===");
    info!("Version: {}", env!("CARGO_PKG_VERSION"));

    let args = parse_arguments()?;

    info!("MongoDB Connection: {}", mask_credentials(&args.mongodb_uri));
    info!("Configuration Key: {}", args.config_key);

    info!("Connecting to MongoDB...");
    let config_manager = ConfigManager::new(&args.mongodb_uri, Some(&args.database_name))
        .await
        .context("Failed to connect to MongoDB")?;

    info!("Loading monitoring settings...");
    let settings = config_manager
        .load_settings(&args.config_key)
        .await
        .context("Failed to load monitoring settings from MongoDB")?;

    // Storage shares the same MongoDB client
    let storage = MetricStorage::new(
        config_manager.client(),
        config_manager.database_name(),
    );

    let collectors = create_all_collectors();
    info!("Created {} metric collector(s)", collectors.len());

    if args.create_indexes {
        info!("Creating database indexes for metric collections...");
        let collections = [
            "load_average_metrics",
            "memory_metrics",
            "disk_metrics",
            "docker_metrics",
        ];
        for collection in &collections {
            info!("Creating indexes for collection: {}", collection);
            if let Err(e) = storage.create_indexes(collection).await {
                error!("Failed to create indexes for {}: {}", collection, e);
            }
        }
    }

    let scheduler = MetricScheduler::new(config_manager, storage, args.config_key.clone());

    info!("=== Metrics Collector Started Successfully ===");
    info!("Node ID: {}", args.config_key);
    info!("Press Ctrl+C to stop");

    scheduler.start(collectors, settings).await;

    error!("Scheduler stopped unexpectedly");
    Ok(())
}

struct AppConfig {
    mongodb_uri: String,
    database_name: String,
    config_key: String,
    create_indexes: bool,
}

fn parse_arguments() -> Result<AppConfig> {
    let args: Vec<String> = env::args().collect();

    let find_arg = |flag: &str| -> Option<String> {
        args.iter()
            .position(|arg| arg == flag)
            .and_then(|pos| args.get(pos + 1))
            .map(|s| s.to_string())
    };

    let mongodb_uri = find_arg("--mongodb")
        .context("Missing required argument: --mongodb <connection-string>")?;
    let config_key = find_arg("--key")
        .context("Missing required argument: --key <config-key>")?;
    let database_name = find_arg("--database").unwrap_or_else(|| "monitoring".to_string());
    let create_indexes = args.contains(&"--create-indexes".to_string());

    Ok(AppConfig {
        mongodb_uri,
        database_name,
        config_key,
        create_indexes,
    })
}

fn init_logging() {
    let is_systemd = env::var("INVOCATION_ID").is_ok();
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));

    if is_systemd {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt::layer().json())
            .init();
    } else {
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

fn mask_credentials(uri: &str) -> String {
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
