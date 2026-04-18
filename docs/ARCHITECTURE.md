# Nexus Architecture

Nexus is a high-performance property graph database designed for read-heavy workloads with native vector search integration. It implements a Neo4j-inspired storage architecture with Cypher subset query language, optimized for KNN-seeded graph traversal.

## Design Philosophy

1. **Property Graph Model**: Nodes with labels, relationships with types, both carrying properties
2. **Read Optimized**: Record stores with linked lists for O(1) traversal without index lookups
3. **Vector Native**: First-class KNN support via HNSW indexes per label
4. **Simple Transactions**: MVCC via epochs, single-writer for predictability
5. **Append-Only**: Immutable record architecture with periodic compaction

## System Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                        REST/HTTP API                             │
│            (Cypher, KNN Traverse, Bulk Ingest)                  │
└────────────────────────┬────────────────────────────────────────┘
                         │
┌────────────────────────┴────────────────────────────────────────┐
│                    Cypher Executor                               │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐       │
│  │  Parser  │→ │ Planner  │→ │Operators │→ │ Results  │       │
│  └──────────┘  └──────────┘  └──────────┘  └──────────┘       │
│         Pattern Match • Expand • Filter • Project               │
└────────────────────────┬────────────────────────────────────────┘
                         │
┌────────────────────────┴────────────────────────────────────────┐
│                  Transaction Layer                               │
│  ┌──────────────────┐         ┌──────────────────┐             │
│  │  MVCC (Epochs)   │         │ Locking (Queues) │             │
│  └──────────────────┘         └──────────────────┘             │
└────────────────────────┬────────────────────────────────────────┘
                         │
┌────────────────────────┴────────────────────────────────────────┐
│                     Index Layer                                  │
│  ┌─────────┐  ┌─────────┐  ┌──────────┐  ┌──────────┐         │
│  │ Label   │  │ B-tree  │  │Tantivy   │  │  HNSW    │         │
│  │ Bitmap  │  │(Props)  │  │(FullText)│  │  (KNN)   │         │
│  └─────────┘  └─────────┘  └──────────┘  └──────────┘         │
└────────────────────────┬────────────────────────────────────────┘
                         │
┌────────────────────────┴────────────────────────────────────────┐
│                    Storage Layer                                 │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐       │
│  │  Catalog │  │   WAL    │  │  Record  │  │   Page   │       │
│  │  (LMDB)  │  │ (AppendJ)│  │  Stores  │  │  Cache   │       │
│  └──────────┘  └──────────┘  └──────────┘  └──────────┘       │
│   Labels/Keys    Durability   Nodes/Rels    Memory Mgmt        │
└─────────────────────────────────────────────────────────────────┘
```

## Storage Layer

### Record Stores (Neo4j-Inspired)

Fixed-size record architecture for predictable performance:

#### nodes.store

```
NodeRecord (32 bytes):
┌──────────────┬──────────────┬──────────────┬──────────────┐
│ label_bits   │ first_rel_ptr│  prop_ptr    │   flags      │
│   (8 bytes)  │  (8 bytes)   │  (8 bytes)   │  (8 bytes)   │
└──────────────┴──────────────┴──────────────┴──────────────┘

- label_bits: Bitmap of label IDs (supports 64 labels per node)
- first_rel_ptr: Head of doubly-linked relationship list
- prop_ptr: Pointer to property chain
- flags: Deleted, locked, version bits
```

#### rels.store

```
RelationshipRecord (48 bytes):
┌─────────┬─────────┬─────────┬────────────┬────────────┬──────────┬────────┐
│  src_id │ dst_id  │ type_id │next_src_ptr│next_dst_ptr│ prop_ptr │ flags  │
│(8 bytes)│(8 bytes)│(4 bytes)│  (8 bytes) │  (8 bytes) │(8 bytes) │(4 bytes)│
└─────────┴─────────┴─────────┴────────────┴────────────┴──────────┴────────┘

- Doubly-linked lists: next_src_ptr (outgoing from src), next_dst_ptr (incoming to dst)
- Enables O(1) traversal without index lookups
```

#### props.store

```
PropertyRecord (Variable):
┌──────────┬──────────┬──────────┬──────────┐
│  key_id  │  type    │  value   │ next_ptr │
│(4 bytes) │(1 byte)  │ (varies) │(8 bytes) │
└──────────┴──────────┴──────────┴──────────┘

- Chain of properties per entity
- Small values inline, large values in strings.store
- Types: null, bool, int64, float64, string_ref, bytes_ref
```

#### strings.store

```
String/Blob Dictionary:
┌──────────────┬──────────────┬──────────────┬──────────┐
│ varint_len   │    data      │    crc32     │  padding │
└──────────────┴──────────────┴──────────────┴──────────┘

- Deduplicated string/blob storage
- CRC32 for corruption detection
- Reference counted for garbage collection
```

### Catalog (LMDB via heed)

Bidirectional mappings stored in embedded key-value store:

```
Tables:
- label_name → label_id
- label_id → label_name
- type_name → type_id
- type_id → type_name
- key_name → key_id
- key_id → key_name

