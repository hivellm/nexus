# KNN Integration Specification

This document defines the native K-Nearest Neighbors (KNN) vector search integration in Nexus.

## Overview

Nexus provides **first-class vector search** via HNSW (Hierarchical Navigable Small World) indexes, enabling:
- **Hybrid RAG queries**: Combine vector similarity with graph traversal
- **Per-label indexes**: Separate vector space for each node label
- **Native procedures**: `CALL vector.knn()` as part of Cypher queries
- **High performance**: Logarithmic search with approximate nearest neighbors

## Architecture

```
┌─────────────────────────────────────────────┐
│            Cypher Executor                   │
│  CALL vector.knn('Person', $vec, 10)        │
└─────────────┬───────────────────────────────┘
              │
┌─────────────▼───────────────────────────────┐
│         KNN Procedure Handler                │
│  - Parse label, vector, k                   │
│  - Route to appropriate index               │
│  - Convert results to nodes                 │
└─────────────┬───────────────────────────────┘
              │
┌─────────────▼───────────────────────────────┐
│          HNSW Index (per label)             │
│  File: indexes/hnsw_<label_id>.bin          │
│  - Graph layers (M connections)             │
│  - Greedy search from entry point           │
│  - ef_search parameter for quality/speed    │
└─────────────┬───────────────────────────────┘
              │
┌─────────────▼───────────────────────────────┐
│       Vector Storage (dense f32)            │
│  - Packed array: [vec0, vec1, ..., vecN]   │
│  - Mapping: node_id → embedding_idx         │
└──────────────────────────────────────────────┘
```

## HNSW Index Format

### File Structure

```
indexes/hnsw_<label_id>.bin:

┌────────────────┐
│  Header (64B)  │  Magic, version, dimension, M, ef_construction
├────────────────┤
│  Graph Layers  │  HNSW hierarchical graph structure
├────────────────┤
│  Vectors (raw) │  f32 array: dimension × node_count × 4 bytes
├────────────────┤
│  Mapping       │  node_id → embedding_idx pairs (sorted)
└────────────────┘
```

### Header Format

```
Header (64 bytes):
┌──────────┬──────────┬──────────┬──────────┬──────────┬──────────┬──────────┬──────────┐
│  magic   │ version  │dimension │    M     │ef_constr │ metric   │node_count│ reserved │
│(8 bytes) │(4 bytes) │(4 bytes) │(4 bytes) │(4 bytes) │(1 byte)  │(8 bytes) │(31 bytes)│
└──────────┴──────────┴──────────┴──────────┴──────────┴──────────┴──────────┴──────────┘

magic:         0x4E455855534B4E4E ("NEXUSKNN")
version:       1 (u32)
dimension:     Vector dimension, e.g., 384, 768, 1536 (u32)
M:             Max connections per layer (typically 16-48) (u32)
ef_construction: Build quality (typically 200-400) (u32)
metric:        Distance metric: 0=cosine, 1=euclidean (u8)
node_count:    Number of indexed vectors (u64)
reserved:      Future use (31 bytes, all zeros)
```

### Vector Storage

```
Vectors are stored as packed f32 arrays:

Offset: 64 (after header) + graph_size
Size: dimension × node_count × 4 bytes

Layout (for dimension=384, node_count=1000):
[vec0[0], vec0[1], ..., vec0[383],   # 384 floats = 1536 bytes
 vec1[0], vec1[1], ..., vec1[383],
 ...
 vec999[0], vec999[1], ..., vec999[383]]

Total size: 384 × 1000 × 4 = 1,536,000 bytes
```

### Node ID Mapping

```
Mapping (sorted by node_id for binary search):
┌──────────┬──────────┐
│ node_id  │embed_idx │
│(8 bytes) │(8 bytes) │
├──────────┼──────────┤
│   42     │    0     │  # Node 42 → vector index 0
│   99     │    1     │  # Node 99 → vector index 1
│   123    │    2     │
│   ...    │   ...    │
└──────────┴──────────┘

Binary search: O(log N) to find embedding_idx for node_id
```

## Distance Metrics

### Cosine Similarity

```rust
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    
    dot / (norm_a * norm_b)
}

// Cosine distance (for HNSW, lower is better):
fn cosine_distance(a: &[f32], b: &[f32]) -> f32 {
    1.0 - cosine_similarity(a, b)
}
```

**Characteristics**:
- Range: [0, 2] (distance), [-1, 1] (similarity)
- Normalized: Ignores magnitude, only direction
- Use case: Text embeddings, semantic similarity

### Euclidean Distance

