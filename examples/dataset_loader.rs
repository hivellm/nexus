use nexus_core::catalog::Catalog;
use nexus_core::executor::Executor;
use nexus_core::index::{LabelIndex, KnnIndex};
use nexus_protocol::rest::{NodeIngest, RelIngest, IngestRequest};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tempfile::tempdir;
use tokio::sync::RwLock;

/// Dataset loader for Nexus graph database
pub struct DatasetLoader {
    executor: Arc<RwLock<Executor>>,
    catalog: Arc<RwLock<Catalog>>,
    label_index: Arc<RwLock<LabelIndex>>,
    knn_index: Arc<RwLock<KnnIndex>>,
}

impl DatasetLoader {
    /// Create a new dataset loader with initialized components
    pub async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        // Initialize executor
        let executor = Arc::new(RwLock::new(Executor::default()));
        
        // Initialize catalog
        let temp_dir = tempdir()?;
        let catalog = Arc::new(RwLock::new(Catalog::new(temp_dir.path())?));
        
        // Initialize indexes
        let label_index = Arc::new(RwLock::new(LabelIndex::new()));
        let knn_index = Arc::new(RwLock::new(KnnIndex::new(128)?));
        
        Ok(Self {
            executor,
            catalog,
            label_index,
            knn_index,
        })
    }
    
    /// Load a dataset from a JSON file
    pub async fn load_dataset(&self, dataset_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
        let content = std::fs::read_to_string(dataset_path)?;
        let dataset: Value = serde_json::from_str(&content)?;
        
        println!("Loading dataset: {}", dataset["name"].as_str().unwrap_or("Unknown"));
        println!("Description: {}", dataset["description"].as_str().unwrap_or(""));
        
        // Load nodes
        if let Some(nodes) = dataset["nodes"].as_array() {
            println!("Loading {} nodes...", nodes.len());
            self.load_nodes(nodes).await?;
        }
        
        // Load relationships
        if let Some(relationships) = dataset["relationships"].as_array() {
            println!("Loading {} relationships...", relationships.len());
            self.load_relationships(relationships).await?;
        }
        
        // Print statistics
        if let Some(stats) = dataset["statistics"].as_object() {
            println!("\nDataset Statistics:");
            for (key, value) in stats {
                println!("  {}: {}", key, value);
            }
        }
        
        Ok(())
    }
    
    /// Load nodes from dataset
    async fn load_nodes(&self, nodes: &[Value]) -> Result<(), Box<dyn std::error::Error>> {
        let mut ingest_request = IngestRequest {
            nodes: Vec::new(),
            relationships: Vec::new(),
        };
        
        for node in nodes {
            let labels = node["labels"]
                .as_array()
                .unwrap_or(&vec![])
                .iter()
                .map(|v| v.as_str().unwrap_or("").to_string())
                .collect::<Vec<String>>();
            
            let mut properties = node["properties"].as_object().unwrap_or(&serde_json::Map::new()).clone();
            
            // Extract vector if present for KNN index
            if let Some(vector) = properties.get("vector") {
                if let Some(vector_array) = vector.as_array() {
                    let vector_values: Result<Vec<f32>, _> = vector_array
                        .iter()
                        .map(|v| v.as_f64().map(|f| f as f32))
                        .collect::<Option<Vec<_>>>()
                        .ok_or("Invalid vector format")?;
                    
                    // Add to KNN index
                    let node_id = node["id"].as_u64().unwrap_or(0) as u32;
                    let label_id = self.catalog.read().await.get_or_create_label(&labels[0])?;
                    
                    self.knn_index.write().await.add_vector(
                        node_id,
                        label_id,
                        &vector_values,
                    )?;
                    
                    // Remove vector from properties to avoid storing it twice
                    properties.remove("vector");
                }
            }
            
            let node_ingest = NodeIngest {
                id: Some(node["id"].as_u64().unwrap_or(0) as u32),
                labels,
                properties: Value::Object(properties),
            };
            
            ingest_request.nodes.push(node_ingest);
        }
        
        // Execute ingestion
        self.execute_ingestion(ingest_request).await?;
        
        Ok(())
    }
    
    /// Load relationships from dataset
    async fn load_relationships(&self, relationships: &[Value]) -> Result<(), Box<dyn std::error::Error>> {
        let mut ingest_request = IngestRequest {
            nodes: Vec::new(),
            relationships: Vec::new(),
        };
        
        for rel in relationships {
            let rel_ingest = RelIngest {
                id: Some(rel["id"].as_u64().unwrap_or(0) as u32),
                src: rel["source"].as_u64().unwrap_or(0) as u32,
                dst: rel["target"].as_u64().unwrap_or(0) as u32,
                r#type: rel["type"].as_str().unwrap_or("").to_string(),
                properties: rel["properties"].clone(),
            };
            
            ingest_request.relationships.push(rel_ingest);
        }
        
        // Execute ingestion
        self.execute_ingestion(ingest_request).await?;
        
        Ok(())
    }
    
    /// Execute the ingestion request
    async fn execute_ingestion(&self, request: IngestRequest) -> Result<(), Box<dyn std::error::Error>> {
        // This is a simplified version - in a real implementation,
        // you would use the actual ingestion endpoint logic
        println!("Executing ingestion: {} nodes, {} relationships", 
                request.nodes.len(), request.relationships.len());
        
        // For now, just simulate the ingestion
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        
        Ok(())
    }
    
    /// Get dataset statistics
    pub async fn get_stats(&self) -> Result<Value, Box<dyn std::error::Error>> {
        let catalog_stats = self.catalog.read().await.get_stats();
        let label_stats = self.label_index.read().await.get_stats();
        let knn_stats = self.knn_index.read().await.get_stats();
        
        Ok(json!({
            "catalog": {
                "total_labels": catalog_stats.label_count,
                "total_types": catalog_stats.type_count,
                "total_keys": catalog_stats.key_count
            },
            "label_index": {
                "indexed_labels": label_stats.label_count,
                "total_nodes": label_stats.total_nodes
            },
            "knn_index": {
                "total_vectors": knn_stats.total_vectors,
                "dimension": self.knn_index.read().await.dimension()
            }
        }))
    }
}