Metadata:
- Statistics (node count per label, relationship count per type)
- Schema constraints (UNIQUE, NOT NULL)
- Index definitions
```

### Page Cache

4-8KB pages with eviction policies:

```
Page Structure:
┌──────────┬──────────┬──────────┬──────────┐
│ page_id  │ checksum │   data   │  flags   │
│(8 bytes) │(4 bytes) │(varies)  │(4 bytes) │
└──────────┴──────────┴──────────┴──────────┘

Eviction Policies:
- Clock: Simple second-chance algorithm
- 2Q: Hot/cold queue split
- TinyLFU: Frequency + recency estimation

Pin/Unpin Semantics:
- Pinned pages cannot be evicted (active transactions)
- Dirty pages flushed on checkpoint
- xxHash3 checksums for validation
```

### Write-Ahead Log (WAL)

Append-only transaction log:

```
WAL Entry Format:
┌──────────┬──────────┬──────────┬──────────┬──────────┐
│  epoch   │  tx_id   │  type    │ payload  │  crc32   │
│(8 bytes) │(8 bytes) │(1 byte)  │(varies)  │(4 bytes) │
└──────────┴──────────┴──────────┴──────────┴──────────┘

Entry Types:
- BEGIN_TX, COMMIT_TX, ABORT_TX
- CREATE_NODE, DELETE_NODE
- CREATE_REL, DELETE_REL
- SET_PROPERTY, DELETE_PROPERTY
- CHECKPOINT

Checkpointing:
1. Freeze current epoch
2. Flush all dirty pages
3. Write CHECKPOINT entry
4. Truncate old WAL segments
5. Run compactor on record stores
```

## Transaction Layer

### MVCC via Epochs

```
Epoch-Based Snapshots:
- Global epoch counter (atomic u64)
- Readers pin an epoch (snapshot isolation)
- Writers increment epoch on commit
- Garbage collection removes old versions

Visibility Rules:
- Record visible if: created_epoch ≤ tx_epoch < deleted_epoch
- Append-only: new versions append, old kept until GC
```

### Locking Strategy

```
Single-Writer per Partition (MVP):
- Hash(node_id) → partition_id
- Queue per partition (FIFO)
- Group commit: batch small writes

Future (V2):
- Intent locks (read/write)
- Deadlock detection via wait-for graph
```

## Index Layer

### Label Bitmap Index

```
RoaringBitmap per Label:
label_id → RoaringBitmap<node_id>

- Compressed bitmap (runs, arrays, bitmaps)
- Fast AND/OR/NOT for label filtering
- Cardinality estimates for planner
```

### Property B-tree Index (V1)

```
Composite Key: (label_id, key_id, value) → [node_ids]

- Range queries: WHERE n.age > 18
- Equality: WHERE n.email = "user@example.com"
- Statistics: NDV (number distinct values), histograms
```

### Full-Text Index (V1)

```
Tantivy Index per (label, key):

- Inverted index with positions
- BM25 scoring
- Fuzzy search, phrase queries
- Procedure: CALL text.search('Person', 'bio', 'engineer')
```

### KNN Vector Index (MVP)

```
HNSW Index per Label:

File: hnsw_<label_id>.bin
Mapping: node_id → embedding_idx

- HNSW (Hierarchical Navigable Small World)
- Cosine similarity or Euclidean distance
- Configurable ef_construction, M parameters
- Procedure: CALL vector.knn('Person', $embedding, 10)
```

## Executor Layer

### Cypher Subset

Supported syntax (20% covering 80% use cases):

```cypher
-- Pattern matching
MATCH (n:Person)-[r:KNOWS]->(m:Person)
WHERE n.age > 25 AND m.city = 'NYC'
RETURN n.name, m.name, r.since
ORDER BY r.since DESC
LIMIT 100

-- KNN-seeded traversal
CALL vector.knn('Person', $embedding, 10) YIELD node AS n
MATCH (n)-[:WORKS_AT]->(c:Company)
RETURN n.name, c.name

-- Aggregations
MATCH (p:Person)-[:LIKES]->(product:Product)
RETURN product.category, COUNT(*) AS likes
ORDER BY likes DESC
```

### Physical Operators

```
Operator Pipeline:

1. NodeByLabel(label_id)
   → Scan label bitmap
   → Output: stream of node_ids

2. Filter(predicate)
   → Apply property filters
   → Vectorized evaluation (SIMD where possible)

3. Expand(type_id, direction)
   → Follow linked lists (next_src_ptr/next_dst_ptr)
   → Direction: OUT, IN, BOTH

4. Project(expressions)
   → Evaluate return expressions
   → Property access, functions

5. OrderBy(keys) + Limit(n)
   → Top-K heap for efficiency
   → Partial sort when LIMIT present

6. Aggregate(group_keys, agg_funcs)
   → Hash aggregation
   → COUNT, SUM, AVG, MIN, MAX, COLLECT