```rust
fn euclidean_distance(a: &[f32], b: &[f32]) -> f32 {
    a.iter()
        .zip(b)
        .map(|(x, y)| {
            let diff = x - y;
            diff * diff
        })
        .sum::<f32>()
        .sqrt()
}
```

**Characteristics**:
- Range: [0, ∞)
- Magnitude-sensitive
- Use case: Image embeddings, continuous features

## HNSW Parameters

### Build-Time Parameters

```rust
struct HnswConfig {
    /// Max connections per layer (M)
    /// Higher M → better recall, more memory
    /// Typical: 16-48
    m: usize,
    
    /// Max connections at layer 0 (M0)
    /// Typically 2×M for denser base layer
    m0: usize,
    
    /// Build quality (ef_construction)
    /// Higher → better index quality, slower build
    /// Typical: 200-400
    ef_construction: usize,
    
    /// Distance metric
    metric: DistanceMetric,
}

impl Default for HnswConfig {
    fn default() -> Self {
        Self {
            m: 16,
            m0: 32,
            ef_construction: 200,
            metric: DistanceMetric::Cosine,
        }
    }
}
```

### Search-Time Parameters

```rust
struct SearchConfig {
    /// Search quality (ef_search)
    /// Higher → better recall, slower search
    /// Must be >= k
    /// Typical: 2×k to 10×k
    ef_search: usize,
}

// Example: For k=10, use ef_search=50-100
```

### Performance Trade-offs

| Parameter | Recall | Speed | Memory | Build Time |
|-----------|--------|-------|--------|------------|
| M ↑       | ↑      | ↓     | ↑      | ↑         |
| ef_construction ↑ | ↑ | same | same | ↑         |
| ef_search ↑ | ↑    | ↓     | same   | same      |

**Recommended Configurations**:

```
High Recall (production):
- M = 32
- ef_construction = 400
- ef_search = 100 (for k=10)
Recall: ~99%, Latency: ~1-2 ms

Balanced (default):
- M = 16
- ef_construction = 200
- ef_search = 50
Recall: ~95%, Latency: ~0.5 ms

Fast (low latency):
- M = 8
- ef_construction = 100
- ef_search = 20
Recall: ~90%, Latency: ~0.2 ms
```

## Cypher Integration

### vector.knn Procedure

```cypher
CALL vector.knn(
  label: String,        -- Node label to search
  vector: List<Float>,  -- Query embedding
  k: Integer            -- Number of neighbors
) 
YIELD 
  node: Node,          -- Matched node
  score: Float         -- Similarity score (0.0-1.0, higher = more similar)
```

**Example**:
```cypher
CALL vector.knn('Person', [0.1, 0.2, ..., 0.9], 10)
YIELD node, score
RETURN node.name, score
ORDER BY score DESC
```

### Hybrid Queries

**Pattern 1: KNN + Filtering**
```cypher
-- Find similar people who are active
CALL vector.knn('Person', $embedding, 100)
YIELD node AS person, score
WHERE person.active = true AND person.age > 25
RETURN person.name, score
ORDER BY score DESC
LIMIT 10
```

**Pattern 2: KNN + Traversal**
```cypher
-- Find similar people and their companies
CALL vector.knn('Person', $embedding, 20)
YIELD node AS similar, score
MATCH (similar)-[:WORKS_AT]->(company:Company)
RETURN similar.name, company.name, score
ORDER BY score DESC
```

**Pattern 3: Two-Stage Retrieval**
```cypher
-- RAG: Retrieve documents, expand to related
CALL vector.knn('Document', $query_embedding, 10)
YIELD node AS doc, score AS doc_score
MATCH (doc)-[:CITES]->(related:Document)
RETURN doc, related, doc_score
ORDER BY doc_score DESC
```

## Index Management

### Creating Index

```rust
impl KnnIndex {
    fn create(
        label_id: u32,
        dimension: usize,
        config: HnswConfig,
    ) -> Result<Self> {
        let path = format!("indexes/hnsw_{}.bin", label_id);
        
        // Initialize empty index
        let hnsw = Hnsw::new(dimension, config.m, config.ef_construction);
        
        // Write header
        let header = HnswHeader {
            magic: 0x4E455855534B4E4E,
            version: 1,
            dimension: dimension as u32,
            m: config.m as u32,
            ef_construction: config.ef_construction as u32,
            metric: config.metric as u8,
            node_count: 0,
        };
        
        let mut file = File::create(&path)?;
        file.write_all(&header.to_bytes())?;
        
        Ok(Self {
            label_id,
            dimension,
            hnsw,
            node_to_idx: HashMap::new(),
            idx_to_node: Vec::new(),
            path,
        })
    }
}
```

