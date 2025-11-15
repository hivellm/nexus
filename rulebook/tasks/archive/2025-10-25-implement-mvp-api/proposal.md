# Implement MVP HTTP API

## Why

Complete the HTTP REST API to expose graph database functionality to external clients. This makes Nexus usable as a standalone service.

## What Changes

- Implement complete REST endpoints (POST /cypher, POST /knn_traverse, POST /ingest)
- Add streaming support (Server-Sent Events for large results)
- Add error handling and validation
- Add timeout configuration
- Add comprehensive API tests (95%+ coverage)

**BREAKING**: None (completing existing stubs)

## Impact

### Affected Specs
- NEW capability: `rest-api`

### Affected Code
- `nexus-server/src/api/cypher.rs` - Complete implementation (~150 lines)
- `nexus-server/src/api/knn.rs` - Complete implementation (~200 lines)
- `nexus-server/src/api/ingest.rs` - Complete implementation (~180 lines)
- `nexus-server/src/streaming.rs` - SSE streaming (~100 lines)
- `tests/api_tests.rs` - API integration tests (~400 lines)

### Dependencies
- Requires: `implement-mvp-executor` AND `implement-mvp-indexes`

### Timeline
- **Duration**: 1 week
- **Complexity**: Low (mostly glue code)