```

### Query Planner

```
Heuristic Cost-Based:

Statistics Used:
- |V_label|: Node count per label
- |E_type|: Relationship count per type
- avg_degree(label, type): Average out-degree
- NDV(label, key): Number distinct values

Optimization Rules:
1. Push filters down (early elimination)
2. Reorder patterns by selectivity (smallest first)
3. Index selection (label vs property vs KNN)
4. Limit pushdown for top-K queries

Example:
MATCH (a:Rare)-[:TYPE1]->(b:Common)-[:TYPE2]->(c)
→ Start with Rare (smaller cardinality)
→ Expand to Common, filter early
```

## Integration Layer

### StreamableHTTP API (Default Protocol)

**Primary protocol** following Vectorizer implementation:

```
Default Transport: StreamableHTTP
- Chunked transfer encoding for large result sets
- Server-Sent Events (SSE) for streaming
- HTTP/2 multiplexing and flow control
- Efficient for both small and large responses

Endpoints:

POST /cypher
{
  "query": "MATCH (n:Person) RETURN n LIMIT 10",
  "params": {"name": "Alice"}
}

POST /knn_traverse
{
  "label": "Person",
  "vector": [0.1, 0.2, ...],
  "k": 10,
  "expand": ["(n)-[:KNOWS]->(m)"],
  "where": "m.age > 25",
  "limit": 100
}

POST /ingest
{
  "nodes": [
    {"labels": ["Person"], "properties": {"name": "Alice", "age": 30}}
  ],
  "relationships": [
    {"src": 1, "dst": 2, "type": "KNOWS", "properties": {"since": 2020}}
  ]
}

Streaming Response (SSE):
event: row
data: {"node": {...}, "score": 0.95}

event: row
data: {"node": {...}, "score": 0.92}

event: complete
data: {"total": 100, "execution_time_ms": 15}
```

### MCP Protocol Integration

**Model Context Protocol** for AI integrations (Vectorizer-style):

```
MCP Tools (19+ focused tools):
- nexus/query - Execute Cypher queries
- nexus/knn_search - KNN vector search
- nexus/pattern_match - Graph pattern matching
- nexus/ingest_node - Create single node
- nexus/ingest_relationship - Create relationship
- nexus/get_schema - Get graph schema
- nexus/get_stats - Database statistics
- ... (expandable)

Benefits:
- Reduced entropy (no enum parameters)
- Tool-specific parameters only
- Better model tool calling accuracy
```

### UMICP Protocol Integration

**Universal Model Interoperability Protocol** following Vectorizer v0.2.1:

```
UMICP Features:
- Native JSON types support
- Tool Discovery endpoint: GET /umicp/discover
- Exposes all MCP tools with full schemas
- Cross-service graph queries
- Event-driven graph updates

Discovery Response:
{
  "service": "nexus-graph",
  "version": "0.1.0",
  "protocol": "UMICP/0.2.1",
  "tools": [
    {
      "name": "graph.query",
      "description": "Execute Cypher query",
      "inputSchema": {...}
    }
  ]
}
```

### Vectorizer Integration (Native)

**Direct integration** with Vectorizer for hybrid search:

```
Integration Modes:

1. Embedding Generation:
   POST /vectorizer/embed → vector
   Store vector in Nexus KNN index

2. Hybrid Search:
   Nexus KNN search → node_ids
   Vectorizer semantic search → enhanced relevance
   Combined ranking via Reciprocal Rank Fusion (RRF)

3. Bidirectional Sync:
   Vectorizer change → Nexus graph update (relationship creation)
   Nexus mutation → Vectorizer re-index (embedding update)

Example Flow:
┌────────────┐      embed()      ┌────────────┐
│ Vectorizer │◄─────────────────│   Nexus    │
│            │────────────────►  │            │
└────────────┘   vector result   └────────────┘
      │                                │
      │ semantic search                │ graph traversal
      ▼                                ▼
   Relevance scores              Relationship context
      │                                │
      └────────────┬───────────────────┘
                   │
              Combined Results
           (RRF ranking algorithm)
```

## Performance Characteristics

### Read Performance

```
- Node lookup by ID: O(1) - direct offset
- Expand neighbors: O(degree) - linked list traversal
- Pattern match: O(|V_start| × selectivity × expansions)
- KNN search: O(log N) - HNSW logarithmic
- Full scan: O(|V|) - bitmap scan

Target Throughput (Single Node):
- Point reads: 100K+ ops/sec
- KNN queries: 10K+ ops/sec
- Pattern traversal: 1K-10K ops/sec (depth dependent)
```

### Write Performance

```
- Insert node: O(1) - append to nodes.store
- Insert relationship: O(1) - append + update pointers
- Update property: O(props_per_entity) - traverse chain
- Bulk ingest: Batch via WAL, bypass cache

