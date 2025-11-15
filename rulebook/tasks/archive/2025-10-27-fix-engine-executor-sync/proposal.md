## Why

Engine and Executor were using separate RecordStore instances, causing MATCH queries to return empty results after CREATE operations. The Engine's create_node method was not updating the label_index, preventing Executor from finding nodes by label.

## What Changes

- **MODIFIED**: Engine::create_node now updates label_index automatically after node creation
- **MODIFIED**: MATCH queries now use engine.execute_cypher() to access shared storage
- **MODIFIED**: /data/nodes endpoint now uses shared Engine instance via ENGINE.get()
- **MODIFIED**: Added engine initialization to data.rs module

## Impact

- **Affected specs**: Cypher Executor
- **Affected code**: 
  - `nexus-core/src/lib.rs` - Added label_index update in create_node
  - `nexus-server/src/api/cypher.rs` - MATCH queries use shared Engine
  - `nexus-server/src/api/data.rs` - Added ENGINE static and init_engine()
  - `nexus-server/src/main.rs` - Added engine initialization for data module

## Breaking Changes

None - internal implementation improvement with no API changes.

