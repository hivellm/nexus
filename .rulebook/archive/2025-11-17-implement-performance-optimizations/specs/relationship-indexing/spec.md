# Advanced Relationship Indexing Specification

## 🎯 **Overview**

Replace current linked-list relationship storage with sophisticated indexing system to achieve Neo4j-level performance for relationship queries and traversals.

## 📋 **Requirements**

### Functional Requirements:
- [ ] Type-based relationship indexes
- [ ] Direction-aware indexing (incoming/outgoing)
- [ ] Compressed bitmap storage for fast set operations
- [ ] Real-time index maintenance on relationship creation/deletion
- [ ] Index consistency guarantees

### Performance Requirements:
- [ ] Relationship traversal <4ms average (currently 8-9ms)
- [ ] Relationship count <2ms average (currently 3-4ms)
- [ ] Index lookup <1ms
- [ ] Memory overhead <20% of relationship data
- [ ] Index build time <10 seconds for 1M relationships

### Compatibility Requirements:
- [ ] Backward compatibility with existing linked-list storage
- [ ] Graceful fallback if indexes unavailable
- [ ] Online index maintenance (no downtime)
- [ ] Incremental index updates

## 🏗️ **Current Architecture Issues**

### Linked List Problems:
```rust
// Current: Linked list traversal (slow)
// Each relationship lookup requires sequential node reads
pub struct NodeRecord {
    pub first_rel_ptr: u64,  // → rel1 → rel2 → rel3 → ...
}

// Traversal requires O(degree) operations
fn traverse_relationships(&self, node_id: u64) -> Vec<RelationshipRecord> {
    let mut relationships = Vec::new();
    let mut current_ptr = self.read_node(node_id)?.first_rel_ptr;

    while current_ptr != 0 {
        let rel = self.read_relationship(current_ptr)?;
        relationships.push(rel);
        current_ptr = rel.next_rel_ptr;  // Sequential access!
    }

    relationships
}
```

## 🎯 **Target Architecture**

### Indexed Relationship Storage:

```
Node Records:
┌─────────────┐    ┌─────────────┐
│ Node 1      │    │ Node 2      │
│ first_rel=0 │    │ first_rel=0 │
└─────────────┘    └─────────────┘

Relationship Records: (unchanged)
┌─────────────┐    ┌─────────────┐    ┌─────────────┐
│ Rel 1 (1→2) │    │ Rel 2 (2→3) │    │ Rel 3 (1→3) │
│ src=1,dst=2 │    │ src=2,dst=3 │    │ src=1,dst=3 │
│ type=FRIEND │    │ type=FRIEND │    │ type=FOLLOWS│
└─────────────┘    └─────────────┘    └─────────────┘

Type-Based Indexes:
┌─────────────────┐    ┌─────────────────┐
│ FRIEND (type=1) │    │ FOLLOWS (type=2)│
│ src→bitmap      │    │ src→bitmap      │
│ 1: {2}          │    │ 1: {3}          │
│ 2: {1,3}        │    │                 │
├─────────────────┤    └─────────────────┘
│ dst→bitmap      │
│ 1: {2}          │
│ 2: {1}          │
│ 3: {1,2}        │
└─────────────────┘
```

## 🏗️ **Implementation Design**

### 1. Relationship Index Manager

```rust
struct RelationshipIndexManager {
    type_indexes: Arc<RwLock<HashMap<u32, RelationshipTypeIndex>>>,
    stats: Arc<RelationshipIndexStats>,
    max_memory: usize,  // 500MB default
}

struct RelationshipTypeIndex {
    type_id: u32,
    outgoing: HashMap<u64, RoaringBitmap>,  // src_node → dst_nodes
    incoming: HashMap<u64, RoaringBitmap>,  // dst_node → src_nodes
    last_updated: Instant,
}
```

### 2. Index Operations

```rust
impl RelationshipIndexManager {
    pub fn add_relationship(&self, rel: &RelationshipRecord) -> Result<()> {
        let mut indexes = self.type_indexes.write();

        let type_index = indexes.entry(rel.type_id)
            .or_insert_with(|| RelationshipTypeIndex::new(rel.type_id));

        // Add to outgoing index (src → dst)
        type_index.outgoing
            .entry(rel.src)
            .or_insert_with(RoaringBitmap::new)
            .insert(rel.dst);

        // Add to incoming index (dst → src)
        type_index.incoming
            .entry(rel.dst)
            .or_insert_with(RoaringBitmap::new)
            .insert(rel.src);

        Ok(())
    }

    pub fn remove_relationship(&self, rel: &RelationshipRecord) -> Result<()> {
        let mut indexes = self.type_indexes.write();

        if let Some(type_index) = indexes.get_mut(&rel.type_id) {
            // Remove from outgoing index
            if let Some(outgoing) = type_index.outgoing.get_mut(&rel.src) {
                outgoing.remove(rel.dst);
                if outgoing.is_empty() {
                    type_index.outgoing.remove(&rel.src);
                }
            }

            // Remove from incoming index
            if let Some(incoming) = type_index.incoming.get_mut(&rel.dst) {
                incoming.remove(rel.src);
                if incoming.is_empty() {
                    type_index.incoming.remove(&rel.dst);
                }
            }
        }

        Ok(())
    }
}
```