Target Throughput (Single Writer):
- Inserts: 10K-50K ops/sec
- Updates: 5K-20K ops/sec
- Bulk load: 100K+ nodes/sec (direct store generation)
```

### Space Overhead

```
Per Node: ~32 bytes (record) + properties + index entries
Per Relationship: ~48 bytes + properties
Property: ~16 bytes + value size
Index Overhead:
- Label bitmap: ~0.1-1 byte per node (compressed)
- HNSW vector: ~100-200 bytes per vector (M=16)

Example (1M nodes, 2M relationships, avg 5 props):
- Nodes: 32MB
- Relationships: 96MB
- Properties: ~160MB (assuming 20 bytes avg per prop)
- Indexes: ~50MB (bitmaps + HNSW)
Total: ~340MB (reasonable)
```

## Authentication & Security

### API Key Authentication

**Vectorizer-style authentication** with flexible configuration:

```
Configuration:
- Default: Authentication DISABLED (localhost development)
- Production: REQUIRED when binding to 0.0.0.0 (public interface)
- API Keys: 32-character random strings
- Storage: Hashed with Argon2 in catalog (LMDB)

AuthConfig:
{
  "enabled": false,                    // Default: disabled for localhost
  "require_for_public_bind": true,     // Force enable for 0.0.0.0
  "api_key_length": 32,
  "rate_limit_per_minute": 1000,
  "rate_limit_per_hour": 10000,
  "jwt_secret": "change-in-production",
  "jwt_expiration": 3600
}

API Key Format:
{
  "id": "key_abc123",
  "name": "Production App",
  "key_hash": "argon2id$...",
  "user_id": "admin",
  "permissions": ["read", "write", "admin"],
  "created_at": 1704067200,
  "expires_at": null,  // Never expires
  "active": true
}

Usage:
Authorization: Bearer nexus_sk_abc123...xyz

Rate Limiting:
- 1000 requests/minute per API key
- 10000 requests/hour per API key
- 429 Too Many Requests on exceed
- X-RateLimit-* headers in response
```

### Security Model (V1)

```
Permissions:
- READ: Query execution (MATCH, CALL vector.knn)
- WRITE: Data mutations (CREATE, SET, DELETE)
- ADMIN: Index management, constraints, schema changes
- SUPER: Replication, cluster management

Role-Based Access Control (RBAC):
- User → Roles → Permissions
- API Key → Permissions (direct)
- JWT tokens for session management
- Audit logging for all write operations

Transport Security:
- TLS 1.3 for production (via Axum/Tower)
- mTLS for service-to-service (V2)
- Certificate-based authentication (optional)
```

## Replication System (V1)

### Master-Replica Architecture

**Inspired by Redis/Vectorizer replication** with graph-specific optimizations:

```
Topology:
┌────────────────┐         ┌────────────────┐
│  Master Node   │────────>│ Replica Node 1 │
│  (Read+Write)  │         │  (Read-Only)   │
└────────┬───────┘         └────────────────┘
         │
         └───────────────> ┌────────────────┐
                           │ Replica Node 2 │
                           │  (Read-Only)   │
                           └────────────────┘

Replication Flow:
1. Master receives write
2. Append to WAL
3. Apply to local storage
4. Stream WAL entry to replicas
5. Replicas apply + ACK
6. Master commits transaction
```

### Replication Modes

```
1. Full Sync (Initial):
   - Master creates snapshot
   - Transfer snapshot.tar.zst to replica
   - CRC32 checksum verification
   - Replica loads snapshot
   - Switch to incremental sync

2. Incremental Sync:
   - Stream WAL entries to replicas
   - Circular replication log (1M operations)
   - Auto-reconnect with exponential backoff
   - Lag monitoring and alerts

3. Async Replication:
   - Master doesn't wait for replica ACK (default)
   - Higher throughput, eventual consistency
   - Configurable ACK timeout

4. Sync Replication (optional):
   - Wait for N replicas to ACK
   - Lower throughput, stronger durability
   - Configurable quorum
```

### Replication Protocol

```
WAL Streaming:
┌──────────────────────────────────────┐
│  Master WAL Entry                     │
│  {epoch, tx_id, type, payload, crc}  │
└──────────────┬───────────────────────┘
               │ TCP stream
┌──────────────▼───────────────────────┐
│  Replica Receiver                     │
│  - Validate CRC                       │
│  - Apply to local WAL                 │
│  - Update storage                     │
│  - Send ACK                           │
└──────────────────────────────────────┘

REST API Endpoints:
- GET  /replication/status
- POST /replication/promote   (replica → master)
- POST /replication/pause
- POST /replication/resume
- GET  /replication/lag       (replication lag in seconds)
```

### Failover & High Availability

```
Automatic Failover (V1):
1. Monitor master health (heartbeat every 5s)
2. Detect master failure (3 missed heartbeats)
3. Elect new master (manual or via consensus)
4. Promote replica: POST /replication/promote
5. Redirect clients to new master
6. Old master rejoins as replica (when recovered)

Manual Failover:
curl -X POST http://replica:15474/replication/promote \
  -H "Authorization: Bearer admin_key"
