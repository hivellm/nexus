# Implementation Tasks - MVP Indexes

## 1. Label Bitmap Index

- [ ] 1.1 Setup RoaringBitmap per label_id
- [ ] 1.2 Implement add_node(label_id, node_id)
- [ ] 1.3 Implement remove_node(label_id, node_id)
- [ ] 1.4 Implement get_nodes(label_id) → Vec<node_id>
- [ ] 1.5 Implement bitmap operations (AND, OR, NOT for multi-label queries)
- [ ] 1.6 Implement cardinality estimation
- [ ] 1.7 Add persistence (serialize/deserialize to files)
- [ ] 1.8 Add unit tests for bitmap operations (95%+ coverage)
- [ ] 1.9 Add integration test with storage layer

## 2. KNN Vector Index (HNSW)

- [ ] 2.1 Setup hnsw_rs with configurable M and ef_construction
- [ ] 2.2 Implement add_vector(node_id, vector) with normalization
- [ ] 2.3 Implement search_knn(query_vector, k, ef_search) → [(node_id, score)]
- [ ] 2.4 Implement node_id ↔ embedding_idx mapping (binary search)
- [ ] 2.5 Add distance metric configuration (cosine, euclidean)
- [ ] 2.6 Implement index persistence (custom binary format)
- [ ] 2.7 Add index rebuild functionality
- [ ] 2.8 Add unit tests for KNN search (95%+ coverage)
- [ ] 2.9 Add recall@k benchmarks (vs brute-force)
- [ ] 2.10 Add performance tests (10K+ queries/sec target)

## 3. Index Statistics

- [ ] 3.1 Track node count per label
- [ ] 3.2 Track relationship count per type
- [ ] 3.3 Track NDV (number distinct values) per property key
- [ ] 3.4 Implement statistics update on insert/delete
- [ ] 3.5 Add statistics persistence in catalog
- [ ] 3.6 Add unit tests for statistics tracking

## 4. Integration & Testing

- [ ] 4.1 Integration test: Label index with storage layer
- [ ] 4.2 Integration test: KNN index with storage layer
- [ ] 4.3 Integration test: Multi-label query (bitmap AND)
- [ ] 4.4 Performance benchmark: Label scan throughput
- [ ] 4.5 Performance benchmark: KNN query latency
- [ ] 4.6 Verify 95%+ test coverage

## 5. Documentation & Quality

- [ ] 5.1 Update docs/ROADMAP.md (mark Phase 1.3 complete)
- [ ] 5.2 Add index examples to README
- [ ] 5.3 Update CHANGELOG.md
- [ ] 5.4 Run all quality checks (fmt, clippy, test, coverage)