### Adding Vectors

```rust
impl KnnIndex {
    fn add_vector(
        &mut self,
        node_id: u64,
        vector: &[f32],
    ) -> Result<()> {
        if vector.len() != self.dimension {
            return Err(Error::index(format!(
                "Vector dimension mismatch: expected {}, got {}",
                self.dimension, vector.len()
            )));
        }
        
        // Normalize vector for cosine similarity
        let normalized = if matches!(self.config.metric, DistanceMetric::Cosine) {
            normalize_vector(vector)
        } else {
            vector.to_vec()
        };
        
        // Add to HNSW index
        let embedding_idx = self.hnsw.insert(&normalized)?;
        
        // Update mappings
        self.node_to_idx.insert(node_id, embedding_idx);
        self.idx_to_node.push(node_id);
        
        Ok(())
    }
}

fn normalize_vector(v: &[f32]) -> Vec<f32> {
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm == 0.0 {
        return v.to_vec();
    }
    v.iter().map(|x| x / norm).collect()
}
```

### Update & Eviction Contract

The pseudocode above illustrates the intended design; the actual implementation
(`crates/nexus-core/src/index/knn_index.rs`) enforces one additional invariant that
is easy to get wrong: **exactly one HNSW entry maps to a given `node_id` at all
times**, across both re-insertion (update) and removal (delete).

`hnsw_rs` (0.3.x, the crate `KnnIndex` is built on) has **no in-place update or
delete API** — once a vector is `insert`ed, its data stays physically resident
in the HNSW graph for the lifetime of the index. `KnnIndex` therefore cannot
implement "update" or "delete" by removing data from the graph itself. Instead
it uses a **tombstone-by-unmapping** strategy over the `node_id ↔ vector_index`
mapping tables:

- **`search_knn_with_ef`** only ever resolves an HNSW hit back to a `node_id`
  through the `index_to_node` map (`knn_index.rs:263`); a raw HNSW graph slot
  with no `index_to_node` entry is silently skipped and never appears in a
  result set, even though its vector payload is still inside the graph.
- **`add_vector(node_id, embedding)`** on a `node_id` that already has an
  entry evicts the OLD entry's `index_to_node` mapping BEFORE inserting the
  new vector as a fresh HNSW slot and remapping `node_id` to it. This makes
  the old vector permanently unreachable through the public search API from
  that point on, and keeps `KnnIndexStats::total_vectors` counting nodes
  (not physical graph slots) — a re-insert is a logical update, not an
  addition.
- **`remove_vector(node_id)`** evicts `node_id`'s current mapping from both
  `node_to_index` and `index_to_node`. Because `add_vector` maintains the
  one-entry-per-node invariant on every re-insert, there is never more than
  one mapping to evict — `remove_vector` cannot be asked to reach an orphan
  left by an earlier re-insert, because no such orphan can exist.
- **`knn_evict_node(node_id)`** (`engine/crud/index_maintenance.rs`) is the
  engine-level maintenance hook that calls `remove_vector`, mirroring the
  `fts_evict_node` / `spatial_evict_node` pattern used by the full-text and
  spatial indexes. Unlike those two, the KNN index is a single global
  instance rather than a named per-label registry, so eviction needs no
  `indexes_containing` lookup. As of this writing it has no production
  caller — no CREATE/SET write path maintains the KNN index yet (see
  `.rulebook/tasks/phase0_fix-knn-index-divergence/proposal.md` "Related")
  — wiring `add_vector` into CREATE/SET and calling `knn_evict_node` from
  `delete_node` is tracked as a follow-up task.

Consequence for callers: a caller that re-inserts a vector for the same node
id (an "update"), or that deletes a node and calls `knn_evict_node`, is
guaranteed the old vector is unreachable via `search_knn`/`search_knn_with_ef`
immediately afterward — no rebuild or compaction step is required. The
trade-off is that the underlying `Hnsw` graph itself only ever grows (tombstoned
slots are never physically reclaimed); a full index rebuild (`clear()` +
re-`add_vector` every live node) is the only way to reclaim that memory, and is
out of scope for this contract.

### Searching

