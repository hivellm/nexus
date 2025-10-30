//! Bulk data loading module for fast initial data loading

use crate::catalog::Catalog;
use crate::error::{Error, Result};
use crate::index::IndexManager;
use crate::storage::RecordStore;
use crate::transaction::TransactionManager;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Bulk loader for fast data loading
pub struct BulkLoader {
    /// Catalog for metadata management
    catalog: Arc<Catalog>,
    /// Storage for data persistence
    storage: Arc<RwLock<RecordStore>>,
    /// Index manager for index updates
    indexes: Arc<IndexManager>,
    /// Transaction manager
    transaction_manager: Arc<RwLock<TransactionManager>>,
    /// Loading statistics
    stats: Arc<RwLock<LoadingStats>>,
    /// Batch size for processing
    batch_size: usize,
    /// Parallel workers
    worker_count: usize,
}

/// Loading statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadingStats {
    /// Total nodes loaded
    pub nodes_loaded: u64,
    /// Total relationships loaded
    pub relationships_loaded: u64,
    /// Loading start time
    pub start_time: chrono::DateTime<chrono::Utc>,
    /// Loading end time
    pub end_time: Option<chrono::DateTime<chrono::Utc>>,
    /// Loading duration in seconds
    pub duration_seconds: Option<f64>,
    /// Errors encountered
    pub errors: Vec<String>,
    /// Warnings encountered
    pub warnings: Vec<String>,
}

/// Bulk loading configuration
#[derive(Debug, Clone)]
pub struct BulkLoadConfig {
    /// Batch size for processing
    pub batch_size: usize,
    /// Number of parallel workers
    pub worker_count: usize,
    /// Enable progress reporting
    pub enable_progress: bool,
    /// Progress reporting interval (records)
    pub progress_interval: u64,
    /// Enable index updates during loading
    pub enable_index_updates: bool,
    /// Enable transaction batching
    pub enable_transaction_batching: bool,
    /// Transaction batch size
    pub transaction_batch_size: usize,
}

impl Default for BulkLoadConfig {
    fn default() -> Self {
        Self {
            batch_size: 10000,
            worker_count: 4,
            enable_progress: true,
            progress_interval: 10000,
            enable_index_updates: true,
            enable_transaction_batching: true,
            transaction_batch_size: 1000,
        }
    }
}

/// Data source for bulk loading
#[derive(Debug, Clone)]
pub enum DataSource {
    /// JSON file containing nodes and relationships
    JsonFile { path: String },
    /// CSV files for nodes and relationships
    CsvFiles {
        nodes_path: String,
        relationships_path: String,
    },
    /// In-memory data
    InMemory {
        nodes: Vec<NodeData>,
        relationships: Vec<RelationshipData>,
    },
}

/// Node data for bulk loading
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeData {
    /// Node ID (optional, will be generated if not provided)
    pub id: Option<u64>,
    /// Node labels
    pub labels: Vec<String>,
    /// Node properties
    pub properties: HashMap<String, serde_json::Value>,
}

/// Relationship data for bulk loading
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationshipData {
    /// Relationship ID (optional, will be generated if not provided)
    pub id: Option<u64>,
    /// Source node ID
    pub source_id: u64,
    /// Target node ID
    pub target_id: u64,
    /// Relationship type
    pub rel_type: String,
    /// Relationship properties
    pub properties: HashMap<String, serde_json::Value>,
}

/// Loading progress callback
pub type ProgressCallback = Box<dyn Fn(LoadingProgress) + Send + Sync>;

/// Loading progress information
#[derive(Debug, Clone)]
pub struct LoadingProgress {
    /// Current progress (0.0 to 1.0)
    pub progress: f64,
    /// Nodes processed
    pub nodes_processed: u64,
    /// Relationships processed
    pub relationships_processed: u64,
    /// Current phase
    pub phase: LoadingPhase,
    /// Estimated time remaining (seconds)
    pub estimated_remaining_seconds: Option<f64>,
}

