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

### REST/HTTP API

```
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

Streaming:
- Server-Sent Events (SSE) for large result sets
- Chunked transfer encoding
- Backpressure via HTTP/2 flow control
```

### MCP/UMICP Integration

```
Protocol Adapters:

MCP (Model Context Protocol):
- Semantic search over graph structure
- LLM-friendly result serialization
- Context window management

UMICP (Universal Model Interoperability):
- Cross-model communication
- Graph as shared knowledge base
- Event-driven updates
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

## References

- Neo4j Internals: https://neo4j.com/docs/operations-manual/current/
- HNSW Algorithm: https://arxiv.org/abs/1603.09320
- MVCC in Databases: Postgres, CockroachDB documentation
- Roaring Bitmaps: https://roaringbitmap.org/
- Tantivy: https://github.com/quickwit-oss/tantivy