### 3. Query Operations

```rust
impl RelationshipIndexManager {
    pub fn find_outgoing_relationships(
        &self,
        src_node: u64,
        rel_type: Option<u32>,
    ) -> Result<Vec<u64>> {
        let indexes = self.type_indexes.read();

        if let Some(rel_type) = rel_type {
            // Specific type query
            if let Some(type_index) = indexes.get(&rel_type) {
                if let Some(outgoing) = type_index.outgoing.get(&src_node) {
                    return Ok(outgoing.iter().collect());
                }
            }
            Ok(Vec::new())
        } else {
            // All types query - union of all type indexes
            let mut result = RoaringBitmap::new();
            for type_index in indexes.values() {
                if let Some(outgoing) = type_index.outgoing.get(&src_node) {
                    result |= outgoing;
                }
            }
            Ok(result.iter().collect())
        }
    }

    pub fn count_relationships(
        &self,
        src_node: Option<u64>,
        rel_type: Option<u32>,
        direction: Direction,
    ) -> Result<u64> {
        let indexes = self.type_indexes.read();

        let mut count = 0u64;

        if let Some(rel_type) = rel_type {
            // Specific type
            if let Some(type_index) = indexes.get(&rel_type) {
                match direction {
                    Direction::Outgoing => {
                        if let Some(node) = src_node {
                            if let Some(outgoing) = type_index.outgoing.get(&node) {
                                count += outgoing.len();
                            }
                        } else {
                            count += type_index.outgoing.values().map(|b| b.len()).sum::<u64>();
                        }
                    }
                    Direction::Incoming => {
                        if let Some(node) = src_node {
                            if let Some(incoming) = type_index.incoming.get(&node) {
                                count += incoming.len();
                            }
                        } else {
                            count += type_index.incoming.values().map(|b| b.len()).sum::<u64>();
                        }
                    }
                    Direction::Both => {
                        // Union of outgoing and incoming
                        if let Some(node) = src_node {
                            let outgoing_count = type_index.outgoing.get(&node)
                                .map(|b| b.len()).unwrap_or(0);
                            let incoming_count = type_index.incoming.get(&node)
                                .map(|b| b.len()).unwrap_or(0);
                            count += outgoing_count + incoming_count;
                        } else {
                            let outgoing_total = type_index.outgoing.values()
                                .map(|b| b.len()).sum::<u64>();
                            let incoming_total = type_index.incoming.values()
                                .map(|b| b.len()).sum::<u64>();
                            count += outgoing_total + incoming_total;
                        }
                    }
                }
            }
        } else {
            // All types
            for type_index in indexes.values() {
                count += self.count_relationships(src_node, Some(type_index.type_id), direction)?;
            }
        }

        Ok(count)
    }
}
```

### 4. Integration with Query Executor

```rust
impl Executor {
    fn execute_expand_indexed(
        &self,
        context: &mut ExecutionContext,
        type_ids: &[u32],
        direction: Direction,
        source_var: &str,
        target_var: &str,
        rel_var: &str,
    ) -> Result<()> {
        // Get source nodes from context
        let source_values = context.get_variable(source_var)
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        for source_value in source_values {
            if let Some(source_id) = Self::extract_entity_id(&source_value) {
                // Use indexes instead of linked list traversal
                let target_ids = if type_ids.is_empty() {
                    // All relationship types
                    self.relationship_index.find_outgoing_relationships(source_id, None)?
                } else {
                    // Specific relationship types
                    let mut all_targets = RoaringBitmap::new();
                    for &type_id in type_ids {
                        let type_targets = self.relationship_index
                            .find_outgoing_relationships(source_id, Some(type_id))?;
                        all_targets.extend(type_targets);
                    }
                    all_targets.iter().collect()
                };

                // Process each target relationship
                for target_id in target_ids {
                    // Load target node and relationship details
                    if let Ok(target_node) = self.read_node_as_value(target_id) {
                        // Create relationship record (simplified)
                        let rel_record = self.create_relationship_record(source_id, target_id, type_ids[0])?;

                        // Add to result set
                        let mut result_row = HashMap::new();
                        result_row.insert(source_var.to_string(), source_value.clone());
                        result_row.insert(target_var.to_string(), target_node);
                        result_row.insert(rel_var.to_string(), serde_json::to_value(&rel_record)?);

                        context.result_set.rows.push(Row {
                            values: vec![
                                source_value.clone(),
                                target_node,
                                serde_json::to_value(&rel_record)?,
                            ]
                        });
                    }
                }
            }
        }

        Ok(())
    }
}
```