/// Loading phases
#[derive(Debug, Clone, PartialEq)]
pub enum LoadingPhase {
    /// Initializing
    Initializing,
    /// Loading nodes
    LoadingNodes,
    /// Loading relationships
    LoadingRelationships,
    /// Updating indexes
    UpdatingIndexes,
    /// Finalizing
    Finalizing,
    /// Completed
    Completed,
}

impl BulkLoader {
    /// Create a new bulk loader
    pub fn new(
        catalog: Arc<Catalog>,
        storage: Arc<RwLock<RecordStore>>,
        indexes: Arc<IndexManager>,
        transaction_manager: Arc<RwLock<TransactionManager>>,
        config: BulkLoadConfig,
    ) -> Self {
        Self {
            catalog,
            storage,
            indexes,
            transaction_manager,
            stats: Arc::new(RwLock::new(LoadingStats {
                nodes_loaded: 0,
                relationships_loaded: 0,
                start_time: chrono::Utc::now(),
                end_time: None,
                duration_seconds: None,
                errors: Vec::new(),
                warnings: Vec::new(),
            })),
            batch_size: config.batch_size,
            worker_count: config.worker_count,
        }
    }

    /// Load data from a data source
    pub async fn load_data(
        &self,
        source: DataSource,
        progress_callback: Option<ProgressCallback>,
    ) -> Result<LoadingStats> {
        let start_time = chrono::Utc::now();

        // Update stats
        {
            let mut stats = self.stats.write().await;
            stats.start_time = start_time;
            stats.nodes_loaded = 0;
            stats.relationships_loaded = 0;
            stats.errors.clear();
            stats.warnings.clear();
        }

        // Report initial progress
        if let Some(callback) = &progress_callback {
            callback(LoadingProgress {
                progress: 0.0,
                nodes_processed: 0,
                relationships_processed: 0,
                phase: LoadingPhase::Initializing,
                estimated_remaining_seconds: None,
            });
        }

        // Load data based on source type
        match source {
            DataSource::JsonFile { path } => {
                self.load_from_json_file(path, progress_callback.as_ref())
                    .await?;
            }
            DataSource::CsvFiles {
                nodes_path,
                relationships_path,
            } => {
                self.load_from_csv_files(
                    nodes_path,
                    relationships_path,
                    progress_callback.as_ref(),
                )
                .await?;
            }
            DataSource::InMemory {
                nodes,
                relationships,
            } => {
                self.load_from_memory(nodes, relationships, progress_callback.as_ref())
                    .await?;
            }
        }

        // Update final stats
        let end_time = chrono::Utc::now();
        {
            let mut stats = self.stats.write().await;
            stats.end_time = Some(end_time);
            stats.duration_seconds =
                Some((end_time - start_time).num_milliseconds() as f64 / 1000.0);
        }

        // Report completion
        if let Some(callback) = &progress_callback {
            callback(LoadingProgress {
                progress: 1.0,
                nodes_processed: self.stats.read().await.nodes_loaded,
                relationships_processed: self.stats.read().await.relationships_loaded,
                phase: LoadingPhase::Completed,
                estimated_remaining_seconds: Some(0.0),
            });
        }

        Ok(self.stats.read().await.clone())
    }

    /// Load data from JSON file
    async fn load_from_json_file(
        &self,
        path: String,
        progress_callback: Option<&ProgressCallback>,
    ) -> Result<()> {
        let file_path = Path::new(&path);
        if !file_path.exists() {
            return Err(Error::internal(format!("File not found: {}", path)));
        }

        let content = std::fs::read_to_string(file_path)?;
        let data: serde_json::Value = serde_json::from_str(&content)?;

        let nodes = if let Some(nodes_array) = data.get("nodes").and_then(|v| v.as_array()) {
            nodes_array
                .iter()
                .filter_map(|node| serde_json::from_value::<NodeData>(node.clone()).ok())
                .collect()
        } else {
            Vec::new()
        };

        let relationships =
            if let Some(rels_array) = data.get("relationships").and_then(|v| v.as_array()) {
                rels_array
                    .iter()
                    .filter_map(|rel| serde_json::from_value::<RelationshipData>(rel.clone()).ok())
                    .collect()
            } else {
                Vec::new()
            };

        self.load_from_memory(nodes, relationships, progress_callback)
            .await
    }