```rust
impl KnnIndex {
    fn search(
        &self,
        query_vector: &[f32],
        k: usize,
        ef_search: usize,
    ) -> Result<Vec<(u64, f32)>> {
        // Normalize query for cosine
        let normalized_query = if matches!(self.config.metric, DistanceMetric::Cosine) {
            normalize_vector(query_vector)
        } else {
            query_vector.to_vec()
        };
        
        // HNSW search (returns embedding indices + distances)
        let results = self.hnsw.search(&normalized_query, k, ef_search)?;
        
        // Convert to (node_id, similarity_score)
        let mut node_results = Vec::new();
        for (embedding_idx, distance) in results {
            let node_id = self.idx_to_node[embedding_idx];
            
            // Convert distance to similarity (0-1, higher=better)
            let score = match self.config.metric {
                DistanceMetric::Cosine => 1.0 - distance,  // Distance → Similarity
                DistanceMetric::Euclidean => 1.0 / (1.0 + distance),  // Decay function
            };
            
            node_results.push((node_id, score));
        }
        
        // Sort by score descending
        node_results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        
        Ok(node_results)
    }
}
```

## Integration with Vectorizer

### Data Flow

```
┌─────────────────────────────────────────┐
│         Vectorizer MCP Server            │
│  (Embedding generation, vector storage) │
└─────────────┬───────────────────────────┘
              │ HTTP/MCP
              │ POST /embed {"text": "..."}
              │ Response: {"embedding": [...]}
┌─────────────▼───────────────────────────┐
│          Nexus Protocol Client           │
│      (nexus-protocol/src/rest.rs)       │
└─────────────┬───────────────────────────┘
              │
┌─────────────▼───────────────────────────┐
│         Nexus KNN Index                  │
│  - Store embedding with node             │
│  - Build/update HNSW index              │
└──────────────────────────────────────────┘
```

### Example Integration

```rust
// Ingest node with embedding from Vectorizer
async fn ingest_node_with_embedding(
    engine: &mut Engine,
    label: &str,
    properties: HashMap<String, Value>,
    text_field: &str,
) -> Result<u64> {
    // 1. Create node in graph
    let node_id = engine.create_node(
        vec![engine.catalog.get_label_id(label)?],
        properties.clone(),
    )?;
    
    // 2. Get embedding from Vectorizer
    let text = properties.get(text_field)
        .and_then(|v| v.as_str())
        .ok_or(Error::internal("Missing text field"))?;
    
    let vectorizer = VectorizerClient::new("http://localhost:8080");
    let embedding = vectorizer.embed(text).await?;
    
    // 3. Add vector to KNN index
    let label_id = engine.catalog.get_label_id(label)?;
    engine.knn_index.add_vector(label_id, node_id, &embedding)?;
    
    Ok(node_id)
}
```

## Performance Characteristics

### Build Performance

```
Dataset: 1M vectors, dimension=768

Configuration: M=16, ef_construction=200
- Build time: ~30 minutes (single-threaded)
- Index size: ~2GB (graph + vectors)
- Memory usage: ~4GB (build + index)

Parallel build (8 threads):
- Build time: ~8 minutes
- Memory usage: ~6GB
```

### Search Performance

```
Dataset: 1M vectors, k=10, ef_search=50

Latency:
- p50: 0.8 ms
- p95: 1.5 ms
- p99: 3.0 ms

Throughput: ~10,000 queries/sec (single thread)

Recall@10: ~95% (compared to exhaustive search)
```

### Scaling

```
Index size vs node count:
100K nodes:  ~200 MB (M=16, dim=768)
1M nodes:    ~2 GB
10M nodes:   ~20 GB
100M nodes:  ~200 GB (requires sharding)

Search latency scaling (logarithmic):
100K:  ~0.5 ms
1M:    ~0.8 ms
10M:   ~1.2 ms
100M:  ~1.8 ms
```

## Error Handling

```rust
// Dimension mismatch
if vector.len() != expected_dimension {
    return Err(Error::index(format!(
        "Vector dimension mismatch: expected {}, got {}",
        expected_dimension, vector.len()
    )));
}

// Index not found
if !index_exists(label_id) {
    return Err(Error::index(format!(
        "No KNN index found for label {}. Create index first.",
        label_id
    )));
}

// Invalid k parameter
if k == 0 {
    return Err(Error::executor("k must be > 0"));
}
if k > node_count {
    tracing::warn!("k={} exceeds node count={}, returning all nodes", k, node_count);
}
```

## Testing

### Unit Tests

