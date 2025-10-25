# Implementation Tasks - MVP Storage Layer

## 1. Catalog Implementation (LMDB Integration)

- [x] 1.1 Setup heed (LMDB wrapper) with database environment
- [x] 1.2 Create bidirectional mappings (label_name ↔ label_id)
- [x] 1.3 Create bidirectional mappings (type_name ↔ type_id)
- [x] 1.4 Create bidirectional mappings (key_name ↔ key_id)
- [x] 1.5 Add statistics storage (node counts per label, rel counts per type)
- [x] 1.6 Add metadata storage (version, epoch, page_size)
- [x] 1.7 Implement get_or_create_label/type/key methods
- [x] 1.8 Add unit tests for catalog (95%+ coverage)
- [x] 1.9 Add concurrent access tests (multiple readers/writers)

## 2. Record Stores Implementation

- [x] 2.1 Implement NodeRecord struct (32 bytes: label_bits, first_rel_ptr, prop_ptr, flags)
- [x] 2.2 Implement RelationshipRecord struct (48 bytes: src, dst, type, next_src, next_dst, prop_ptr, flags)
- [x] 2.3 Implement PropertyRecord struct (variable: key_id, type, value, next_ptr)
- [x] 2.4 Setup memory-mapped files with memmap2
- [x] 2.5 Implement nodes.store read/write operations
- [x] 2.6 Implement rels.store read/write with linked lists
- [x] 2.7 Implement props.store with property chains
- [x] 2.8 Implement strings.store with varint encoding + CRC32
- [x] 2.9 Add file growth strategy (1MB → 2x growth)
- [x] 2.10 Add unit tests for each store type (95%+ coverage)
- [x] 2.11 Add integration tests for linked list traversal

## 3. Page Cache Implementation

- [x] 3.1 Implement Page struct (8KB: header + body)
- [x] 3.2 Implement page header (page_id, checksum, flags)
- [x] 3.3 Implement Clock eviction algorithm
- [x] 3.4 Add pin/unpin semantics with reference counting
- [x] 3.5 Implement dirty page tracking
- [x] 3.6 Add xxHash3 checksum validation
- [x] 3.7 Implement flush operations (single page + all dirty)
- [x] 3.8 Add page cache statistics (hits, misses, evictions)
- [x] 3.9 Add unit tests for eviction policy (95%+ coverage)
- [x] 3.10 Add integration tests with storage layer
- [x] 3.11 Add concurrency tests (pin/unpin from multiple threads)

## 4. Write-Ahead Log (WAL) Implementation

- [x] 4.1 Implement WalEntry enum (BeginTx, CommitTx, AbortTx, CreateNode, CreateRel, SetProperty, Checkpoint)
- [x] 4.2 Implement WAL binary format (epoch, tx_id, type, length, payload, crc32)
- [x] 4.3 Implement append operation with CRC32 validation
- [x] 4.4 Implement flush operation (fsync for durability)
- [x] 4.5 Implement checkpoint mechanism
- [x] 4.6 Implement WAL recovery (replay entries after crash)
- [x] 4.7 Implement WAL archiving (archive old segments)
- [x] 4.8 Add configuration (checkpoint_interval, max_wal_size)
- [x] 4.9 Add unit tests for WAL operations (95%+ coverage)
- [x] 4.10 Add crash recovery tests (simulate crash + recovery)
- [x] 4.11 Add corruption detection tests (invalid CRC)

## 5. Transaction Manager (MVCC) Implementation

- [x] 5.1 Implement EpochManager (atomic epoch counter)
- [x] 5.2 Implement Snapshot struct (epoch pinning)
- [x] 5.3 Implement Transaction struct (id, epoch, state)
- [x] 5.4 Implement begin_read (create snapshot)
- [x] 5.5 Implement begin_write (acquire write lock)
- [x] 5.6 Implement commit operation (WAL flush + epoch increment)
- [x] 5.7 Implement abort operation (rollback changes)
- [x] 5.8 Implement single-writer locking (parking_lot::Mutex)
- [x] 5.9 Implement version visibility rules (created_epoch, deleted_epoch)
- [x] 5.10 Add unit tests for transaction lifecycle (95%+ coverage)
- [x] 5.11 Add snapshot isolation tests (concurrent read/write)
- [x] 5.12 Add MVCC visibility tests

## 6. Integration & Testing

- [x] 6.1 Create integration test: create node + read node
- [x] 6.2 Create integration test: create relationship + traverse
- [x] 6.3 Create integration test: update property + read
- [x] 6.4 Create integration test: transaction commit + rollback
- [x] 6.5 Create integration test: WAL recovery after crash
- [x] 6.6 Create integration test: page cache eviction
- [x] 6.7 Create performance benchmark: node insert throughput
- [x] 6.8 Create performance benchmark: node read latency
- [x] 6.9 Create performance benchmark: relationship traversal
- [x] 6.10 Verify 95%+ test coverage (cargo llvm-cov)

## 7. Documentation Updates

- [x] 7.1 Update docs/ROADMAP.md (mark Phase 1.1-1.2 as complete)
- [x] 7.2 Add implementation notes to docs/ARCHITECTURE.md
- [x] 7.3 Update CHANGELOG.md with v0.2.0 notes
- [x] 7.4 Add usage examples to README.md (programmatic API)
- [x] 7.5 Document configuration options in config.example.yml

## 8. Quality Gates

- [x] 8.1 Run cargo +nightly fmt --all
- [x] 8.2 Run cargo clippy --workspace --all-targets -- -D warnings
- [x] 8.3 Run cargo test --workspace --verbose (100% passing)
- [x] 8.4 Run cargo nextest run --workspace (all tests passing)
- [x] 8.5 Run cargo llvm-cov --workspace (verify 95%+ coverage)
- [x] 8.6 Run codespell (0 errors)
- [x] 8.7 Run cargo build --release (verify release build works)
- [x] 8.8 Performance validation (meet target: 100K+ point reads/sec)

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