## 📊 **Index Storage and Compression**

### Bitmap Compression:
- **RoaringBitmap**: Efficient for sparse data
- **Run-length encoding** for consecutive ranges
- **Container-based storage**: Array/RLE/Bitmap containers

### Memory Layout:
```
RelationshipTypeIndex (per type):
├── type_id: u32
├── outgoing: HashMap<u64, RoaringBitmap>
│   └── src_node → dst_nodes_bitmap
└── incoming: HashMap<u64, RoaringBitmap>
    └── dst_node → src_nodes_bitmap
```

### Memory Estimation:
- **Per relationship**: ~8-16 bytes in indexes
- **Compression ratio**: 10-50x vs linked lists
- **Lookup time**: O(1) vs O(degree) for linked lists

## 🔄 **Index Maintenance**

### Online Index Updates:
```rust
impl Engine {
    pub fn create_relationship_with_indexing(
        &mut self,
        from: u64,
        to: u64,
        rel_type: String,
        properties: serde_json::Value,
    ) -> Result<u64> {
        // Create relationship (existing logic)
        let rel_id = self.create_relationship_with_transaction(from, to, rel_type, properties, &mut tx_ref)?;

        // Update indexes
        if let Ok(rel_record) = self.storage.get_relationship(&tx, rel_id) {
            self.relationship_index.add_relationship(&rel_record)?;
        }

        Ok(rel_id)
    }
}
```

### Index Recovery:
```rust
impl RelationshipIndexManager {
    pub fn rebuild_indexes(&self, storage: &RecordStore) -> Result<()> {
        // Clear existing indexes
        self.clear_all_indexes()?;

        // Scan all relationships and rebuild indexes
        let all_relationships = storage.scan_all_relationships()?;

        for rel in all_relationships {
            self.add_relationship(&rel)?;
        }

        Ok(())
    }
}
```

## 📈 **Performance Characteristics**

### Query Performance Improvements:

| Operation | Current (Linked List) | Target (Indexed) | Improvement |
|-----------|----------------------|------------------|-------------|
| Single relationship lookup | O(degree) | O(1) | 10-100x |
| Relationship count | O(degree) | O(1) | 50-500x |
| Type-filtered traversal | O(degree) | O(log n) | 20-200x |
| Multi-hop traversal | O(degree²) | O(degree) | degree x |

### Memory Overhead:
- **Index size**: 10-30% of relationship data size
- **Bitmap compression**: 5-10x compression ratio
- **Memory per relationship**: ~12 bytes average
- **Scalability**: Sub-linear growth with relationship count

### Build Performance:
- **Initial build**: O(relationships) time
- **Incremental updates**: O(1) per relationship
- **Memory usage during build**: 2x peak index size

## 🧪 **Testing Strategy**

### Unit Tests:
- [ ] Index creation and deletion operations
- [ ] Bitmap operations and compression
- [ ] Concurrent index access
- [ ] Index consistency after failures

### Integration Tests:
- [ ] Relationship creation with indexing
- [ ] Complex traversal queries
- [ ] Index recovery after restart
- [ ] Concurrent read/write workloads

### Performance Tests:
- [ ] Index build time measurement
- [ ] Query performance comparison
- [ ] Memory usage monitoring
- [ ] Index maintenance overhead

### Compatibility Tests:
- [ ] Fallback to linked lists when indexes unavailable
- [ ] Data consistency between indexed and non-indexed queries
- [ ] Migration path for existing databases

## 📊 **Monitoring & Observability**

### Metrics to Collect:
- Index hit/miss ratios
- Index build times
- Memory usage per index
- Query performance improvements
- Index maintenance overhead

### Alerts:
- Index inconsistency detected
- Index build time >10 minutes
- Memory usage >80% of limit
- Query performance regression >20%

## 🔄 **Migration Strategy**

### Phase 1: Dual Storage
- Keep linked list storage as primary
- Build indexes alongside for comparison
- Allow runtime switching between implementations

### Phase 2: Gradual Migration
- Enable indexing for new relationships only
- Background index building for existing data
- Feature flags for index usage per query type

### Phase 3: Full Migration
- Make indexed storage the default
- Remove linked list fallback code
- Optimize storage format for indexed access

## 🚨 **Safety Guarantees**

### Data Consistency:
- Indexes updated atomically with relationship operations
- Index recovery on startup if inconsistencies detected
- Transactional semantics for index operations

### Performance Safety:
- Fallback to linked list traversal if indexes fail
- Bounded memory usage with configurable limits
- No performance regression for non-indexed operations

### Reliability:
- Index corruption detection and repair
- Graceful degradation under memory pressure
- Online index maintenance without downtime
