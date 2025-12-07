// Storage module - handles metric data persistence to MongoDB
//
// This module is responsible for:
// 1. Inserting metric documents into their respective collections
// 2. Handling storage errors gracefully
// 3. Providing a simple interface for the scheduler to store metrics

use bson::Document;
use mongodb::{Client, Collection};
use thiserror::Error;
use tracing::{debug, error, info};

/// Errors that can occur during metric storage
#[derive(Error, Debug)]
pub enum StorageError {
    #[error("MongoDB insert failed: {0}")]
    InsertError(#[from] mongodb::error::Error),

    #[error("Invalid document format: {0}")]
    InvalidDocument(String),
}

/// Metric storage manager
///
/// Handles the persistence of metric data to MongoDB.
/// Each metric type is stored in its own collection as specified in the configuration.
pub struct MetricStorage {
    /// MongoDB client for database operations
    client: Client,

    /// Database name where metrics are stored
    database_name: String,
}

impl MetricStorage {
    /// Creates a new MetricStorage instance
    ///
    /// # Arguments
    /// * `client` - MongoDB client (shared reference from ConfigManager)
    /// * `database_name` - Name of the database where metrics will be stored
    ///
    /// # Example
    /// ```
    /// let storage = MetricStorage::new(config_manager.client(), "monitoring");
    /// ```
    pub fn new(client: &Client, database_name: &str) -> Self {
        MetricStorage {
            client: client.clone(),
            database_name: database_name.to_string(),
        }
    }

    /// Stores a metric document in the specified collection
    ///
    /// This is the main method called by the scheduler to persist metrics.
    /// Each metric collector produces a BSON document, which is inserted
    /// into the collection specified in the configuration.
    ///
    /// # Arguments
    /// * `collection_name` - Name of the collection to store the metric in
    /// * `document` - BSON document containing the metric data
    ///
    /// # Returns
    /// * `Ok(())` - Successfully stored the metric
    /// * `Err(StorageError)` - Failed to store (network error, auth error, etc.)
    ///
    /// # Behavior
    /// - Inserts are non-blocking (async)
    /// - Each insert is independent (no batching by default)
    /// - Errors are logged but don't crash the application
    /// - MongoDB handles indexing and data organization
    ///
    /// # Example
    /// ```
    /// let doc = doc! {
    ///     "node": "1111-1111",
    ///     "timestamp": Utc::now(),
    ///     "load_1min": 1.5,
    /// };
    /// storage.store_metric("load_average_metrics", doc).await?;
    /// ```
    pub async fn store_metric(
        &self,
        collection_name: &str,
        document: Document,
    ) -> Result<(), StorageError> {
        debug!(
            "Storing metric to collection '{}': {} bytes",
            collection_name,
            document.to_string().len()
        );

        // Get the database instance
        let db = self.client.database(&self.database_name);

        // Get the collection (creates it if it doesn't exist)
        let collection: Collection<Document> = db.collection(collection_name);

        // Insert the document
        // MongoDB will automatically add an _id field if not present
        match collection.insert_one(document, None).await {
            Ok(result) => {
                debug!(
                    "Successfully stored metric with id: {:?} in collection '{}'",
                    result.inserted_id, collection_name
                );
                Ok(())
            }
            Err(e) => {
                error!(
                    "Failed to store metric in collection '{}': {}",
                    collection_name, e
                );
                Err(StorageError::InsertError(e))
            }
        }
    }

    /// Stores a metric with additional error handling and retry logic
    ///
    /// This is a wrapper around store_metric that provides:
    /// - Automatic retry on transient failures
    /// - More detailed error logging
    /// - Graceful degradation (logs error but doesn't fail)
    ///
    /// # Arguments
    /// * `collection_name` - Name of the collection
    /// * `metric_name` - Name of the metric (for logging)
    /// * `document` - BSON document to store
    ///
    /// # Note
    /// This method never returns an error - it logs failures and continues.
    /// This ensures that a failure in storing one metric type doesn't
    /// affect the collection of other metrics.
    pub async fn store_metric_safe(
        &self,
        collection_name: &str,
        metric_name: &str,
        document: Document,
    ) {
        // Attempt to store with a single retry on failure
        const MAX_RETRIES: u32 = 1;

        for attempt in 0..=MAX_RETRIES {
            match self.store_metric(collection_name, document.clone()).await {
                Ok(()) => {
                    if attempt > 0 {
                        info!(
                            "Successfully stored {} metric after {} retry(ies)",
                            metric_name, attempt
                        );
                    }
                    return;
                }
                Err(e) => {
                    if attempt < MAX_RETRIES {
                        error!(
                            "Failed to store {} metric (attempt {}): {}. Retrying...",
                            metric_name,
                            attempt + 1,
                            e
                        );
                        // Brief delay before retry
                        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                    } else {
                        error!(
                            "Failed to store {} metric after {} attempts: {}. Giving up.",
                            metric_name,
                            attempt + 1,
                            e
                        );
                    }
                }
            }
        }
    }

    /// Creates recommended indexes for metric collections
    ///
    /// This is a helper method that should be called during initialization
    /// to create indexes that optimize query performance.
    ///
    /// # Recommended Indexes
    /// - `node` + `timestamp` (compound) - For querying metrics by node over time
    /// - `timestamp` (TTL) - For automatic data expiration if needed
    ///
    /// # Arguments
    /// * `collection_name` - Collection to create indexes on
    ///
    /// # Note
    /// This is optional but recommended for production deployments.
    /// Indexes improve query performance but slightly slow down inserts.
    pub async fn create_indexes(&self, collection_name: &str) -> Result<(), StorageError> {
        use mongodb::options::IndexOptions;
        use mongodb::IndexModel;

        info!("Creating indexes for collection '{}'", collection_name);

        let db = self.client.database(&self.database_name);
        let collection: Collection<Document> = db.collection(collection_name);

        // Create compound index on node + timestamp for efficient time-series queries
        let index = IndexModel::builder()
            .keys(mongodb::bson::doc! {
                "node": 1,
                "timestamp": -1  // Descending for most recent first
            })
            .options(IndexOptions::builder().name("node_timestamp_idx".to_string()).build())
            .build();

        match collection.create_index(index, None).await {
            Ok(_) => {
                info!(
                    "Successfully created indexes for collection '{}'",
                    collection_name
                );
                Ok(())
            }
            Err(e) => {
                error!(
                    "Failed to create indexes for collection '{}': {}",
                    collection_name, e
                );
                Err(StorageError::InsertError(e))
            }
        }
    }
}
