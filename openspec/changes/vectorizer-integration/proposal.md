# Vectorizer Integration - Proposal

## Overview

Integration of Nexus graph database with Vectorizer to enable bidirectional synchronization, context enrichment from graph relationships, and hybrid search combining graph traversal with semantic similarity.

## Motivation

Current Nexus implementation focuses on graph relationships but lacks:
- **Semantic Search**: No native embedding-based similarity search beyond KNN
- **Document Indexing**: Limited full-text search capabilities
- **Auto-Relationship Discovery**: Manual relationship creation required
- **Multi-Format Support**: No PDF, DOCX, or document conversion
- **Semantic Context**: Missing semantic meaning in graph context

## Goals

### Primary Goals
1. **Bidirectional Sync**: Automatic synchronization with Vectorizer
2. **Context Enrichment**: Enhance graph nodes with semantic embeddings
3. **Hybrid Search**: Combine Cypher queries with semantic search
4. **Auto-Relationship Discovery**: Automatically discover relationships via similarity
5. **Graph-Enhanced Embeddings**: Use graph context to improve embedding quality

### Secondary Goals
1. **Document Ingestion**: Support PDF, DOCX, and other formats via Vectorizer
2. **Semantic Similarity Edges**: Create edges based on embedding similarity
3. **Cross-Collection Search**: Search across Vectorizer collections from graph queries
4. **Impact Analysis**: Enhanced impact analysis using semantic similarity

## Architecture

### Components

```
┌──────────────────────────────────────────────────────────────┐
│                        NEXUS                                  │
│                                                               │
│  ┌────────────────┐         ┌──────────────────┐            │
│  │  Graph Engine  │◄───────►│ VectorizerClient │            │
│  │  (Existing)    │         │     (New)        │            │
│  └────────┬───────┘         └────────┬─────────┘            │
│           │                           │                       │
│  ┌────────▼───────────────────────────▼─────────┐           │
│  │          SyncCoordinator (New)                │           │
│  │  - Listen for graph events                    │           │
│  │  - Trigger Vectorizer updates                 │           │
│  │  - Manage webhooks                            │           │
│  └────────────────────────────────────────────────┘          │
│           │                                                   │
│  ┌────────▼───────────────────────────────────────┐         │
│  │     Graph Correlation Analysis (Existing)      │         │
│  │  - Enhanced with semantic context              │         │
│  └────────────────────────────────────────────────┘         │
└───────────────────────────┬──────────────────────────────────┘
                            │ HTTP/REST + Webhooks
┌───────────────────────────▼──────────────────────────────────┐
│                      VECTORIZER                               │
│                                                               │
│  ┌────────────────┐         ┌──────────────────┐            │
│  │  VectorStore   │         │ EmbeddingManager │            │
│  │  (Existing)    │         │  (Existing)      │            │
│  └────────────────┘         └──────────────────┘            │
└───────────────────────────────────────────────────────────────┘
```

### Data Flow

#### Nexus → Vectorizer (Node Creation/Update)

```
1. Node created/updated in Nexus
   ↓
2. GraphEventHandler detects change
   ↓
3. Extract node properties and text content
   ↓
4. Build enriched context from relationships
   ↓
5. Call VectorizerClient.upsert_document()
   ↓
6. Vectorizer generates embedding
   ↓
7. Store in appropriate collection
   ↓
8. Return vector_id and store in node properties
```

#### Vectorizer → Nexus (Webhook for Enrichment)

```
1. Similar documents found in Vectorizer
   ↓
2. Webhook notification to Nexus
   ↓
3. SyncCoordinator processes webhook
   ↓
4. Query both nodes in graph
   ↓
5. Create/update SIMILAR_TO relationship
   ↓
6. Update relationship score
```

## Implementation Plan

### Phase 1: Core Infrastructure (Week 1-2)

**Nexus Changes:**
- Create `vectorizer_client` module for Vectorizer REST API
- Implement `SyncCoordinator` for event management
- Add webhook system for receiving Vectorizer events
- Create configuration management

**Deliverables:**
- Vectorizer client with retry logic
- Event listener for graph changes
- Webhook receivers
- Configuration validation