```rust
#[test]
fn test_knn_search_cosine() {
    let mut index = KnnIndex::create(0, 128, HnswConfig::default()).unwrap();
    
    // Add vectors
    let v1 = vec![1.0; 128];
    let v2 = vec![0.5; 128];
    let v3 = vec![-1.0; 128];
    
    index.add_vector(1, &v1).unwrap();
    index.add_vector(2, &v2).unwrap();
    index.add_vector(3, &v3).unwrap();
    
    // Query similar to v1
    let query = vec![0.9; 128];
    let results = index.search(&query, 2, 50).unwrap();
    
    // Should return nodes 1 and 2 (most similar)
    assert_eq!(results[0].0, 1);  // node_id 1
    assert_eq!(results[1].0, 2);  // node_id 2
    assert!(results[0].1 > results[1].1);  // v1 more similar than v2
}
```

### Integration Tests

```rust
#[tokio::test]
async fn test_knn_procedure() {
    let engine = Engine::new().unwrap();
    
    // Create nodes with embeddings
    // ... (setup data)
    
    // Execute KNN procedure
    let query = "
        CALL vector.knn('Person', $embedding, 10)
        YIELD node, score
        RETURN node.name, score
        ORDER BY score DESC
    ";
    
    let result = engine.execute(query).await.unwrap();
    assert_eq!(result.rows.len(), 10);
    
    // Verify scores are descending
    for i in 0..9 {
        let score1: f32 = result.rows[i]["score"].as_f64().unwrap() as f32;
        let score2: f32 = result.rows[i+1]["score"].as_f64().unwrap() as f32;
        assert!(score1 >= score2);
    }
}
```

## Future Enhancements

### V1

- **Quantization**: Reduce memory via 8-bit quantization (PQ, SQ)
- **Multi-vector**: Multiple embeddings per node
- **Dynamic updates**: Incremental index updates without rebuild
- **Filtered search**: HNSW search with property filters

### V2

- **Distributed KNN**: Sharded vector indexes
- **GPU acceleration**: CUDA/ROCm for batch searches
- **Hybrid sparse-dense**: Combine BM25 + KNN (SPLADE, ColBERT)
  - **v1.8 status**: BM25 is available today via `db.index.fulltext.*`
    (Tantivy 0.22). Hybrid retrieval pattern: run `queryNodes`
    against the FTS index and `knn_traverse` against the KNN index
    in the same Cypher script, then merge / re-rank in the calling
    layer. Fused scoring at the planner level is still V2. See
    `docs/guides/FULL_TEXT_SEARCH.md`.
- **Graph-based retrieval**: Use graph structure to improve KNN

## Spatial neighbours (R-tree)

Vector KNN (HNSW) covers semantic similarity in high-dimensional
embedding spaces. Spatial KNN — "what's the closest store to
this lat/long?" — runs through a separate, packed Hilbert R-tree
backend documented in detail at
[`docs/specs/rtree-index.md`](rtree-index.md). Quick comparison:

|                         | HNSW (vector)              | R-tree (spatial)            |
|-------------------------|----------------------------|-----------------------------|
| Dimensionality          | Configurable (e.g. 128–1536)| 2 (3-D z-coord stored, query path 2-D)  |
| Distance metric         | Cosine, Euclidean, Inner   | Cartesian (Wgs84 reserved)  |
| Search class            | Approximate nearest        | Exact nearest               |
| Index payload           | Per-label vector graph     | Per-`{label}.{prop}` Hilbert R-tree |
| Hot-path complexity     | O(log N) approximate       | O(log_b N + k) exact         |
| Cypher procedure        | `CALL vector.knn(label, vec, k)` | `CALL spatial.nearest(point, label, k)` |
| DDL                     | implicit (per-label index) | `CREATE INDEX … USING RTREE` |
| Storage backend         | `index/mod.rs::KnnIndex`   | `index/rtree::RTreeRegistry` |

Hybrid retrieval (semantic + spatial) is straightforward: run
`vector.knn` and `spatial.nearest` in the same Cypher script,
then merge / re-rank at the calling layer the same way the
HNSW + FTS hybrid pattern works (see § Hybrid retrieval).

For the full spatial guide — predicates, procedures, DDL, SLO
targets — see [`docs/guides/GEOSPATIAL.md`](../guides/GEOSPATIAL.md).

## References

- HNSW Paper: https://arxiv.org/abs/1603.09320
- hnsw_rs: https://github.com/jean-pierreBoth/hnswlib-rs
- Faiss: https://github.com/facebookresearch/faiss
- Approximate Nearest Neighbors Oh Yeah (Annoy): https://github.com/spotify/annoy
- Guttman 1984 — R-Trees: A Dynamic Index Structure for Spatial Searching
- Lam-Shapiro 1994 — Hilbert curve mapping
- Skilling 2004 — 3-D Hilbert iteration

