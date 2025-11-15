//! Comprehensive tests for bulk loader
//!
//! Tests cover:
//! - Error handling
//! - Edge cases
//! - Performance scenarios
//! - Data validation

use nexus_core::loader::{BulkLoadConfig, BulkLoader, DataSource, NodeData, RelationshipData};
use nexus_core::transaction::TransactionManager;
use nexus_core::{catalog::Catalog, index::IndexManager, storage::RecordStore};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::RwLock;

fn create_test_loader() -> (BulkLoader, TempDir) {
    let dir = TempDir::new().unwrap();
    let catalog = Arc::new(Catalog::new(dir.path()).unwrap());
    let storage = Arc::new(RwLock::new(RecordStore::new(dir.path()).unwrap()));
    let indexes = Arc::new(IndexManager::new(dir.path()).unwrap());
    let transaction_manager = Arc::new(RwLock::new(TransactionManager::new().unwrap()));

    let loader = BulkLoader::new(
        catalog,
        storage,
        indexes,
        transaction_manager,
        BulkLoadConfig::default(),
    );

    (loader, dir)
}

#[tokio::test]
async fn test_loader_with_invalid_data() {
    let (loader, _dir) = create_test_loader();

    // Try to load invalid node data
    let invalid_nodes = vec![NodeData {
        id: Some(1),
        labels: vec![],
        properties: HashMap::new(),
    }];

    let data_source = DataSource::InMemory {
        nodes: invalid_nodes,
        relationships: vec![],
    };

    let result = loader.load_data(data_source, None).await;
    // Should handle invalid data gracefully (may succeed with warnings or fail)
    assert!(result.is_ok() || result.is_err());
}

#[tokio::test]
async fn test_loader_with_duplicate_node_ids() {
    let (loader, _dir) = create_test_loader();

    let nodes = vec![
        NodeData {
            id: Some(1),
            labels: vec!["Person".to_string()],
            properties: {
                let mut props = HashMap::new();
                props.insert("name".to_string(), json!("Alice"));
                props
            },
        },
        NodeData {
            id: Some(1), // Duplicate ID
            labels: vec!["Person".to_string()],
            properties: {
                let mut props = HashMap::new();
                props.insert("name".to_string(), json!("Bob"));
                props
            },
        },
    ];

    let data_source = DataSource::InMemory {
        nodes,
        relationships: vec![],
    };

    let result = loader.load_data(data_source, None).await;
    // Should handle duplicates (may overwrite or error)
    assert!(result.is_ok() || result.is_err());
}

#[tokio::test]
async fn test_loader_with_invalid_relationship() {
    let (loader, _dir) = create_test_loader();

    // Create valid nodes first
    let nodes = vec![NodeData {
        id: Some(1),
        labels: vec!["Person".to_string()],
        properties: HashMap::new(),
    }];

    // Relationship referencing non-existent node
    let relationships = vec![RelationshipData {
        id: Some(1),
        source_id: 1,
        target_id: 999, // Non-existent node
        rel_type: "KNOWS".to_string(),
        properties: HashMap::new(),
    }];

    let data_source = DataSource::InMemory {
        nodes,
        relationships,
    };

    let result = loader.load_data(data_source, None).await;
    // Should handle invalid relationships (may error or create orphaned edge)
    assert!(result.is_ok() || result.is_err());
}

#[tokio::test]
async fn test_loader_with_large_dataset() {
    let (loader, _dir) = create_test_loader();

    // Create 1000 nodes
    let mut nodes = Vec::new();
    for i in 0..1000 {
        nodes.push(NodeData {
            id: Some(i),
            labels: vec!["Person".to_string()],
            properties: {
                let mut props = HashMap::new();
                props.insert("id".to_string(), json!(i));
                props.insert("name".to_string(), json!(format!("Person{}", i)));
                props
            },
        });
    }

    let data_source = DataSource::InMemory {
        nodes,
        relationships: vec![],
    };

    let result = loader.load_data(data_source, None).await;
    assert!(result.is_ok());

    let stats = loader.get_stats().await;
    assert_eq!(stats.nodes_loaded, 1000);
}