### Phase 2: Bidirectional Sync (Week 3-4)

**Nexus Changes:**
- Implement graph event hooks (node create/update/delete)
- Add automatic vector creation on node changes
- Create relationship synchronization
- Implement context extraction from graph

**Deliverables:**
- Auto-sync on graph mutations
- Context-enriched embeddings
- Bidirectional relationship sync
- Error handling and recovery

### Phase 3: Hybrid Search (Week 5-6)

**Nexus Changes:**
- Extend Cypher with semantic search procedures
- Implement RRF (Reciprocal Rank Fusion) for hybrid ranking
- Add cross-collection graph queries
- Create semantic similarity edge builder

**Deliverables:**
- `CALL vector.semantic_search()` procedure
- Hybrid search API endpoints
- Similarity-based relationship creation
- Performance optimization

### Phase 4: Production Features (Week 7-8)

**Nexus Changes:**
- Add monitoring and metrics
- Implement sync reconciliation tools
- Create admin API for sync management
- Add comprehensive error handling

**Deliverables:**
- Prometheus metrics
- Reconciliation tool
- Admin endpoints
- Production-ready deployment

## API Changes

### New Cypher Procedures

```cypher
// Semantic search across Vectorizer collections
CALL vector.semantic_search(
  $query_text,
  $collections,    // Vectorizer collections to search
  $k,
  $filters
) YIELD node, score, collection

// Hybrid search (graph + semantic)
CALL vector.hybrid_search(
  $query_text,
  $graph_pattern,
  $k,
  $rrf_k
) YIELD node, graph_score, semantic_score, combined_score

// Create similarity edges from Vectorizer data
CALL vector.build_similarity_graph(
  $label,
  $threshold,
  $max_edges_per_node
) YIELD relationships_created, execution_time
```

### New REST Endpoints

```
POST   /api/v1/sync/vectorizer/enable       Enable Vectorizer sync
POST   /api/v1/sync/vectorizer/disable      Disable sync
GET    /api/v1/sync/vectorizer/status       Get sync status
POST   /api/v1/sync/vectorizer/reconcile    Reconcile graph with Vectorizer

POST   /webhooks/vectorizer/similarity      Receive similarity notifications
POST   /webhooks/vectorizer/document        Receive document updates
```

### Configuration

```yaml
# config.yml
vectorizer_integration:
  enabled: true
  vectorizer_url: "http://localhost:15002"
  api_key: "${VECTORIZER_API_KEY}"
  
  # Sync settings
  sync_mode: "async"
  auto_create_vectors: true
  auto_update_vectors: true
  batch_size: 50
  worker_threads: 2
  
  # Collection mapping
  collections:
    Document:
      collection: "documents"
      enabled: true
    LegalDocument:
      collection: "legal_documents"
      enabled: true
    FinancialDocument:
      collection: "financial_documents"
      enabled: true
  
  # Context enrichment
  enrichment:
    enabled: true
    include_relationships: true
    max_relationship_depth: 2
    relationship_types: ["REFERENCES", "SIMILAR_TO", "MENTIONS"]
  
  # Similarity settings
  similarity:
    auto_create_edges: true
    threshold: 0.75
    max_edges_per_node: 20
    edge_type: "SIMILAR_TO"
  
  # Webhooks
  webhooks:
    enabled: true
    secret: "${VECTORIZER_WEBHOOK_SECRET}"
```

## Data Models

### Extended Node Properties

```rust
// Additional properties added to nodes when synced
pub struct VectorizerSyncProperties {
    /// Vectorizer vector ID
    pub vector_id: Option<String>,
    
    /// Vectorizer collection name
    pub vectorizer_collection: Option<String>,
    
    /// Last sync timestamp
    pub vectorizer_synced_at: Option<DateTime<Utc>>,
    
    /// Sync status
    pub vectorizer_sync_status: Option<SyncStatus>,
    
    /// Embedding dimension
    pub embedding_dimension: Option<usize>,
}
```

### VectorizerClient