    /// Load data from CSV files
    async fn load_from_csv_files(
        &self,
        nodes_path: String,
        relationships_path: String,
        progress_callback: Option<&ProgressCallback>,
    ) -> Result<()> {
        // Load nodes from CSV
        let nodes = self.load_nodes_from_csv(nodes_path).await?;

        // Load relationships from CSV
        let relationships = self.load_relationships_from_csv(relationships_path).await?;

        self.load_from_memory(nodes, relationships, progress_callback)
            .await
    }

    /// Load nodes from CSV file
    async fn load_nodes_from_csv(&self, path: String) -> Result<Vec<NodeData>> {
        let file_path = Path::new(&path);
        if !file_path.exists() {
            return Err(Error::internal(format!("File not found: {}", path)));
        }

        let content = std::fs::read_to_string(file_path)?;
        let mut nodes = Vec::new();
        let mut lines = content.lines();

        // Skip header
        if let Some(header) = lines.next() {
            let columns: Vec<&str> = header.split(',').collect();

            for line in lines {
                if line.trim().is_empty() {
                    continue;
                }

                let values: Vec<&str> = line.split(',').collect();
                if values.len() != columns.len() {
                    continue; // Skip malformed lines
                }

                let mut properties = HashMap::new();
                for (i, value) in values.iter().enumerate() {
                    if i < columns.len() {
                        properties.insert(
                            columns[i].to_string(),
                            serde_json::Value::String(value.to_string()),
                        );
                    }
                }

                nodes.push(NodeData {
                    id: None,                         // Will be generated
                    labels: vec!["Node".to_string()], // Default label
                    properties,
                });
            }
        }

        Ok(nodes)
    }

    /// Load relationships from CSV file
    async fn load_relationships_from_csv(&self, path: String) -> Result<Vec<RelationshipData>> {
        let file_path = Path::new(&path);
        if !file_path.exists() {
            return Err(Error::internal(format!("File not found: {}", path)));
        }

        let content = std::fs::read_to_string(file_path)?;
        let mut relationships = Vec::new();
        let mut lines = content.lines();

        // Skip header
        if let Some(header) = lines.next() {
            let columns: Vec<&str> = header.split(',').collect();

            for line in lines {
                if line.trim().is_empty() {
                    continue;
                }

                let values: Vec<&str> = line.split(',').collect();
                if values.len() < 3 {
                    continue; // Need at least source, target, type
                }

                let source_id = values[0].parse::<u64>().unwrap_or(0);
                let target_id = values[1].parse::<u64>().unwrap_or(0);
                let rel_type = values[2].to_string();

                let mut properties = HashMap::new();
                for (i, value) in values.iter().enumerate().skip(3) {
                    if i < columns.len() {
                        properties.insert(
                            columns[i].to_string(),
                            serde_json::Value::String(value.to_string()),
                        );
                    }
                }

                relationships.push(RelationshipData {
                    id: None, // Will be generated
                    source_id,
                    target_id,
                    rel_type,
                    properties,
                });
            }
        }

        Ok(relationships)
    }