```

## Sharding & Distribution (V2)

### Sharding Strategy

**Hash-based partitioning** for horizontal scalability:

```
Shard Assignment:
shard_id = hash(node_id) % num_shards

Example (4 shards):
- Shard 0: node_ids where hash % 4 == 0
- Shard 1: node_ids where hash % 4 == 1
- Shard 2: node_ids where hash % 4 == 2
- Shard 3: node_ids where hash % 4 == 3

Relationship Placement:
- Relationships reside with source node
- Cross-shard edges stored as remote pointers
- Minimize cross-shard hops in queries

Topology:
┌──────────────┐  ┌──────────────┐  ┌──────────────┐
│   Shard 0    │  │   Shard 1    │  │   Shard 2    │
│  (Master)    │  │  (Master)    │  │  (Master)    │
└──────┬───────┘  └──────┬───────┘  └──────┬───────┘
       │                 │                 │
   ┌───┴────┐        ┌───┴────┐        ┌───┴────┐
   │Replica1│        │Replica1│        │Replica1│
   └────────┘        └────────┘        └────────┘
```

### Distributed Queries

```
Query Coordinator:
1. Parse Cypher query
2. Identify required shards (WHERE clause analysis)
3. Decompose plan into shard-local subplans
4. Execute in parallel (scatter)
5. Merge results (gather)
6. Apply global ORDER BY + LIMIT

Optimizations:
- Pushdown filters to shards
- Pushdown LIMIT (top-K per shard)
- Minimize data transfer
- Cache remote node metadata

Cross-Shard Traversal:
MATCH (n:Person)-[:KNOWS]->(m:Person)
→ If n and m on different shards:
  1. Execute on n's shard
  2. Fetch m's metadata remotely
  3. Continue traversal
  4. Cache cross-shard edges
```

### Consensus & Coordination

```
Raft Consensus (via openraft):
- One Raft group per shard
- Leader handles all writes
- Followers replicate via Raft log
- Strong consistency per shard
- Eventual consistency cross-shard

Shard Metadata (in catalog):
{
  "shard_id": 0,
  "leader": "node1:15474",
  "followers": ["node2:15474", "node3:15474"],
  "status": "healthy",
  "node_count": 250000,
  "rel_count": 500000
}
```

## Desktop GUI (Electron)

### Overview

**Modern desktop application** for visual graph management and exploration:

```
Technology Stack:
- Electron (cross-platform desktop)
- Vue 3 + Composition API
- TailwindCSS (styling)
- D3.js / Cytoscape.js (graph visualization)
- Chart.js (metrics)

Features:
🎨 Beautiful interface with dark/light themes
📊 Real-time graph visualization (force-directed layout)
🔍 Visual Cypher query builder
⚡ Live query execution with syntax highlighting
📈 Database metrics and monitoring
💾 Backup/restore operations
🔧 Configuration editor
📁 Schema browser (labels, types, properties)
🎯 KNN vector search interface
```

### GUI Capabilities

```
1. Graph Visualization:
   - Force-directed graph layout
   - Node filtering by label
   - Relationship filtering by type
   - Property inspector
   - Zoom, pan, node selection

2. Query Interface:
   - Cypher editor with syntax highlighting
   - Query history
   - Result table/graph view toggle
   - Export results (JSON, CSV)
   - Saved queries

3. Schema Management:
   - View all labels and types
   - Property statistics
   - Index management
   - Constraint creation/deletion

4. KNN Vector Search:
   - Text input → generate embedding
   - Visual similarity search results
   - Hybrid query builder (KNN + patterns)
   - Vector index management

5. Database Operations:
   - Backup/restore
   - Import/export data
   - Replication monitoring
   - Performance metrics
   - Log viewer

6. Monitoring Dashboard:
   - Query throughput
   - Page cache hit rate
   - WAL size
   - Replication lag
   - Index sizes
```

### Installation & Usage

```bash
# Development
cd gui
npm install
npm run dev

# Build installers
npm run build:win     # Windows MSI
npm run build:mac     # macOS DMG
npm run build:linux   # Linux AppImage/DEB

# Run packaged app
./dist/Nexus-Setup-0.1.0.exe       # Windows
./dist/Nexus-0.1.0.dmg             # macOS
./dist/nexus_0.1.0_amd64.AppImage  # Linux
```

### GUI Architecture

```
┌─────────────────────────────────────────┐
│          Electron Main Process          │
│  - Window management                    │
│  - Auto-updater                         │
│  - File system access                   │
└─────────────┬───────────────────────────┘
              │ IPC
┌─────────────▼───────────────────────────┐
│        Electron Renderer Process        │
│  ┌────────────────────────────────────┐ │
│  │  Vue 3 Application                 │ │
│  │  - Graph Visualization (Cytoscape) │ │
│  │  - Query Editor (CodeMirror)       │ │
│  │  - Dashboard (Chart.js)            │ │
│  └──────────┬─────────────────────────┘ │
└─────────────┼───────────────────────────┘
              │ HTTP/WebSocket
