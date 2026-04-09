// Configuration module - handles MongoDB connection and settings retrieval

use mongodb::{Client, Collection, Database};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{info, warn};

/// Errors that can occur during configuration loading
#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("MongoDB connection failed: {0}")]
    MongoConnectionError(#[from] mongodb::error::Error),

    #[error("Settings document not found for key: {0}")]
    SettingsNotFound(String),

    #[allow(dead_code)]
    #[error("Invalid settings format: {0}")]
    InvalidSettings(String),

    #[allow(dead_code)]
    #[error("Missing required setting: {0}")]
    MissingRequiredSetting(String),
}

/// Main configuration structure loaded from MongoDB
///
/// # Example MongoDB Document
/// ```json
/// {
///   "key": "0001-0001",
///   "collect_timeout": 5,
///   "collect_docker_timeout": 20,
///   "store_timeout": 60
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringSettings {
    /// Unique identifier for this configuration (e.g., "0001-0001")
    pub key: String,

    /// Collection interval in seconds for LoadAverage, Memory, DiskSpace
    pub collect_timeout: u64,

    /// Collection interval in seconds for DockerStats
    pub collect_docker_timeout: u64,

    /// How often (seconds) to flush the aggregated buffer to MongoDB
    pub store_timeout: u64,
}

/// Configuration manager for the monitoring application
pub struct ConfigManager {
    client: Client,
    database_name: String,
}

impl ConfigManager {
    /// Creates a new ConfigManager and establishes MongoDB connection
    pub async fn new(
        connection_string: &str,
        database_name: Option<&str>,
    ) -> Result<Self, ConfigError> {
        info!("Connecting to MongoDB at: {}", connection_string);

        let client = Client::with_uri_str(connection_string).await?;

        match client.list_database_names(None, None).await {
            Ok(_) => info!("Successfully connected to MongoDB"),
            Err(e) => {
                warn!("MongoDB connection verification failed: {}", e);
                return Err(ConfigError::MongoConnectionError(e));
            }
        }

        let database_name = database_name.unwrap_or("monitoring").to_string();

        Ok(ConfigManager {
            client,
            database_name,
        })
    }

    fn get_database(&self) -> Database {
        self.client.database(&self.database_name)
    }

    /// Fetches monitoring settings from MongoDB for a specific key (called at startup)
    pub async fn load_settings(&self, key: &str) -> Result<MonitoringSettings, ConfigError> {
        info!("Loading monitoring settings for key: {}", key);

        let settings = self.fetch_settings(key).await?;

        info!(
            "Settings loaded — collect: {}s, docker: {}s, store: {}s",
            settings.collect_timeout, settings.collect_docker_timeout, settings.store_timeout
        );

        Ok(settings)
    }

    /// Re-fetches monitoring settings from MongoDB (called after each flush)
    pub async fn reload_settings(&self, key: &str) -> Result<MonitoringSettings, ConfigError> {
        info!("Reloading monitoring settings for key: {}", key);

        let settings = self.fetch_settings(key).await?;

        info!(
            "Settings reloaded — collect: {}s, docker: {}s, store: {}s",
            settings.collect_timeout, settings.collect_docker_timeout, settings.store_timeout
        );

        Ok(settings)
    }

    async fn fetch_settings(&self, key: &str) -> Result<MonitoringSettings, ConfigError> {
        let db = self.get_database();
        let collection: Collection<MonitoringSettings> = db.collection("MonitoringSettings");
        let filter = mongodb::bson::doc! { "key": key };

        match collection.find_one(filter, None).await? {
            Some(settings) => Ok(settings),
            None => {
                warn!("No settings found for key: {}", key);
                Err(ConfigError::SettingsNotFound(key.to_string()))
            }
        }
    }

    pub fn client(&self) -> &Client {
        &self.client
    }

    pub fn database_name(&self) -> &str {
        &self.database_name
    }
}