    /// Load data from memory
    async fn load_from_memory(
        &self,
        nodes: Vec<NodeData>,
        relationships: Vec<RelationshipData>,
        progress_callback: Option<&ProgressCallback>,
    ) -> Result<()> {
        let total_nodes = nodes.len() as u64;
        let total_relationships = relationships.len() as u64;
        let total_items = total_nodes + total_relationships;

        // Load nodes in batches
        if !nodes.is_empty() {
            if let Some(callback) = &progress_callback {
                callback(LoadingProgress {
                    progress: 0.0,
                    nodes_processed: 0,
                    relationships_processed: 0,
                    phase: LoadingPhase::LoadingNodes,
                    estimated_remaining_seconds: None,
                });
            }

            for (i, batch) in nodes.chunks(self.batch_size).enumerate() {
                self.load_node_batch(batch).await?;

                let processed = ((i + 1) * self.batch_size).min(nodes.len()) as u64;
                let progress = processed as f64 / total_items as f64;

                if let Some(callback) = &progress_callback {
                    callback(LoadingProgress {
                        progress,
                        nodes_processed: processed,
                        relationships_processed: 0,
                        phase: LoadingPhase::LoadingNodes,
                        estimated_remaining_seconds: self
                            .estimate_remaining_time(processed, total_items),
                    });
                }
            }
        }

        // Load relationships in batches
        if !relationships.is_empty() {
            if let Some(callback) = &progress_callback {
                callback(LoadingProgress {
                    progress: total_nodes as f64 / total_items as f64,
                    nodes_processed: total_nodes,
                    relationships_processed: 0,
                    phase: LoadingPhase::LoadingRelationships,
                    estimated_remaining_seconds: None,
                });
            }

            for (i, batch) in relationships.chunks(self.batch_size).enumerate() {
                self.load_relationship_batch(batch).await?;

                let processed = ((i + 1) * self.batch_size).min(relationships.len()) as u64;
                let progress = (total_nodes + processed) as f64 / total_items as f64;

                if let Some(callback) = &progress_callback {
                    callback(LoadingProgress {
                        progress,
                        nodes_processed: total_nodes,
                        relationships_processed: processed,
                        phase: LoadingPhase::LoadingRelationships,
                        estimated_remaining_seconds: self
                            .estimate_remaining_time(total_nodes + processed, total_items),
                    });
                }
            }
        }

        // Update indexes
        if let Some(callback) = &progress_callback {
            callback(LoadingProgress {
                progress: 0.9,
                nodes_processed: total_nodes,
                relationships_processed: total_relationships,
                phase: LoadingPhase::UpdatingIndexes,
                estimated_remaining_seconds: None,
            });
        }

        // Finalize
        if let Some(callback) = &progress_callback {
            callback(LoadingProgress {
                progress: 1.0,
                nodes_processed: total_nodes,
                relationships_processed: total_relationships,
                phase: LoadingPhase::Finalizing,
                estimated_remaining_seconds: Some(0.0),
            });
        }

        Ok(())
    }

    /// Load a batch of nodes
    async fn load_node_batch(&self, batch: &[NodeData]) -> Result<()> {
        let mut tx_manager = self.transaction_manager.write().await;
        let mut tx = tx_manager.begin_write()?;

        for node_data in batch {
            // Create node using storage directly
            let labels = node_data.labels.clone();
            let properties = serde_json::Value::Object(
                node_data
                    .properties
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect(),
            );

            // Store node
            let mut storage = self.storage.write().await;
            let node_id = storage.create_node(&mut tx, labels.clone(), properties)?;
            drop(storage);

            // Update indexes
            for label in &node_data.labels {
                let label_id = self.catalog.get_or_create_label(label)?;
                self.indexes.add_node_to_label(node_id, label_id)?;
            }

            // Update stats
            {
                let mut stats = self.stats.write().await;
                stats.nodes_loaded += 1;
            }
        }

        tx_manager.commit(&mut tx)?;
        Ok(())
    }

    /// Load a batch of relationships
    async fn load_relationship_batch(&self, batch: &[RelationshipData]) -> Result<()> {
        let mut tx_manager = self.transaction_manager.write().await;
        let mut tx = tx_manager.begin_write()?;

        for rel_data in batch {
            // Create relationship using storage directly
            let properties = serde_json::Value::Object(
                rel_data
                    .properties
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect(),
            );

            // Store relationship
            let mut storage = self.storage.write().await;
            let type_id = self.catalog.get_or_create_type(&rel_data.rel_type)?;
            let _rel_id = storage.create_relationship(
                &mut tx,
                rel_data.source_id,
                rel_data.target_id,
                type_id,
                properties,
            )?;
            drop(storage);

            self.catalog.increment_rel_count(type_id)?;

            // Update stats
            {
                let mut stats = self.stats.write().await;
                stats.relationships_loaded += 1;
            }
        }

        tx_manager.commit(&mut tx)?;
        Ok(())
    }