┌─────────────▼───────────────────────────┐
│         Nexus Server (Axum)              │
│    http://localhost:15474                │
└──────────────────────────────────────────┘
```

## Scalability Path

### V1: Single Node Optimization

- Batch optimizations (vectorization, SIMD)
- Advanced indexes (B-tree, full-text)
- Query cache (prepared statements)
- Read replicas (WAL streaming)

### V2: Distributed Graph

```
Sharding Strategy:
- Hash(node_id) → shard_id
- Relationships reside with source node
- Cross-shard edges via remote pointers

Replication:
- Raft consensus per shard (openraft)
- Leader handles writes, followers serve reads
- Causal consistency via vector clocks

Distributed Queries:
- Coordinator decomposes plan
- Pushdown filters/limits to shards
- Scatter/gather with streaming results
- Minimize cross-shard hops
```

## Comparison with Neo4j

| Feature | Neo4j | Nexus |
|---------|-------|-------|
| Storage | Record stores + page cache | Same approach |
| Query Language | Full Cypher | Cypher subset (20%) |
| Transactions | ACID, full MVCC | Simplified MVCC (epochs) |
| Indexes | B-tree, full-text, native | Same + native KNN (HNSW) |
| Clustering | Causal cluster | Future (openraft) |
| Vector Search | Plugin (GDS) | Native first-class |
| Target Workload | General graph | Read-heavy + RAG |

## Future Enhancements

- **Temporal Graph**: Valid-time versioning for time-travel queries
- **Geospatial**: PostGIS-like spatial indexes and functions
- **Graph Algorithms**: Native BFS, DFS, PageRank, community detection
- **Streaming Ingestion**: Kafka/Pulsar integration
- **Advanced Analytics**: Integration with Apache Arrow for OLAP

## Graph Correlation Analysis Architecture

### Overview

The Graph Correlation Analysis module provides automatic generation of code relationship graphs from source code, enabling visualization of code flow, dependencies, and processing patterns. It integrates with Vectorizer for semantic code analysis and provides multiple protocol interfaces (REST, MCP, UMICP).

### System Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                    API Layer (REST/MCP/UMICP)                        │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐              │
│  │ REST API     │  │ MCP Tools   │  │ UMICP Handler│              │
│  │ /api/v1/     │  │ graph_*     │  │ graph.*      │              │
│  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘              │
└─────────┼──────────────────┼──────────────────┼────────────────────┘
          │                  │                  │
┌─────────┴──────────────────┴──────────────────┴────────────────────┐
│              Graph Correlation Manager                               │
│  ┌──────────────────────────────────────────────────────────────┐  │
│  │  GraphCorrelationManager                                      │  │
│  │  - Builder Registry (Call, Dependency, DataFlow, Component)  │  │
│  │  - Graph Storage & Retrieval                                  │  │
│  │  - Request Routing                                            │  │
│  └──────────────────────────────────────────────────────────────┘  │
└─────────┬──────────────────────────────────────────────────────────┘
          │
┌─────────┴──────────────────────────────────────────────────────────┐
│                    Graph Builder Layer                              │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐            │
│  │ Call Graph   │  │ Dependency   │  │ Data Flow    │            │
│  │ Builder      │  │ Graph Builder│  │ Graph Builder│            │
│  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘            │
│         │                  │                  │                    │
│  ┌──────┴──────────────────┴──────────────────┴──────┐          │
│  │ Component Graph Builder                              │          │
│  └──────────────────────────────────────────────────────┘          │
└─────────┬──────────────────────────────────────────────────────────┘
          │
┌─────────┴──────────────────────────────────────────────────────────┐
│                    Analysis Layer                                   │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐            │
│  │ Pattern      │  │ Statistics   │  │ Impact       │            │
│  │ Recognition  │  │ Calculator   │  │ Analysis     │            │
│  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘            │
│         │                  │                  │                    │
│  ┌──────┴──────────────────┴──────────────────┴──────┐          │
│  │  Pattern Detectors:                                │          │
│  │  - PipelinePatternDetector                         │          │
│  │  - EventDrivenPatternDetector                      │          │
│  │  - ArchitecturalPatternDetector                    │          │
│  │  - DesignPatternDetector (Observer, Factory, etc.) │          │
│  └──────────────────────────────────────────────────────┘          │
└─────────┬──────────────────────────────────────────────────────────┘
          │
┌─────────┴──────────────────────────────────────────────────────────┐
│                    Visualization Layer                              │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐            │
│  │ SVG Renderer │  │ Layout      │  │ Export       │            │
│  │              │  │ Algorithms  │  │ Formats      │            │
│  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘            │
│         │                  │                  │                    │
│  ┌──────┴──────────────────┴──────────────────┴──────┐          │
│  │  Layouts: Force-directed, Hierarchical, Flow-based │          │
│  │  Formats: JSON, GraphML, GEXF, DOT, SVG, PNG, PDF │          │
│  └──────────────────────────────────────────────────────┘          │
└─────────┬──────────────────────────────────────────────────────────┘
          │
┌─────────┴──────────────────────────────────────────────────────────┐
│                    Data Source Layer                                │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐            │
│  │ Vectorizer   │  │ Source Code  │  │ AST Parser  │            │
│  │ Integration  │  │ Files        │  │             │            │
│  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘            │
│         │                  │                  │                    │
│  ┌──────┴──────────────────┴──────────────────┴──────┐          │
│  │  GraphSourceData: files, functions, imports       │          │
│  └──────────────────────────────────────────────────────┘          │
└────────────────────────────────────────────────────────────────────┘
```

