# Vectorizer Integration - Proposal

## Why

Enable bidirectional synchronization between Nexus graph database and Vectorizer to add semantic search capabilities, auto-relationship discovery, and hybrid search combining graph traversal with semantic similarity. This integration addresses Nexus's lack of native embedding-based similarity search and enables automatic relationship discovery based on semantic similarity.

## What Changes

### Core Integration
- **Vectorizer Client**: HTTP client for Vectorizer REST API with retry logic and circuit breaker
- **Sync Coordinator**: Event-driven coordinator for bidirectional sync between graph and Vectorizer
- **Webhook System**: Receive and process Vectorizer events (similarity notifications, document updates)
- **Configuration System**: Configuration management for Vectorizer integration settings

### Bidirectional Synchronization
- **Graph Event Hooks**: Listen for node/relationship create/update/delete events
- **Automatic Vector Creation**: Create vectors in Vectorizer when nodes are created/updated
- **Context Enrichment**: Enhance embeddings with graph relationship context
- **Relationship Synchronization**: Sync relationships bidirectionally with Vectorizer metadata

### Hybrid Search
- **Semantic Search Procedure**: `CALL vector.semantic_search()` Cypher procedure
- **Hybrid Search**: `CALL vector.hybrid_search()` combining graph traversal with semantic search using RRF
- **Similarity Edge Builder**: `CALL vector.build_similarity_graph()` to create SIMILAR_TO relationships
- **Cross-Collection Search**: Search across multiple Vectorizer collections

### Production Features
- **Monitoring & Metrics**: Prometheus metrics for sync operations, search performance, error rates
- **Admin API**: REST endpoints for sync control (enable/disable, status, reconcile)
- **Reconciliation Tool**: Detect and repair inconsistencies between graph and Vectorizer
- **Performance Optimization**: Caching, batching, connection pooling

## Impact

### Affected Specs
- Cypher procedures (`specs/cypher-procedures/spec.md`)
- Graph storage engine (`specs/storage/spec.md`)
- API endpoints (`specs/api/spec.md`)
- Event system (`specs/events/spec.md`)

### Affected Code
- `nexus-core/src/vectorizer_client/` - New module for Vectorizer API client
- `nexus-core/src/vectorizer_sync/` - New module for sync coordinator
- `nexus-core/src/storage/mod.rs` - Add event hooks for sync
- `nexus-core/src/executor/procedures/vector.rs` - New Cypher procedures
- `nexus-server/src/api/webhooks/` - New webhook handlers
- `nexus-server/src/api/admin/` - Admin API endpoints
- `nexus-server/src/config.rs` - Add VectorizerIntegrationConfig

### Breaking Changes
- **None** - All changes are additive, maintaining backward compatibility

## API Changes

### New Cypher Procedures
```cypher
// Semantic search across Vectorizer collections
CALL vector.semantic_search($query_text, $collections, $k, $filters)
YIELD node, score, collection

// Hybrid search (graph + semantic)
CALL vector.hybrid_search($query_text, $graph_pattern, $k, $rrf_k)
YIELD node, graph_score, semantic_score, combined_score

// Build similarity graph
CALL vector.build_similarity_graph($label, $threshold, $max_edges_per_node)
YIELD relationships_created, execution_time
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

## Success Metrics

- **Sync Performance**: 99% of nodes synced within 10 seconds
- **Search Latency**: Hybrid search < 100ms p95 latency
- **Data Consistency**: Zero data loss during sync
- **Scalability**: Support for 10M+ nodes with vectors
- **Test Coverage**: 95%+ test coverage
- **Sync Lag**: Bidirectional sync lag < 2 seconds

## Implementation Tasks

See `tasks.md` for detailed task breakdown and progress tracking.

## Risks & Mitigations

- **Risk**: Vectorizer unavailability causing sync failures
  - **Mitigation**: Circuit breaker, graceful degradation, queue for later sync
  
- **Risk**: Sync lag affecting data consistency
  - **Mitigation**: Async processing, monitoring, reconciliation tool
  
- **Risk**: Performance degradation from sync overhead
  - **Mitigation**: Batching, caching, connection pooling, async processing
  
- **Risk**: Embedding costs from frequent re-embedding
  - **Mitigation**: Cache embeddings, incremental updates, batch processing

## Timeline

- **Weeks 1-2**: Core infrastructure (Vectorizer client, sync coordinator, webhooks)
- **Weeks 3-4**: Bidirectional sync (event hooks, automatic vector creation, relationship sync)
- **Weeks 5-6**: Hybrid search (semantic search procedure, RRF, similarity edge builder)
- **Weeks 7-8**: Production features (monitoring, admin API, reconciliation tool, documentation)

**Total**: 10-11 weeks

## Dependencies

- Vectorizer server (v1.1.2+)
- Nexus core (v0.8.0+)
- Rust 1.85+ (edition 2024)
- HTTP client library (reqwest) for Vectorizer API calls

## Configuration

```yaml
vectorizer_integration:
  enabled: true
  vectorizer_url: "http://localhost:15002"
  api_key: "${VECTORIZER_API_KEY}"
  sync_mode: "async"
  auto_create_vectors: true
  collections:
    Document:
      collection: "documents"
      enabled: true
  enrichment:
    enabled: true
    include_relationships: true
    max_relationship_depth: 2
  similarity:
    auto_create_edges: true
    threshold: 0.75
    max_edges_per_node: 20
```

## Next Steps

1. Review and approve proposal
2. Begin Phase 1: Core infrastructure implementation
3. Set up Vectorizer test environment
4. Create integration test suite
5. Implement monitoring and metrics from the start
