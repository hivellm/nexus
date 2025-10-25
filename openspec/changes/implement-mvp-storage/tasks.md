# Implementation Tasks - MVP Storage Layer

## 1. Catalog Implementation (LMDB Integration)

- [ ] 1.1 Setup heed (LMDB wrapper) with database environment
- [ ] 1.2 Create bidirectional mappings (label_name ↔ label_id)
- [ ] 1.3 Create bidirectional mappings (type_name ↔ type_id)
- [ ] 1.4 Create bidirectional mappings (key_name ↔ key_id)
- [ ] 1.5 Add statistics storage (node counts per label, rel counts per type)
- [ ] 1.6 Add metadata storage (version, epoch, page_size)
- [ ] 1.7 Implement get_or_create_label/type/key methods
- [ ] 1.8 Add unit tests for catalog (95%+ coverage)
- [ ] 1.9 Add concurrent access tests (multiple readers/writers)

## 2. Record Stores Implementation

- [ ] 2.1 Implement NodeRecord struct (32 bytes: label_bits, first_rel_ptr, prop_ptr, flags)
- [ ] 2.2 Implement RelationshipRecord struct (48 bytes: src, dst, type, next_src, next_dst, prop_ptr, flags)
- [ ] 2.3 Implement PropertyRecord struct (variable: key_id, type, value, next_ptr)
- [ ] 2.4 Setup memory-mapped files with memmap2
- [ ] 2.5 Implement nodes.store read/write operations
- [ ] 2.6 Implement rels.store read/write with linked lists
- [ ] 2.7 Implement props.store with property chains
- [ ] 2.8 Implement strings.store with varint encoding + CRC32
- [ ] 2.9 Add file growth strategy (1MB → 2x growth)
- [ ] 2.10 Add unit tests for each store type (95%+ coverage)
- [ ] 2.11 Add integration tests for linked list traversal

## 3. Page Cache Implementation

- [ ] 3.1 Implement Page struct (8KB: header + body)
- [ ] 3.2 Implement page header (page_id, checksum, flags)
- [ ] 3.3 Implement Clock eviction algorithm
- [ ] 3.4 Add pin/unpin semantics with reference counting
- [ ] 3.5 Implement dirty page tracking
- [ ] 3.6 Add xxHash3 checksum validation
- [ ] 3.7 Implement flush operations (single page + all dirty)
- [ ] 3.8 Add page cache statistics (hits, misses, evictions)
- [ ] 3.9 Add unit tests for eviction policy (95%+ coverage)
- [ ] 3.10 Add integration tests with storage layer
- [ ] 3.11 Add concurrency tests (pin/unpin from multiple threads)

## 4. Write-Ahead Log (WAL) Implementation

- [ ] 4.1 Implement WalEntry enum (BeginTx, CommitTx, AbortTx, CreateNode, CreateRel, SetProperty, Checkpoint)
- [ ] 4.2 Implement WAL binary format (epoch, tx_id, type, length, payload, crc32)
- [ ] 4.3 Implement append operation with CRC32 validation
- [ ] 4.4 Implement flush operation (fsync for durability)
- [ ] 4.5 Implement checkpoint mechanism
- [ ] 4.6 Implement WAL recovery (replay entries after crash)
- [ ] 4.7 Implement WAL archiving (archive old segments)
- [ ] 4.8 Add configuration (checkpoint_interval, max_wal_size)
- [ ] 4.9 Add unit tests for WAL operations (95%+ coverage)
- [ ] 4.10 Add crash recovery tests (simulate crash + recovery)
- [ ] 4.11 Add corruption detection tests (invalid CRC)

## 5. Transaction Manager (MVCC) Implementation

- [ ] 5.1 Implement EpochManager (atomic epoch counter)
- [ ] 5.2 Implement Snapshot struct (epoch pinning)
- [ ] 5.3 Implement Transaction struct (id, epoch, state)
- [ ] 5.4 Implement begin_read (create snapshot)
- [ ] 5.5 Implement begin_write (acquire write lock)
- [ ] 5.6 Implement commit operation (WAL flush + epoch increment)
- [ ] 5.7 Implement abort operation (rollback changes)
- [ ] 5.8 Implement single-writer locking (parking_lot::Mutex)
- [ ] 5.9 Implement version visibility rules (created_epoch, deleted_epoch)
- [ ] 5.10 Add unit tests for transaction lifecycle (95%+ coverage)
- [ ] 5.11 Add snapshot isolation tests (concurrent read/write)
- [ ] 5.12 Add MVCC visibility tests

## 6. Integration & Testing

- [ ] 6.1 Create integration test: create node + read node
- [ ] 6.2 Create integration test: create relationship + traverse
- [ ] 6.3 Create integration test: update property + read
- [ ] 6.4 Create integration test: transaction commit + rollback
- [ ] 6.5 Create integration test: WAL recovery after crash
- [ ] 6.6 Create integration test: page cache eviction
- [ ] 6.7 Create performance benchmark: node insert throughput
- [ ] 6.8 Create performance benchmark: node read latency
- [ ] 6.9 Create performance benchmark: relationship traversal
- [ ] 6.10 Verify 95%+ test coverage (cargo llvm-cov)

## 7. Documentation Updates

- [ ] 7.1 Update docs/ROADMAP.md (mark Phase 1.1-1.2 as complete)
- [ ] 7.2 Add implementation notes to docs/ARCHITECTURE.md
- [ ] 7.3 Update CHANGELOG.md with v0.2.0 notes
- [ ] 7.4 Add usage examples to README.md (programmatic API)
- [ ] 7.5 Document configuration options in config.example.yml

## 8. Quality Gates

- [ ] 8.1 Run cargo +nightly fmt --all
- [ ] 8.2 Run cargo clippy --workspace --all-targets -- -D warnings
- [ ] 8.3 Run cargo test --workspace --verbose (100% passing)
- [ ] 8.4 Run cargo nextest run --workspace (all tests passing)
- [ ] 8.5 Run cargo llvm-cov --workspace (verify 95%+ coverage)
- [ ] 8.6 Run codespell (0 errors)
- [ ] 8.7 Run cargo build --release (verify release build works)
- [ ] 8.8 Performance validation (meet target: 100K+ point reads/sec)

## Success Criteria

- ✅ All 8 sections above completed
- ✅ 95%+ test coverage achieved
- ✅ All quality checks passing
- ✅ Can create nodes, relationships, and properties programmatically
- ✅ Transactions commit and rollback correctly
- ✅ WAL recovery works after simulated crash
- ✅ Page cache eviction prevents OOM
- ✅ Performance targets met (100K+ reads/sec)

## Notes

- Follow strict test-driven development (TDD)
- Each module must have unit tests before integration tests
- Run quality checks after each major milestone
- Update documentation as implementation progresses
- Commit after completing each numbered section