### Component Architecture

#### Graph Builder Pattern

```
┌─────────────────────────────────────────────────────────────┐
│                    GraphBuilder Trait                       │
│  ┌───────────────────────────────────────────────────────┐ │
│  │  build(source_data) -> CorrelationGraph              │ │
│  │  graph_type() -> GraphType                            │ │
│  │  name() -> &str                                       │ │
│  │  capabilities() -> GraphBuilderCapabilities          │ │
│  └───────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────┘
                    ▲
                    │ implements
    ┌───────────────┼───────────────┬───────────────┐
    │               │               │               │
┌───┴────┐  ┌──────┴──────┐  ┌────┴──────┐  ┌────┴──────┐
│ Call   │  │ Dependency  │  │ DataFlow  │  │ Component │
│ Graph  │  │ Graph       │  │ Graph     │  │ Graph     │
│ Builder│  │ Builder     │  │ Builder   │  │ Builder   │
└────────┘  └─────────────┘  └───────────┘  └───────────┘
```

#### Pattern Detection Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                 PatternDetector Trait                       │
│  ┌───────────────────────────────────────────────────────┐ │
│  │  detect(graph) -> PatternDetectionResult              │ │
│  │  name() -> &str                                       │ │
│  │  supported_patterns() -> Vec<PatternType>            │ │
│  └───────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────┘
                    ▲
                    │ implements
    ┌───────────────┼───────────────┬───────────────┐
    │               │               │               │
┌───┴────┐  ┌──────┴──────┐  ┌────┴──────┐  ┌────┴──────┐
│Pipeline│  │Event-Driven │  │Architect. │  │Design     │
│Pattern │  │Pattern      │  │Pattern    │  │Pattern    │
│Detector│  │Detector     │  │Detector   │  │Detector   │
└────────┘  └─────────────┘  └───────────┘  └───────────┘
```

### Data Flow Architecture

```
Source Code Files
       │
       ▼
┌─────────────────┐
│ GraphSourceData │
│  - files        │
│  - functions    │
│  - imports      │
└────────┬────────┘
         │
         ▼
┌─────────────────────────────────────┐
│   Graph Builder (by type)          │
│  ┌───────────────────────────────┐ │
│  │ 1. Extract relationships      │ │
│  │ 2. Create nodes & edges       │ │
│  │ 3. Add metadata               │ │
│  │ 4. Calculate layout            │ │
│  └───────────────────────────────┘ │
└────────┬────────────────────────────┘
         │
         ▼
┌─────────────────┐
│ CorrelationGraph│
│  - nodes        │
│  - edges        │
│  - metadata     │
└────────┬────────┘
         │
         ├─────────────────┬─────────────────┬─────────────────┐
         ▼                 ▼                 ▼                 ▼
┌──────────────┐  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐
│ Pattern      │  │ Statistics  │  │ Visualization│  │ Export      │
│ Detection    │  │ Calculation │  │ Rendering   │  │ Formats     │
└──────────────┘  └──────────────┘  └──────────────┘  └──────────────┘
```

### Module Structure

```
nexus-core/src/graph/correlation/
├── mod.rs                    # Core types, GraphBuilder trait, Manager
├── call_graph.rs            # Call graph generation
├── dependency.rs            # Dependency graph generation
├── data_flow.rs             # Data flow analysis & graph generation
├── component.rs             # Component (OOP) analysis & graph generation
├── pattern_recognition.rs   # Pattern detection (Pipeline, Event, Arch, Design)
├── visualization.rs         # SVG rendering, layouts, styling
├── graph_export.rs          # Export to JSON, GraphML, GEXF, DOT
├── graph_statistics.rs      # Statistics calculation
├── graph_diff.rs            # Graph comparison & diff
├── performance.rs            # Performance optimization
├── dependency_filter.rs     # Dependency filtering utilities
├── impact_analysis.rs       # Impact analysis for changes
├── vectorizer_cache.rs      # Vectorizer query caching
└── version_constraints.rs   # Version constraint analysis
```

### Integration Points

```
┌─────────────────────────────────────────────────────────────┐
│                    External Systems                         │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐    │
│  │ Vectorizer   │  │ LLM Agents   │  │ Visualization│    │
│  │ MCP Server   │  │ (via MCP)    │  │ Tools        │    │
│  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘    │
└─────────┼──────────────────┼──────────────────┼────────────┘
          │                  │                  │
          │ MCP Tools        │ REST API         │ Export Files
          │                  │                  │
