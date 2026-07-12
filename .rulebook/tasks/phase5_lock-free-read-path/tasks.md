## 1. Baseline
- [ ] 1.1 Write the concurrency benchmark (N parallel clients, MATCH workload via nexus-bench); record before numbers

## 2. Read-only classification
- [ ] 2.1 Extend the shared AST predicate (`api/cypher/routing.rs`) with `is_read_only(ast)`: no write clause, no DDL, no tx command
- [ ] 2.2 Unit-test table for the classifier (OPTIONAL MATCH, WITH-only, UNWIND-read, EXPLAIN-wrapped, subqueries, procedure calls that write)

## 3. Routing
- [ ] 3.1 HTTP handler: autocommit read-only queries → lock-free `Arc<Executor>` clone + `spawn_blocking`; in-transaction reads stay on the engine
- [ ] 3.2 RPC dispatch: same routing
- [ ] 3.3 Executor snapshot freshness: ensure post-write executor refresh makes committed writes visible to subsequent lock-free reads (write→read visibility test)

## 4. Validation
- [ ] 4.1 Snapshot-visibility tests: read-your-own-write inside a session/transaction unchanged; committed-write visibility across connections
- [ ] 4.2 Run the concurrency benchmark after; gate: ≥3x throughput at 8+ parallel clients on MATCH workload

## 5. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 5.1 Update or create documentation covering the implementation (docs/nexus/03 + performance docs with before/after)
- [ ] 5.2 Write tests covering the new behavior (classifier table + visibility + routing)
- [ ] 5.3 Run tests and confirm they pass (workspace suite, clippy zero warnings)