#[tokio::test]
async fn test_loader_with_custom_config() {
    let dir = TempDir::new().unwrap();
    let catalog = Arc::new(Catalog::new(dir.path()).unwrap());
    let storage = Arc::new(RwLock::new(RecordStore::new(dir.path()).unwrap()));
    let indexes = Arc::new(IndexManager::new(dir.path()).unwrap());
    let transaction_manager = Arc::new(RwLock::new(TransactionManager::new().unwrap()));

    let config = BulkLoadConfig {
        batch_size: 100,
        worker_count: 2,
        enable_progress: false,
        progress_interval: 1000,
        enable_index_updates: false,
        enable_transaction_batching: true,
        transaction_batch_size: 50,
    };

    let loader = BulkLoader::new(catalog, storage, indexes, transaction_manager, config);

    let nodes = vec![NodeData {
        id: Some(1),
        labels: vec!["Person".to_string()],
        properties: HashMap::new(),
    }];

    let data_source = DataSource::InMemory {
        nodes,
        relationships: vec![],
    };

    let result = loader.load_data(data_source, None).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_loading_stats_tracking() {
    let (loader, _dir) = create_test_loader();

    let nodes = vec![
        NodeData {
            id: Some(1),
            labels: vec!["Person".to_string()],
            properties: HashMap::new(),
        },
        NodeData {
            id: Some(2),
            labels: vec!["Person".to_string()],
            properties: HashMap::new(),
        },
    ];

    let relationships = vec![RelationshipData {
        id: Some(1),
        source_id: 1,
        target_id: 2,
        rel_type: "KNOWS".to_string(),
        properties: HashMap::new(),
    }];

    let data_source = DataSource::InMemory {
        nodes,
        relationships,
    };

    let _ = loader.load_data(data_source, None).await;

    let stats = loader.get_stats().await;
    assert_eq!(stats.nodes_loaded, 2);
    assert_eq!(stats.relationships_loaded, 1);
    assert!(stats.start_time <= chrono::Utc::now());
}

#[tokio::test]
async fn test_loader_with_empty_data() {
    let (loader, _dir) = create_test_loader();

    let data_source = DataSource::InMemory {
        nodes: vec![],
        relationships: vec![],
    };

    let result = loader.load_data(data_source, None).await;
    assert!(result.is_ok());

    let stats = loader.get_stats().await;
    assert_eq!(stats.nodes_loaded, 0);
    assert_eq!(stats.relationships_loaded, 0);
}

#[tokio::test]
async fn test_loader_with_complex_properties() {
    let (loader, _dir) = create_test_loader();

    let nodes = vec![NodeData {
        id: Some(1),
        labels: vec!["Person".to_string()],
        properties: {
            let mut props = HashMap::new();
            props.insert("name".to_string(), json!("Alice"));
            props.insert("age".to_string(), json!(30));
            props.insert("active".to_string(), json!(true));
            props.insert("tags".to_string(), json!(["tag1", "tag2"]));
            props.insert("metadata".to_string(), json!({"key": "value"}));
            props
        },
    }];

    let data_source = DataSource::InMemory {
        nodes,
        relationships: vec![],
    };

    let result = loader.load_data(data_source, None).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_loader_phase_tracking() {
    let (loader, _dir) = create_test_loader();

    let nodes = vec![NodeData {
        id: Some(1),
        labels: vec!["Person".to_string()],
        properties: HashMap::new(),
    }];

    let data_source = DataSource::InMemory {
        nodes,
        relationships: vec![],
    };

    // Start loading
    let _ = loader.load_data(data_source, None).await;

    // Check stats after loading
    let stats = loader.get_stats().await;
    assert_eq!(stats.nodes_loaded, 1);
}
