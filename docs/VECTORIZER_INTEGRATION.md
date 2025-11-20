# Vectorizer Integration - Technical Implementation Guide

## Table of Contents

1. [Overview](#overview)
2. [Architecture](#architecture)
3. [Core Components](#core-components)
4. [Cypher Extensions](#cypher-extensions)
5. [API Specifications](#api-specifications)
6. [Data Models](#data-models)
7. [Implementation Details](#implementation-details)
8. [Configuration](#configuration)
9. [Testing](#testing)
10. [Deployment](#deployment)
11. [Monitoring](#monitoring)

---

## Overview

This document provides comprehensive technical specifications for integrating Nexus graph database with Vectorizer vector search engine. The integration enables:

- **Hybrid Search**: Combine graph traversal with semantic similarity
- **Context Enrichment**: Enhance embeddings with graph structure
- **Automatic Vectorization**: Create embeddings for graph nodes
- **Bidirectional Sync**: Changes in graph reflected in vector space and vice versa
- **Similarity-Based Relationships**: Automatically create edges based on semantic similarity

### Key Benefits

- **Semantic Graph Queries**: Query graph using natural language
- **Enhanced Discovery**: Find related content across domains
- **Better RAG**: Provide graph context for LLM responses
- **Unified Search**: Single interface for structured and unstructured search

---

## Architecture

### High-Level Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        NEXUS CORE                                │
│                                                                  │
│  ┌──────────────┐    ┌──────────────┐    ┌─────────────────┐  │
│  │ GraphEngine  │───►│SyncCoordinator│───►│VectorizerClient│  │
│  │              │    │              │    │                 │  │
│  │ - Cypher     │    │ - events     │    │ - insert_text() │  │
│  │ - KNN        │    │ - hooks      │    │ - search()      │  │
│  │ - Storage    │    │ - workers    │    │ - get_similar() │  │
│  └──────┬───────┘    └──────┬───────┘    └────────┬────────┘  │
│         │                   │                      │            │
│         │                   │                      │ HTTP/REST  │
│         │                   ▼                      │            │
│         │          ┌────────────────┐              │            │
│         │          │ WebhookHandler │              │            │
│         │          │                │              │            │
│         │          │ - similarity   │              │            │
│         │          │ - document     │              │            │
│         │          └────────────────┘              │            │
│         │                   ▲                      │            │
│         │                   │ Webhooks             │            │
│         ▼                   │                      ▼            │
│  ┌─────────────────────────┴──────────────────────────────┐   │
│  │         Cypher Executor with Vector Procedures          │   │
│  │  - vector.semantic_search()                             │   │
│  │  - vector.hybrid_search()                               │   │
│  │  - vector.build_similarity_graph()                      │   │
│  └─────────────────────────────────────────────────────────┘   │
└────────────────────────────────┬─────────────────────────────┘
                                 │
                                 │ HTTP/JSON
                                 │
┌────────────────────────────────▼─────────────────────────────┐
│                        VECTORIZER                             │
│                                                               │
│  ┌────────────────┐         ┌──────────────────┐            │
│  │  VectorStore   │         │ EmbeddingManager │            │
│  │                │         │                  │            │
│  │ - Collections  │         │ - Fastembed      │            │
│  │ - HNSW Index   │         │ - GPU Accel      │            │
│  │ - Persistence  │         │ - Caching        │            │
│  └────────────────┘         └──────────────────┘            │
└───────────────────────────────────────────────────────────────┘
```

### Component Interaction Sequence

```
User Query (Cypher)
  │
  │ CALL vector.hybrid_search($query_text, ...)
  ├────────────────────►Cypher Executor
  │                           │
  │                           │ 1. Parse query
  │                           │ 2. Plan execution
  │                           │
  │                     ┌─────▼────────┐
  │                     │ Graph Search │
  │                     │  (Pattern)   │
  │                     └─────┬────────┘
  │                           │ graph_results
  │                           │
  │                     ┌─────▼────────────┐
  │                     │ VectorizerClient │
  │                     │   semantic_search│
  │                     └─────┬────────────┘
  │                           │
  │                           │ HTTP POST /api/v1/search
  │                           ├──────────►Vectorizer
  │                           │              │
  │                           │              │ KNN Search
  │                           │◄─────────────┘ semantic_results
  │                           │
  │                     ┌─────▼────────┐
  │                     │  RRF Merger  │
  │                     │  (Hybrid)    │
  │                     └─────┬────────┘
  │                           │ combined_results
  │◄──────────────────────────┘
  │ Return Results
  ▼
```

---

## Core Components

### 1. VectorizerClient (`nexus-core/src/vectorizer_client/mod.rs`)

HTTP client for Vectorizer REST API.

```rust
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

pub struct VectorizerClient {
    client: Client,
    base_url: String,
    api_key: Option<String>,
}

impl VectorizerClient {
    pub fn new(base_url: String, api_key: Option<String>) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .pool_max_idle_per_host(10)
            .build()?;

        Ok(Self {
            client,
            base_url,
            api_key,
        })
    }

    /// Insert text document into Vectorizer collection
    pub async fn insert_text(
        &self,
        collection: &str,
        text: &str,
        metadata: HashMap<String, Value>,
    ) -> Result<VectorInsertResponse> {
        let url = format!("{}/insert", self.base_url);
        
        let request = InsertRequest {
            collection: collection.to_string(),
            text: text.to_string(),
            metadata,
        };

        let mut req = self.client.post(&url).json(&request);
        
        if let Some(key) = &self.api_key {
            req = req.header("Authorization", format!("Bearer {}", key));
        }

        let response = req.send().await?;
        
        if !response.status().is_success() {
            let error: ErrorResponse = response.json().await?;
            return Err(Error::VectorizerApi(error));
        }

        Ok(response.json().await?)
    }

    /// Semantic search across Vectorizer collections
    pub async fn semantic_search(
        &self,
        collections: &[String],
        query: &str,
        limit: usize,
        filters: Option<HashMap<String, Value>>,
    ) -> Result<Vec<SearchResult>> {
        let url = format!("{}/api/v1/search/semantic", self.base_url);
        
        let request = SemanticSearchRequest {
            collections: collections.to_vec(),
            query: query.to_string(),
            limit,
            filters,
        };

        let mut req = self.client.post(&url).json(&request);
        
        if let Some(key) = &self.api_key {
            req = req.header("Authorization", format!("Bearer {}", key));
        }

        let response = req.send().await?;
        
        if !response.status().is_success() {
            return Err(Error::VectorizerApi(response.json().await?));
        }

        let search_response: SemanticSearchResponse = response.json().await?;
        Ok(search_response.results)
    }

    /// Get similar documents to a vector
    pub async fn get_similar(
        &self,
        collection: &str,
        vector_id: &str,
        k: usize,
    ) -> Result<Vec<SimilarDocument>> {
        let url = format!("{}/api/v1/similar", self.base_url);
        
        let request = SimilarRequest {
            collection: collection.to_string(),
            vector_id: vector_id.to_string(),
            k,
        };

        let mut req = self.client.post(&url).json(&request);
        
        if let Some(key) = &self.api_key {
            req = req.header("Authorization", format!("Bearer {}", key));
        }

        let response = req.send().await?;
        
        if !response.status().is_success() {
            return Err(Error::VectorizerApi(response.json().await?));
        }

        let similar_response: SimilarResponse = response.json().await?;
        Ok(similar_response.similar)
    }

    /// Update existing vector
    pub async fn update_vector(
        &self,
        collection: &str,
        vector_id: &str,
        text: Option<&str>,
        metadata: Option<HashMap<String, Value>>,
    ) -> Result<()> {
        let url = format!("{}/update_vector", self.base_url);
        
        let request = UpdateVectorRequest {
            collection: collection.to_string(),
            vector_id: vector_id.to_string(),
            text: text.map(|s| s.to_string()),
            metadata,
        };

        let mut req = self.client.post(&url).json(&request);
        
        if let Some(key) = &self.api_key {
            req = req.header("Authorization", format!("Bearer {}", key));
        }

        let response = req.send().await?;
        
        if !response.status().is_success() {
            return Err(Error::VectorizerApi(response.json().await?));
        }

        Ok(())
    }

    /// Delete vector from collection
    pub async fn delete_vector(
        &self,
        collection: &str,
        vector_id: &str,
    ) -> Result<()> {
        let url = format!("{}/delete_vector", self.base_url);
        
        let request = DeleteVectorRequest {
            collection: collection.to_string(),
            vector_id: vector_id.to_string(),
        };

        let mut req = self.client.post(&url).json(&request);
        
        if let Some(key) = &self.api_key {
            req = req.header("Authorization", format!("Bearer {}", key));
        }

        let response = req.send().await?;
        
        if !response.status().is_success() {
            return Err(Error::VectorizerApi(response.json().await?));
        }

        Ok(())
    }
}

// Request/Response Types

#[derive(Debug, Serialize)]
struct InsertRequest {
    collection: String,
    text: String,
    metadata: HashMap<String, Value>,
}

#[derive(Debug, Deserialize)]
pub struct VectorInsertResponse {
    pub message: String,
    pub vector_id: String,
    pub collection: String,
}

#[derive(Debug, Serialize)]
struct SemanticSearchRequest {
    collections: Vec<String>,
    query: String,
    limit: usize,
    filters: Option<HashMap<String, Value>>,
}

#[derive(Debug, Deserialize)]
struct SemanticSearchResponse {
    results: Vec<SearchResult>,
    execution_time_ms: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SearchResult {
    pub id: String,
    pub score: f32,
    pub collection: String,
    pub metadata: HashMap<String, Value>,
}

#[derive(Debug, Serialize)]
struct SimilarRequest {
    collection: String,
    vector_id: String,
    k: usize,
}

#[derive(Debug, Deserialize)]
struct SimilarResponse {
    similar: Vec<SimilarDocument>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SimilarDocument {
    pub vector_id: String,
    pub score: f32,
    pub metadata: HashMap<String, Value>,
}
```

### 2. SyncCoordinator (`nexus-core/src/vectorizer_sync/coordinator.rs`)

Orchestrates synchronization between graph and vector space.

```rust
use tokio::sync::mpsc;
use std::sync::Arc;
use parking_lot::RwLock;

pub struct SyncCoordinator {
    vectorizer_client: Arc<VectorizerClient>,
    config: VectorizerSyncConfig,
    event_tx: mpsc::UnboundedSender<SyncEvent>,
    event_rx: Arc<RwLock<Option<mpsc::UnboundedReceiver<SyncEvent>>>>,
    workers: Vec<tokio::task::JoinHandle<()>>,
    state: Arc<RwLock<SyncState>>,
}

#[derive(Debug, Clone)]
pub struct VectorizerSyncConfig {
    pub enabled: bool,
    pub worker_threads: usize,
    pub batch_size: usize,
    pub auto_create_vectors: bool,
    pub auto_update_vectors: bool,
    pub collection_mappings: HashMap<String, String>, // label -> collection
    pub enrichment: EnrichmentConfig,
}

#[derive(Debug, Clone)]
pub struct EnrichmentConfig {
    pub enabled: bool,
    pub include_relationships: bool,
    pub max_relationship_depth: usize,
    pub relationship_types: Vec<String>,
}

#[derive(Debug, Clone)]
pub enum SyncEvent {
    NodeCreated {
        node_id: u64,
        labels: Vec<String>,
        properties: HashMap<String, Value>,
    },
    NodeUpdated {
        node_id: u64,
        labels: Vec<String>,
        properties: HashMap<String, Value>,
        changed_props: Vec<String>,
    },
    NodeDeleted {
        node_id: u64,
        vector_id: Option<String>,
        collection: Option<String>,
    },
    RelationshipCreated {
        rel_id: u64,
        source_id: u64,
        target_id: u64,
        rel_type: String,
    },
}

impl SyncCoordinator {
    pub fn new(
        vectorizer_client: Arc<VectorizerClient>,
        config: VectorizerSyncConfig,
    ) -> Result<Self> {
        let (event_tx, event_rx) = mpsc::unbounded_channel();

        Ok(Self {
            vectorizer_client,
            config,
            event_tx,
            event_rx: Arc::new(RwLock::new(Some(event_rx))),
            workers: Vec::new(),
            state: Arc::new(RwLock::new(SyncState::default())),
        })
    }

    /// Start sync workers
    pub async fn start(&mut self) -> Result<()> {
        if !self.config.enabled {
            info!("Vectorizer sync is disabled");
            return Ok(());
        }

        let mut rx = self.event_rx.write()
            .take()
            .ok_or_else(|| Error::AlreadyStarted)?;

        // Start worker threads
        for worker_id in 0..self.config.worker_threads {
            let client = self.vectorizer_client.clone();
            let config = self.config.clone();
            let state = self.state.clone();

            let handle = tokio::spawn(async move {
                Self::worker_loop(
                    worker_id,
                    &mut rx,
                    client,
                    config,
                    state,
                ).await;
            });

            self.workers.push(handle);
        }

        info!("Started {} Vectorizer sync workers", self.config.worker_threads);
        Ok(())
    }

    /// Worker loop processing sync events
    async fn worker_loop(
        worker_id: usize,
        rx: &mut mpsc::UnboundedReceiver<SyncEvent>,
        client: Arc<VectorizerClient>,
        config: VectorizerSyncConfig,
        state: Arc<RwLock<SyncState>>,
    ) {
        debug!("Vectorizer sync worker {} started", worker_id);

        while let Some(event) = rx.recv().await {
            let result = Self::process_event(&event, &client, &config).await;

            match result {
                Ok(sync_result) => {
                    let mut state = state.write();
                    state.total_synced += 1;
                    state.last_sync = Some(Utc::now());
                    
                    info!(
                        "Worker {} synced: {:?}",
                        worker_id,
                        sync_result
                    );
                }
                Err(e) => {
                    let mut state = state.write();
                    state.total_errors += 1;
                    
                    error!(
                        "Worker {} sync failed: {:?}",
                        worker_id,
                        e
                    );
                }
            }
        }

        debug!("Vectorizer sync worker {} stopped", worker_id);
    }

    /// Process sync event
    async fn process_event(
        event: &SyncEvent,
        client: &Arc<VectorizerClient>,
        config: &VectorizerSyncConfig,
    ) -> Result<SyncResult> {
        match event {
            SyncEvent::NodeCreated { node_id, labels, properties } => {
                Self::handle_node_created(*node_id, labels, properties, client, config).await
            }
            SyncEvent::NodeUpdated { node_id, labels, properties, changed_props } => {
                Self::handle_node_updated(*node_id, labels, properties, changed_props, client, config).await
            }
            SyncEvent::NodeDeleted { node_id, vector_id, collection } => {
                Self::handle_node_deleted(*node_id, vector_id, collection, client).await
            }
            SyncEvent::RelationshipCreated { rel_id, source_id, target_id, rel_type } => {
                Self::handle_relationship_created(*rel_id, *source_id, *target_id, rel_type, client, config).await
            }
        }
    }

    /// Handle node creation
    async fn handle_node_created(
        node_id: u64,
        labels: &[String],
        properties: &HashMap<String, Value>,
        client: &Arc<VectorizerClient>,
        config: &VectorizerSyncConfig,
    ) -> Result<SyncResult> {
        if !config.auto_create_vectors {
            return Ok(SyncResult::skipped());
        }

        // Find collection mapping
        let collection = labels.iter()
            .find_map(|label| config.collection_mappings.get(label))
            .cloned()
            .unwrap_or_else(|| "documents".to_string());

        // Extract text from properties
        let text = Self::extract_text_from_properties(properties)?;

        // Build metadata
        let mut metadata = properties.clone();
        metadata.insert("node_id".to_string(), json!(node_id));
        metadata.insert("labels".to_string(), json!(labels));

        // Insert into Vectorizer
        let response = client.insert_text(
            &collection,
            &text,
            metadata,
        ).await?;

        // Store vector_id back to node (would need graph engine reference)
        // This would be done via a callback or message

        Ok(SyncResult {
            success: true,
            vector_id: Some(response.vector_id),
            collection: Some(response.collection),
        })
    }

    /// Extract text content from node properties
    fn extract_text_from_properties(properties: &HashMap<String, Value>) -> Result<String> {
        let mut text_parts = Vec::new();

        // Priority fields
        if let Some(title) = properties.get("title").and_then(|v| v.as_str()) {
            text_parts.push(title.to_string());
        }

        if let Some(content) = properties.get("content").and_then(|v| v.as_str()) {
            text_parts.push(content.to_string());
        }

        if let Some(description) = properties.get("description").and_then(|v| v.as_str()) {
            text_parts.push(description.to_string());
        }

        // If no text found, concatenate all string properties
        if text_parts.is_empty() {
            for (key, value) in properties {
                if let Some(str_val) = value.as_str() {
                    text_parts.push(format!("{}: {}", key, str_val));
                }
            }
        }

        if text_parts.is_empty() {
            return Err(Error::NoTextContent("No text content found in node".to_string()));
        }

        Ok(text_parts.join("\n"))
    }

    /// Enqueue sync event
    pub fn enqueue(&self, event: SyncEvent) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        self.event_tx.send(event)
            .map_err(|e| Error::SyncQueueFull(e.to_string()))?;

        Ok(())
    }

    /// Get sync state
    pub fn get_state(&self) -> SyncState {
        self.state.read().clone()
    }

    /// Shutdown gracefully
    pub async fn shutdown(mut self) -> Result<()> {
        drop(self.event_tx);

        for handle in self.workers.drain(..) {
            handle.await?;
        }

        info!("Vectorizer sync coordinator shut down gracefully");
        Ok(())
    }
}

#[derive(Debug, Clone, Default)]
pub struct SyncState {
    pub total_synced: u64,
    pub total_errors: u64,
    pub last_sync: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct SyncResult {
    pub success: bool,
    pub vector_id: Option<String>,
    pub collection: Option<String>,
}

impl SyncResult {
    pub fn skipped() -> Self {
        Self {
            success: true,
            vector_id: None,
            collection: None,
        }
    }
}
```

---

## Cypher Extensions

### New Procedures

#### 1. vector.semantic_search()

```cypher
// Search for nodes semantically across Vectorizer collections
CALL vector.semantic_search(
  $query_text,         // Natural language query
  $collections,        // List of Vectorizer collections to search
  $k,                  // Number of results
  $filters             // Optional metadata filters
) YIELD node, score, collection
RETURN node, score, collection
ORDER BY score DESC
```

**Example:**
```cypher
CALL vector.semantic_search(
  "contract termination clauses",
  ["legal_documents", "contracts"],
  20,
  {domain: "legal"}
) YIELD node, score
WHERE node.status = "active"
RETURN node.title, score
ORDER BY score DESC
LIMIT 10
```

#### 2. vector.hybrid_search()

```cypher
// Combine graph pattern matching with semantic search
CALL vector.hybrid_search(
  $query_text,         // Natural language query
  $graph_pattern,      // Cypher pattern to match
  $k,                  // Number of results
  $rrf_k               // RRF parameter (default: 60)
) YIELD node, graph_score, semantic_score, combined_score
RETURN node, graph_score, semantic_score, combined_score
ORDER BY combined_score DESC
```

**Example:**
```cypher
CALL vector.hybrid_search(
  "data privacy regulations",
  "(doc:Document)-[:REFERENCES]->(law:Law)",
  10,
  60
) YIELD node, combined_score
RETURN node.title, node.domain, combined_score
ORDER BY combined_score DESC
```

#### 3. vector.build_similarity_graph()

```cypher
// Build similarity edges between nodes based on vector similarity
CALL vector.build_similarity_graph(
  $label,              // Node label to process
  $threshold,          // Similarity threshold (0.0-1.0)
  $max_edges_per_node  // Max edges to create per node
) YIELD relationships_created, execution_time_ms
RETURN relationships_created, execution_time_ms
```

**Example:**
```cypher
// Build similarity graph for all Documents
CALL vector.build_similarity_graph(
  "Document",
  0.75,
  20
) YIELD relationships_created, execution_time_ms
RETURN relationships_created, execution_time_ms
```

---

## Configuration

### config.yml

```yaml
# Vectorizer Integration Configuration
vectorizer_integration:
  # Enable/disable integration
  enabled: true
  
  # Vectorizer server URL
  vectorizer_url: "http://localhost:15002"
  
  # API key for authentication
  api_key: "${VECTORIZER_API_KEY}"
  
  # Connection settings
  connection:
    timeout_seconds: 30
    pool_size: 10
  
  # Sync settings
  sync:
    mode: "async"
    worker_threads: 2
    batch_size: 50
    auto_create_vectors: true
    auto_update_vectors: true
  
  # Collection mappings (label -> collection)
  collections:
    Document: "documents"
    LegalDocument: "legal_documents"
    FinancialDocument: "financial_documents"
    HRDocument: "hr_documents"
    Code: "engineering_documents"
  
  # Context enrichment
  enrichment:
    enabled: true
    include_relationships: true
    max_relationship_depth: 2
    relationship_types:
      - "REFERENCES"
      - "SIMILAR_TO"
      - "MENTIONS"
      - "BELONGS_TO"
  
  # Similarity settings
  similarity:
    auto_create_edges: true
    threshold: 0.75
    max_edges_per_node: 20
    edge_type: "SIMILAR_TO"
    batch_size: 100
  
  # Webhooks
  webhooks:
    enabled: true
    secret: "${VECTORIZER_WEBHOOK_SECRET}"
    endpoints:
      - path: "/webhooks/vectorizer/similarity"
        events: ["similarity.created", "similarity.updated"]
      - path: "/webhooks/vectorizer/document"
        events: ["document.created", "document.updated"]
```

---

## Testing

### Integration Test Example

```rust
#[tokio::test]
async fn test_hybrid_search() {
    // Setup
    let graph = create_test_graph();
    let vectorizer = create_test_vectorizer_client();
    
    // Create nodes with text content
    graph.execute(r#"
        CREATE (d1:Document {title: "GDPR Overview", content: "General Data Protection Regulation..."})
        CREATE (d2:Document {title: "Data Privacy Policy", content: "Company data privacy..."})
        CREATE (d1)-[:REFERENCES]->(d2)
    "#).await.unwrap();
    
    // Wait for sync
    tokio::time::sleep(Duration::from_secs(2)).await;
    
    // Execute hybrid search
    let results = graph.execute(r#"
        CALL vector.hybrid_search(
            "data protection regulations",
            "(d:Document)",
            10,
            60
        ) YIELD node, combined_score
        RETURN node.title, combined_score
        ORDER BY combined_score DESC
    "#).await.unwrap();
    
    assert!(results.len() > 0);
    assert_eq!(results[0]["node.title"], "GDPR Overview");
}
```

---

## Monitoring

### Prometheus Metrics

```rust
use prometheus::{register_counter_vec, register_histogram_vec, CounterVec, HistogramVec};

lazy_static! {
    pub static ref VECTORIZER_SYNC_TOTAL: CounterVec = register_counter_vec!(
        "nexus_vectorizer_sync_total",
        "Total Vectorizer sync operations",
        &["label", "status"]
    ).unwrap();

    pub static ref HYBRID_SEARCH_DURATION: HistogramVec = register_histogram_vec!(
        "nexus_hybrid_search_duration_seconds",
        "Hybrid search duration",
        &["component"]  // "graph", "semantic", "rrf"
    ).unwrap();
}
```

---

This completes the technical implementation documentation for Nexus.