┌─────────┴──────────────────┴──────────────────┴────────────┐
│         Graph Correlation Analysis Module                   │
│  ┌──────────────────────────────────────────────────────┐  │
│  │  Protocol Handlers:                                  │  │
│  │  - MCP: graph_correlation_* tools                   │  │
│  │  - REST: /api/v1/graphs/* endpoints                 │  │
│  │  - UMICP: graph.* methods                           │  │
│  └──────────────────────────────────────────────────────┘  │
└────────────────────────────────────────────────────────────┘
```

### Pattern Recognition Flow

```
CorrelationGraph
       │
       ▼
┌─────────────────────────────────────┐
│   Pattern Detection Pipeline        │
│  ┌───────────────────────────────┐  │
│  │ 1. Pipeline Pattern Detector  │  │
│  │ 2. Event-Driven Detector      │  │
│  │ 3. Architectural Detector     │  │
│  │ 4. Design Pattern Detector    │  │
│  └───────────────────────────────┘  │
└────────┬─────────────────────────────┘
         │
         ▼
┌─────────────────────────────────────┐
│   Pattern Detection Result           │
│  - DetectedPattern[]                 │
│  - PatternStatistics                 │
│  - Quality Score                     │
└────────┬─────────────────────────────┘
         │
         ├─────────────────┬─────────────────┐
         ▼                 ▼                 ▼
┌──────────────┐  ┌──────────────┐  ┌──────────────┐
│ Quality      │  │ Visualization│  │ Recommendation│
│ Metrics      │  │ Overlays     │  │ Engine       │
└──────────────┘  └──────────────┘  └──────────────┘
```

### Key Design Decisions

1. **Trait-Based Architecture**: Uses Rust traits (`GraphBuilder`, `PatternDetector`) for extensibility
2. **Type-Safe Graph Types**: Enum-based graph types prevent invalid operations
3. **Modular Analysis**: Separate modules for each analysis type (call, dependency, data flow, component)
4. **Protocol Agnostic**: Core logic independent of API protocol (REST/MCP/UMICP)
5. **Caching Support**: Vectorizer query caching for performance
6. **Export Flexibility**: Multiple export formats for different use cases
7. **Pattern Extensibility**: Easy to add new pattern detectors via trait implementation

## Server State Ownership (`NexusServer`)

Every piece of shared server state — the engine, the shared executor,
the multi-database manager, RBAC, the auth/JWT/audit chain, and the
root-user config — lives on the `NexusServer` struct in
[`nexus-server/src/lib.rs`](../nexus-server/src/lib.rs).
`NexusServer::new` builds the struct once inside `main.rs::async_main`,
wraps it in an `Arc`, and hands the same `Arc<NexusServer>` to Axum via
`.with_state(...)`.

HTTP handlers reach the state through Axum's `State` extractor:

```rust
pub async fn create_node(
    State(server): State<Arc<NexusServer>>,
    Json(request): Json<CreateNodeRequest>,
) -> Json<CreateNodeResponse> {
    let mut engine = server.engine.write().await;
    // ...
}
```

This replaces an older pattern that kept every subsystem in its own
`OnceLock<Arc<_>>` per API file (`api::cypher::EXECUTOR`,
`api::data::ENGINE`, …), which broke test isolation — two tests that
each called `init_engine` silently collided on the same process-wide
singleton. The migration is sliced into `phase2a`–`phase2e`; phase2a
landed cypher + data, phase2b landed schema + stats + knn, phase2c
landed the performance-monitoring axis (query statistics, plan cache,
DBMS procedures, MCP tool statistics, MCP tool cache), and phase2d
lands graph correlation + comparison + UMICP: the
`GraphCorrelationManager` (shared correlation-graph builder), the two
comparison `Graph` instances, and the `GraphUmicpHandler` (JSON-RPC
dispatcher for `graph.*` methods) now hang off `NexusServer`. `main.rs`
no longer provisions temp dirs or calls `init_graphs` /
`init_manager` / `init_umicp_handler`; those helpers and their
`OnceLock`s are gone. Default comparison graphs are built inside
`NexusServer::new` via the private `build_default_comparison_graphs`
helper, which roots each graph at its own fresh temp dir and falls
back to `std::env::temp_dir()` if `tempfile::tempdir()` fails. To add
a new dimension: add an `Arc<_>` field to `NexusServer`, build it
inside `NexusServer::new` with whatever defaults apply, and read it
from handlers via `server.<field>` — do not reach for a `OnceLock`.
The final phase2e slice migrates health + prometheus and adds a
`tests/no_oncelock_globals.rs` guard that fails if the anti-pattern
comes back.

## References

- Neo4j Internals: https://neo4j.com/docs/operations-manual/current/
- HNSW Algorithm: https://arxiv.org/abs/1603.09320
- MVCC in Databases: Postgres, CockroachDB documentation
- Roaring Bitmaps: https://roaringbitmap.org/
- Tantivy: https://github.com/quickwit-oss/tantivy