    /// Estimate remaining time
    fn estimate_remaining_time(&self, processed: u64, total: u64) -> Option<f64> {
        if processed == 0 {
            return None;
        }

        let stats = self.stats.try_read().ok()?;
        let elapsed = (chrono::Utc::now() - stats.start_time).num_seconds() as f64;
        let rate = processed as f64 / elapsed;
        let remaining = (total - processed) as f64 / rate;

        Some(remaining)
    }

    /// Get current loading statistics
    pub async fn get_stats(&self) -> LoadingStats {
        self.stats.read().await.clone()
    }

    /// Clear loading statistics
    pub async fn clear_stats(&self) {
        let mut stats = self.stats.write().await;
        *stats = LoadingStats {
            nodes_loaded: 0,
            relationships_loaded: 0,
            start_time: chrono::Utc::now(),
            end_time: None,
            duration_seconds: None,
            errors: Vec::new(),
            warnings: Vec::new(),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_bulk_loader_creation() {
        let temp_dir = TempDir::new().unwrap();
        let catalog = Arc::new(Catalog::new(temp_dir.path()).unwrap());
        let storage = Arc::new(RwLock::new(RecordStore::new(temp_dir.path()).unwrap()));
        let indexes = Arc::new(IndexManager::new(temp_dir.path().join("indexes")).unwrap());
        let transaction_manager = Arc::new(RwLock::new(TransactionManager::new().unwrap()));

        let config = BulkLoadConfig::default();
        let loader = BulkLoader::new(catalog, storage, indexes, transaction_manager, config);

        let stats = loader.get_stats().await;
        assert_eq!(stats.nodes_loaded, 0);
        assert_eq!(stats.relationships_loaded, 0);
    }

    #[tokio::test]
    async fn test_load_from_memory() {
        let temp_dir = TempDir::new().unwrap();
        let catalog = Arc::new(Catalog::new(temp_dir.path()).unwrap());
        let storage = Arc::new(RwLock::new(RecordStore::new(temp_dir.path()).unwrap()));
        let indexes = Arc::new(IndexManager::new(temp_dir.path().join("indexes")).unwrap());
        let transaction_manager = Arc::new(RwLock::new(TransactionManager::new().unwrap()));

        let config = BulkLoadConfig {
            batch_size: 2,
            worker_count: 1,
            enable_progress: false,
            progress_interval: 1000,
            enable_index_updates: true,
            enable_transaction_batching: true,
            transaction_batch_size: 10,
        };

        let loader = BulkLoader::new(catalog, storage, indexes, transaction_manager, config);

        let mut properties = HashMap::new();
        properties.insert(
            "name".to_string(),
            serde_json::Value::String("Alice".to_string()),
        );

        let nodes = vec![
            NodeData {
                id: None,
                labels: vec!["Person".to_string()],
                properties: properties.clone(),
            },
            NodeData {
                id: None,
                labels: vec!["Person".to_string()],
                properties,
            },
        ];

        let relationships = vec![RelationshipData {
            id: None,
            source_id: 1,
            target_id: 2,
            rel_type: "KNOWS".to_string(),
            properties: HashMap::new(),
        }];

        let source = DataSource::InMemory {
            nodes,
            relationships,
        };
        let result = loader.load_data(source, None).await.unwrap();

        assert_eq!(result.nodes_loaded, 2);
        assert_eq!(result.relationships_loaded, 1);
        assert!(result.duration_seconds.is_some());
    }

    #[tokio::test]
    async fn test_bulk_load_config_default() {
        let config = BulkLoadConfig::default();
        assert_eq!(config.batch_size, 10000);
        assert_eq!(config.worker_count, 4);
        assert!(config.enable_progress);
        assert_eq!(config.progress_interval, 10000);
        assert!(config.enable_index_updates);
        assert!(config.enable_transaction_batching);
        assert_eq!(config.transaction_batch_size, 1000);
    }

    #[tokio::test]
    async fn test_loading_stats() {
        let stats = LoadingStats {
            nodes_loaded: 100,
            relationships_loaded: 50,
            start_time: chrono::Utc::now(),
            end_time: Some(chrono::Utc::now()),
            duration_seconds: Some(1.5),
            errors: vec!["Test error".to_string()],
            warnings: vec!["Test warning".to_string()],
        };

        assert_eq!(stats.nodes_loaded, 100);
        assert_eq!(stats.relationships_loaded, 50);
        assert!(stats.duration_seconds.is_some());
        assert_eq!(stats.errors.len(), 1);
        assert_eq!(stats.warnings.len(), 1);
    }

    #[tokio::test]
    async fn test_node_data() {
        let mut properties = HashMap::new();
        properties.insert("age".to_string(), serde_json::Value::Number(25.into()));

        let node = NodeData {
            id: Some(1),
            labels: vec!["Person".to_string(), "Employee".to_string()],
            properties,
        };

        assert_eq!(node.id, Some(1));
        assert_eq!(node.labels.len(), 2);
        assert!(node.labels.contains(&"Person".to_string()));
        assert!(node.labels.contains(&"Employee".to_string()));
    }

    #[tokio::test]
    async fn test_relationship_data() {
        let mut properties = HashMap::new();
        properties.insert(
            "since".to_string(),
            serde_json::Value::String("2020".to_string()),
        );

        let rel = RelationshipData {
            id: Some(1),
            source_id: 1,
            target_id: 2,
            rel_type: "WORKS_AT".to_string(),
            properties,
        };

        assert_eq!(rel.id, Some(1));
        assert_eq!(rel.source_id, 1);
        assert_eq!(rel.target_id, 2);
        assert_eq!(rel.rel_type, "WORKS_AT");
    }

    #[tokio::test]
    async fn test_loading_phases() {
        assert_eq!(LoadingPhase::Initializing, LoadingPhase::Initializing);
        assert_ne!(
            LoadingPhase::LoadingNodes,
            LoadingPhase::LoadingRelationships
        );
    }

    #[tokio::test]
    async fn test_bulk_load_config_custom() {
        let config = BulkLoadConfig {
            batch_size: 5000,
            worker_count: 2,
            enable_progress: false,
            progress_interval: 5000,
            enable_index_updates: false,
            enable_transaction_batching: false,
            transaction_batch_size: 500,
        };

        assert_eq!(config.batch_size, 5000);
        assert_eq!(config.worker_count, 2);
        assert!(!config.enable_progress);
        assert_eq!(config.progress_interval, 5000);
        assert!(!config.enable_index_updates);
        assert!(!config.enable_transaction_batching);
        assert_eq!(config.transaction_batch_size, 500);
    }

    #[tokio::test]
    async fn test_loading_stats_operations() {
        let mut stats = LoadingStats {
            nodes_loaded: 0,
            relationships_loaded: 0,
            start_time: chrono::Utc::now(),
            end_time: None,
            duration_seconds: None,
            errors: Vec::new(),
            warnings: Vec::new(),
        };

        // Test updating stats
        stats.nodes_loaded = 1000;
        stats.relationships_loaded = 500;
        stats.end_time = Some(chrono::Utc::now());
        stats.duration_seconds = Some(2.5);
        stats.errors.push("Test error 1".to_string());
        stats.errors.push("Test error 2".to_string());
        stats.warnings.push("Test warning 1".to_string());

        assert_eq!(stats.nodes_loaded, 1000);
        assert_eq!(stats.relationships_loaded, 500);
        assert!(stats.end_time.is_some());
        assert_eq!(stats.duration_seconds, Some(2.5));
        assert_eq!(stats.errors.len(), 2);
        assert_eq!(stats.warnings.len(), 1);
        assert!(stats.errors.contains(&"Test error 1".to_string()));
        assert!(stats.warnings.contains(&"Test warning 1".to_string()));
    }

    #[tokio::test]
    async fn test_node_data_operations() {
        let mut properties = HashMap::new();
        properties.insert(
            "name".to_string(),
            serde_json::Value::String("Alice".to_string()),
        );
        properties.insert("age".to_string(), serde_json::Value::Number(25.into()));
        properties.insert("active".to_string(), serde_json::Value::Bool(true));

        let mut node = NodeData {
            id: None,
            labels: vec!["Person".to_string()],
            properties: properties.clone(),
        };

        // Test updating node data
        node.id = Some(1);
        node.labels.push("Employee".to_string());
        node.properties.insert(
            "salary".to_string(),
            serde_json::Value::Number(50000.into()),
        );

        assert_eq!(node.id, Some(1));
        assert_eq!(node.labels.len(), 2);
        assert!(node.labels.contains(&"Person".to_string()));
        assert!(node.labels.contains(&"Employee".to_string()));
        assert_eq!(node.properties.len(), 4);
        assert!(node.properties.contains_key("name"));
        assert!(node.properties.contains_key("age"));
        assert!(node.properties.contains_key("active"));
        assert!(node.properties.contains_key("salary"));
    }

    #[tokio::test]
    async fn test_relationship_data_operations() {
        let mut properties = HashMap::new();
        properties.insert(
            "since".to_string(),
            serde_json::Value::String("2020".to_string()),
        );
        properties.insert(
            "strength".to_string(),
            serde_json::Value::Number(serde_json::Number::from_f64(0.8).unwrap()),
        );

        let mut rel = RelationshipData {
            id: None,
            source_id: 1,
            target_id: 2,
            rel_type: "KNOWS".to_string(),
            properties: properties.clone(),
        };

        // Test updating relationship data
        rel.id = Some(1);
        rel.source_id = 10;
        rel.target_id = 20;
        rel.rel_type = "WORKS_WITH".to_string();
        rel.properties.insert(
            "department".to_string(),
            serde_json::Value::String("Engineering".to_string()),
        );

        assert_eq!(rel.id, Some(1));
        assert_eq!(rel.source_id, 10);
        assert_eq!(rel.target_id, 20);
        assert_eq!(rel.rel_type, "WORKS_WITH");
        assert_eq!(rel.properties.len(), 3);
        assert!(rel.properties.contains_key("since"));
        assert!(rel.properties.contains_key("strength"));
        assert!(rel.properties.contains_key("department"));
    }

    #[tokio::test]
    async fn test_data_source_serialization() {
        let nodes = vec![NodeData {
            id: Some(1),
            labels: vec!["Person".to_string()],
            properties: HashMap::new(),
        }];
        let relationships = vec![RelationshipData {
            id: Some(1),
            source_id: 1,
            target_id: 2,
            rel_type: "KNOWS".to_string(),
            properties: HashMap::new(),
        }];

        let source = DataSource::InMemory {
            nodes,
            relationships,
        };

        // Test that DataSource can be created and accessed
        match source {
            DataSource::InMemory {
                nodes,
                relationships,
            } => {
                assert_eq!(nodes.len(), 1);
                assert_eq!(relationships.len(), 1);
                assert_eq!(nodes[0].id, Some(1));
                assert_eq!(relationships[0].rel_type, "KNOWS");
            }
            _ => panic!("Expected InMemory variant"),
        }
    }

    #[tokio::test]
    async fn test_loading_phase_variants() {
        let phases = [
            LoadingPhase::Initializing,
            LoadingPhase::LoadingNodes,
            LoadingPhase::LoadingRelationships,
            LoadingPhase::UpdatingIndexes,
            LoadingPhase::Finalizing,
            LoadingPhase::Completed,
        ];

        // Test that all phases can be created and compared
        for (i, phase1) in phases.iter().enumerate() {
            for (j, phase2) in phases.iter().enumerate() {
                if i == j {
                    assert_eq!(phase1, phase2);
                } else {
                    assert_ne!(phase1, phase2);
                }
            }
        }
    }

    #[tokio::test]
    async fn test_bulk_loader_with_custom_config() {
        let temp_dir = TempDir::new().unwrap();
        let catalog = Arc::new(Catalog::new(temp_dir.path()).unwrap());
        let storage = Arc::new(RwLock::new(RecordStore::new(temp_dir.path()).unwrap()));
        let indexes = Arc::new(IndexManager::new(temp_dir.path().join("indexes")).unwrap());
        let transaction_manager = Arc::new(RwLock::new(TransactionManager::new().unwrap()));

        let config = BulkLoadConfig {
            batch_size: 100,
            worker_count: 2,
            enable_progress: true,
            progress_interval: 50,
            enable_index_updates: true,
            enable_transaction_batching: true,
            transaction_batch_size: 25,
        };

        let loader = BulkLoader::new(catalog, storage, indexes, transaction_manager, config);

        // Test that the loader was created with the correct configuration
        assert_eq!(loader.batch_size, 100);
        assert_eq!(loader.worker_count, 2);
    }

    #[tokio::test]
    async fn test_loading_stats_duration_calculation() {
        let start_time = chrono::Utc::now();
        let end_time = start_time + chrono::Duration::seconds(5);

        let stats = LoadingStats {
            nodes_loaded: 1000,
            relationships_loaded: 500,
            start_time,
            end_time: Some(end_time),
            duration_seconds: Some(5.0),
            errors: Vec::new(),
            warnings: Vec::new(),
        };

        assert_eq!(stats.duration_seconds, Some(5.0));
        assert!(stats.end_time.is_some());
        assert!(stats.end_time.unwrap() > stats.start_time);
    }

    #[tokio::test]
    async fn test_node_data_with_multiple_labels() {
        let mut properties = HashMap::new();
        properties.insert(
            "name".to_string(),
            serde_json::Value::String("John Doe".to_string()),
        );

        let node = NodeData {
            id: Some(42),
            labels: vec![
                "Person".to_string(),
                "Employee".to_string(),
                "Manager".to_string(),
                "Developer".to_string(),
            ],
            properties,
        };

        assert_eq!(node.id, Some(42));
        assert_eq!(node.labels.len(), 4);
        assert!(node.labels.contains(&"Person".to_string()));
        assert!(node.labels.contains(&"Employee".to_string()));
        assert!(node.labels.contains(&"Manager".to_string()));
        assert!(node.labels.contains(&"Developer".to_string()));
    }

    #[tokio::test]
    async fn test_relationship_data_with_complex_properties() {
        let mut properties = HashMap::new();
        properties.insert(
            "start_date".to_string(),
            serde_json::Value::String("2020-01-01".to_string()),
        );
        properties.insert(
            "end_date".to_string(),
            serde_json::Value::String("2023-12-31".to_string()),
        );
        properties.insert(
            "salary".to_string(),
            serde_json::Value::Number(75000.into()),
        );
        properties.insert("is_active".to_string(), serde_json::Value::Bool(true));
        properties.insert("bonus".to_string(), serde_json::Value::Number(5000.into()));

        let rel = RelationshipData {
            id: Some(100),
            source_id: 1,
            target_id: 2,
            rel_type: "EMPLOYED_BY".to_string(),
            properties,
        };

        assert_eq!(rel.id, Some(100));
        assert_eq!(rel.source_id, 1);
        assert_eq!(rel.target_id, 2);
        assert_eq!(rel.rel_type, "EMPLOYED_BY");
        assert_eq!(rel.properties.len(), 5);
        assert!(rel.properties.contains_key("start_date"));
        assert!(rel.properties.contains_key("end_date"));
        assert!(rel.properties.contains_key("salary"));
        assert!(rel.properties.contains_key("is_active"));
        assert!(rel.properties.contains_key("bonus"));
    }
}
