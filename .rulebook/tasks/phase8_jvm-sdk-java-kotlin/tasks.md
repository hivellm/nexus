## 1. Project scaffolding
- [ ] 1.1 Create `sdks/jvm/` Gradle Kotlin multi-module (core + http + rpc + tests)
- [ ] 1.2 Wire `kotlinx-coroutines`, `kotlinx-serialization`, `okhttp` (HTTP), `netty` (RPC), `msgpack-jackson` (MessagePack)
- [ ] 1.3 Set up GitHub Actions matrix: JDK 11 + 17 + 21 on Ubuntu / macOS / Windows

## 2. Core API
- [ ] 2.1 Implement `NexusClient` with builder pattern (URL, auth, timeouts)
- [ ] 2.2 Implement `suspend fun query(cypher: String, params: Map<String, Any?>): QueryResult`
- [ ] 2.3 Add `CompletableFuture<QueryResult> queryAsync(...)` for Java callers
- [ ] 2.4 Add blocking `QueryResult queryBlocking(...)` overload
- [ ] 2.5 Implement `QueryResult` with column-aware row accessor + `rowsAsMap()` helper

## 3. RPC transport
- [ ] 3.1 Implement length-prefixed MessagePack frame codec (Netty pipeline)
- [ ] 3.2 Implement connection pooling + reconnect with exponential backoff
- [ ] 3.3 Implement bytes-native KNN embedding path (no base64)
- [ ] 3.4 Add TLS + mTLS support
- [ ] 3.5 Wire 3-attempt leader-hint retry for V2 cluster mode

## 4. HTTP transport
- [ ] 4.1 Implement HTTP/JSON path via OkHttp
- [ ] 4.2 Match the same `NexusClient` API
- [ ] 4.3 TLS support

## 5. Auth
- [ ] 5.1 Implement API-key auth header
- [ ] 5.2 Implement JWT auth header
- [ ] 5.3 Implement rate-limit + 503 Retry-After handling

## 6. Subcommand surface
- [ ] 6.1 `db` (list / create / drop / switch)
- [ ] 6.2 `user` (CRUD + roles)
- [ ] 6.3 `key` (create / revoke / list)
- [ ] 6.4 `schema` (labels / types / properties / constraints / indexes)
- [ ] 6.5 `data` (bulk import / export)

## 7. Tests
- [ ] 7.1 Comprehensive integration test suite ≥ 30 tests against a local Nexus server
- [ ] 7.2 CRUD, parameterized queries, aggregations, KNN, FTS, savepoints
- [ ] 7.3 Java interop test (call from `.java` file)
- [ ] 7.4 Kotlin coroutines test (cancellation, timeouts)

## 8. Documentation + distribution
- [ ] 8.1 Create `sdks/jvm/README.md` quickstart + API ref
- [ ] 8.2 Create `docs/sdks/JVM.md` integration guide
- [ ] 8.3 Add JVM SDK to README highlights
- [ ] 8.4 Publish to Maven Central as `org.hivellm:nexus-sdk:1.16.0`
- [ ] 8.5 Sign artifacts with project GPG key

## 9. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 9.1 Update or create documentation covering the implementation
- [ ] 9.2 Write tests covering the new behavior
- [ ] 9.3 Run tests and confirm they pass