/// Example usage and test functions
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    
    #[tokio::test]
    async fn test_load_social_network_dataset() {
        let loader = DatasetLoader::new().await.unwrap();
        
        let dataset_path = PathBuf::from("examples/datasets/social_network.json");
        if dataset_path.exists() {
            let result = loader.load_dataset(&dataset_path).await;
            assert!(result.is_ok());
            
            let stats = loader.get_stats().await.unwrap();
            assert!(stats["catalog"]["total_labels"].as_u64().unwrap() > 0);
        }
    }
    
    #[tokio::test]
    async fn test_load_knowledge_graph_dataset() {
        let loader = DatasetLoader::new().await.unwrap();
        
        let dataset_path = PathBuf::from("examples/datasets/knowledge_graph.json");
        if dataset_path.exists() {
            let result = loader.load_dataset(&dataset_path).await;
            assert!(result.is_ok());
            
            let stats = loader.get_stats().await.unwrap();
            assert!(stats["catalog"]["total_labels"].as_u64().unwrap() > 0);
        }
    }
}

/// CLI utility for loading datasets
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    
    if args.len() != 2 {
        eprintln!("Usage: {} <dataset.json>", args[0]);
        eprintln!("Available datasets:");
        eprintln!("  examples/datasets/social_network.json");
        eprintln!("  examples/datasets/knowledge_graph.json");
        std::process::exit(1);
    }
    
    let dataset_path = PathBuf::from(&args[1]);
    
    if !dataset_path.exists() {
        eprintln!("Dataset file not found: {}", dataset_path.display());
        std::process::exit(1);
    }
    
    let loader = DatasetLoader::new().await?;
    loader.load_dataset(&dataset_path).await?;
    
    println!("\nDataset loaded successfully!");
    let stats = loader.get_stats().await?;
    println!("Final statistics: {}", serde_json::to_string_pretty(&stats)?);
    
    Ok(())
}
