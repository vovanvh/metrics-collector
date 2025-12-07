// Configuration module - handles MongoDB connection and settings retrieval
//
// This module is responsible for:
// 1. Connecting to MongoDB using the provided connection string
// 2. Fetching monitoring settings from the MonitoringSettings collection
// 3. Parsing and validating the configuration
// 4. Providing strongly-typed access to settings

use mongodb::{Client, Collection, Database};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;
use tracing::{info, warn};

/// Errors that can occur during configuration loading
#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("MongoDB connection failed: {0}")]
    MongoConnectionError(#[from] mongodb::error::Error),

    #[error("Settings document not found for key: {0}")]
    SettingsNotFound(String),

    #[error("Invalid settings format: {0}")]
    InvalidSettings(String),

    #[error("Missing required setting: {0}")]
    MissingRequiredSetting(String),
}

/// Main configuration structure loaded from MongoDB
///
/// This structure represents a document in the MonitoringSettings collection.
/// Each node/server has its own configuration document identified by the key.
///
/// # Example MongoDB Document
/// ```json
/// {
///   "key": "1111-1111",
///   "metric_settings": {
///     "LoadAverage": {
///       "timeout": 5,
///       "collection": "load_average_metrics"
///     },
///     "Memory": {
///       "timeout": 10,
///       "collection": "memory_metrics"
///     },
///     "DiskSpace": {
///       "timeout": 30,
///       "collection": "disk_metrics"
///     },
///     "DockerStats": {
///       "timeout": 15,
///       "collection": "docker_metrics"
///     }
///   }
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringSettings {
    /// Unique identifier for this configuration (e.g., "1111-1111")
    /// Also used as the node identifier in metric documents
    pub key: String,

    /// Map of metric name to its specific settings
    /// Key: Metric name (e.g., "LoadAverage", "Memory")
    /// Value: Settings for that metric (timeout, collection name)
    pub metric_settings: HashMap<String, MetricSettings>,
}

/// Settings for an individual metric type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricSettings {
    /// Collection interval in seconds
    /// How often this metric should be collected and stored
    pub timeout: u64,

    /// Name of the MongoDB collection where this metric's data is stored
    /// Each metric type should have its own collection
    pub collection: String,
}

impl MonitoringSettings {
    /// Retrieves settings for a specific metric by name
    ///
    /// # Arguments
    /// * `metric_name` - Name of the metric (e.g., "LoadAverage")
    ///
    /// # Returns
    /// * `Some(MetricSettings)` - If settings exist for this metric
    /// * `None` - If no settings found (metric will be skipped)
    pub fn get_metric_settings(&self, metric_name: &str) -> Option<&MetricSettings> {
        self.metric_settings.get(metric_name)
    }
}

/// Configuration manager for the monitoring application
///
/// Handles MongoDB connection and settings retrieval.
/// This is the main entry point for configuration management.
pub struct ConfigManager {
    /// MongoDB client instance
    client: Client,

    /// Database name where MonitoringSettings collection resides
    database_name: String,
}

impl ConfigManager {
    /// Creates a new ConfigManager and establishes MongoDB connection
    ///
    /// # Arguments
    /// * `connection_string` - MongoDB connection URI (e.g., "mongodb://localhost:27017")
    /// * `database_name` - Name of the database to use (optional, defaults to "monitoring")
    ///
    /// # Returns
    /// * `Ok(ConfigManager)` - Successfully connected to MongoDB
    /// * `Err(ConfigError)` - Connection failed
    ///
    /// # Example
    /// ```
    /// let config_manager = ConfigManager::new(
    ///     "mongodb://localhost:27017",
    ///     Some("monitoring")
    /// ).await?;
    /// ```
    pub async fn new(
        connection_string: &str,
        database_name: Option<&str>,
    ) -> Result<Self, ConfigError> {
        info!("Connecting to MongoDB at: {}", connection_string);

        // Establish MongoDB connection
        // The connection is validated by attempting to ping the server
        let client = Client::with_uri_str(connection_string).await?;

        // Verify connection by listing databases (lightweight operation)
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

    /// Retrieves the MongoDB database instance
    fn get_database(&self) -> Database {
        self.client.database(&self.database_name)
    }

    /// Fetches monitoring settings from MongoDB for a specific key
    ///
    /// Queries the MonitoringSettings collection for a document matching the provided key.
    /// This method is called once at startup to load the configuration.
    ///
    /// # Arguments
    /// * `key` - The configuration key (e.g., "1111-1111")
    ///
    /// # Returns
    /// * `Ok(MonitoringSettings)` - Successfully loaded settings
    /// * `Err(ConfigError)` - Settings not found or invalid
    ///
    /// # MongoDB Query
    /// Executes: `db.MonitoringSettings.findOne({ key: "<key>" })`
    pub async fn load_settings(&self, key: &str) -> Result<MonitoringSettings, ConfigError> {
        info!("Loading monitoring settings for key: {}", key);

        let db = self.get_database();

        // Access the MonitoringSettings collection
        let collection: Collection<MonitoringSettings> = db.collection("MonitoringSettings");

        // Query for document matching the provided key
        let filter = mongodb::bson::doc! { "key": key };

        match collection.find_one(filter, None).await? {
            Some(settings) => {
                info!(
                    "Successfully loaded settings with {} metric configurations",
                    settings.metric_settings.len()
                );

                // Log each metric's configuration for visibility
                for (metric_name, metric_config) in &settings.metric_settings {
                    info!(
                        "  {} - Collection: '{}', Interval: {}s",
                        metric_name, metric_config.collection, metric_config.timeout
                    );
                }

                Ok(settings)
            }
            None => {
                warn!("No settings found for key: {}", key);
                Err(ConfigError::SettingsNotFound(key.to_string()))
            }
        }
    }

    /// Returns a reference to the MongoDB client
    ///
    /// Used by the storage module to access MongoDB for writing metrics
    pub fn client(&self) -> &Client {
        &self.client
    }

    /// Returns the database name
    pub fn database_name(&self) -> &str {
        &self.database_name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Example of creating a test configuration programmatically
    #[test]
    fn test_settings_structure() {
        let mut metric_settings = HashMap::new();

        metric_settings.insert(
            "LoadAverage".to_string(),
            MetricSettings {
                timeout: 5,
                collection: "load_average_metrics".to_string(),
            },
        );

        let settings = MonitoringSettings {
            key: "test-key".to_string(),
            metric_settings,
        };

        assert_eq!(settings.key, "test-key");
        assert!(settings.get_metric_settings("LoadAverage").is_some());
        assert!(settings.get_metric_settings("NonExistent").is_none());
    }
}
