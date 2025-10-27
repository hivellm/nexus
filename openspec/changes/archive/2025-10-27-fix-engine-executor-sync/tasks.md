# Implementation Tasks - Fix Engine-Executor Sync

## 1. Engine Integration

- [x] 1.1 Update Engine::create_node to call label_index.add_node() after node creation
- [x] 1.2 Collect label_ids during node creation loop

## 2. Cypher API Updates

- [x] 2.1 Add MATCH query detection in cypher.rs
- [x] 2.2 Implement MATCH execution via engine.execute_cypher()
- [x] 2.3 Handle CREATE and MATCH queries separately

## 3. Data API Refactoring

- [x] 3.1 Add static ENGINE to data.rs
- [x] 3.2 Create init_engine() function in data.rs
- [x] 3.3 Refactor create_node() to use ENGINE.get()
- [x] 3.4 Remove temporary Engine creation in create_node handler

## 4. Main Initialization

- [x] 4.1 Add api::data::init_engine() call in main.rs

## 5. Testing and Validation

- [x] 5.1 Test CREATE followed by MATCH returns results
- [x] 5.2 Test /data/nodes creates persistent nodes
- [x] 5.3 Test /stats reflects all created nodes
- [x] 5.4 Run all 1041 tests
- [x] 5.5 Verify no clippy warnings

## 6. Documentation

- [x] 6.1 Update CHANGELOG.md
- [x] 6.2 Update OpenSpec tasks.md files
- [x] 6.3 Commit all changes

## Completed

All tasks completed in v0.9.1 release.
- Commits: ede99eb, caab9e8, 5405d25
- Tag: v0.9.1