```rust
pub struct VectorizerClient {
    client: reqwest::Client,
    base_url: String,
    api_key: Option<String>,
}

impl VectorizerClient {
    /// Insert/update document in Vectorizer
    pub async fn upsert_document(
        &self,
        collection: &str,
        text: &str,
        metadata: HashMap<String, Value>,
    ) -> Result<VectorUpsertResponse>;
    
    /// Search across collections
    pub async fn semantic_search(
        &self,
        collections: &[String],
        query: &str,
        limit: usize,
        filters: Option<HashMap<String, Value>>,
    ) -> Result<Vec<SearchResult>>;
    
    /// Get similar documents
    pub async fn get_similar(
        &self,
        collection: &str,
        vector_id: &str,
        k: usize,
    ) -> Result<Vec<SimilarDocument>>;
    
    /// Delete vector
    pub async fn delete_vector(
        &self,
        collection: &str,
        vector_id: &str,
    ) -> Result<()>;
}
```

## Testing Strategy

### Unit Tests
- Vectorizer client communication
- Context extraction from graph
- Event handling
- Webhook validation
- Configuration parsing

### Integration Tests
- End-to-end sync flow
- Hybrid search queries
- Relationship creation
- Error recovery
- Performance benchmarks

### Load Tests
- 10K node sync
- Concurrent graph updates
- Hybrid search performance
- Webhook throughput

## Metrics

```
# Sync metrics
nexus_vectorizer_sync_total{label, status}
nexus_vectorizer_sync_duration_seconds{label}
nexus_vectorizer_sync_errors_total{label, error_type}

# Search metrics
nexus_hybrid_search_total{status}
nexus_hybrid_search_duration_seconds{component}

# Relationship metrics
nexus_similarity_edges_created_total
nexus_similarity_edges_score_distribution
```

## Security Considerations

1. **API Authentication**: Secure API key management
2. **Webhook Security**: HMAC signature verification
3. **Data Privacy**: Respect node-level permissions
4. **Rate Limiting**: Prevent sync storms
5. **Audit Logging**: Log all sync operations

## Migration Path

### Existing Graphs

```bash
# 1. Enable Vectorizer integration
curl -X POST http://localhost:15474/api/v1/sync/vectorizer/enable \
  -H "Content-Type: application/json" \
  -d '{"label": "Document", "collection": "documents"}'

# 2. Trigger batch sync
curl -X POST http://localhost:15474/api/v1/sync/vectorizer/reconcile \
  -H "Content-Type: application/json" \
  -d '{"label": "Document", "batch_size": 100}'

# 3. Build similarity graph
curl -X POST http://localhost:15474/cypher \
  -H "Content-Type: application/json" \
  -d '{
    "query": "CALL vector.build_similarity_graph(\"Document\", 0.75, 20)"
  }'
```

## Risks & Mitigation

| Risk | Impact | Mitigation |
|------|--------|------------|
| Vectorizer unavailability | High | Circuit breaker, fallback mode |
| Sync lag | Medium | Async processing, monitoring |
| Data inconsistency | High | Reconciliation tool, transactions |
| Performance degradation | Medium | Caching, batching, indexing |
| Embedding cost | Low | Cache embeddings, batch processing |

## Success Criteria

1. ✅ 99% of nodes synced within 10 seconds
2. ✅ Hybrid search < 100ms p95 latency
3. ✅ Zero data loss during sync
4. ✅ Support for 10M+ nodes with vectors
5. ✅ 95%+ test coverage
6. ✅ Bidirectional sync lag < 2 seconds

## Future Enhancements

1. **Multi-Modal Embeddings**: Support image, audio embeddings
2. **Dynamic Re-Embedding**: Trigger re-embedding on graph changes
3. **Federated Search**: Search across multiple Vectorizer instances
4. **Smart Caching**: Predictive caching of frequently accessed vectors
5. **ML-Enhanced Relationships**: Use ML to predict relationships

## References

- Vectorizer API Documentation: `vectorizer/README.md`
- Vectorizer MCP Integration: `vectorizer/docs/specs/MCP_INTEGRATION.md`
- Nexus Graph Correlation: `nexus/docs/specs/graph-correlation-analysis.md`
- Hybrid Search Algorithms: RRF, BM25+Vector

