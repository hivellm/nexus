## 1. Investigation
- [ ] 1.1 Trace the HTTP MERGE dispatch in `write_ops.rs` and confirm why `MERGE (a)-[r:T]->(b)` creates 0 relationships
- [ ] 1.2 Confirm whether `nexus-core` `write_exec.rs` already implements correct MERGE-rel + SET-rel and can be reused by the server path

## 2. Relationship variable binding
- [ ] 2.1 Bind matched/merged relationship variables into the server write-path context (mirror node-variable binding)
- [ ] 2.2 Resolve `SET r.k = v` and `SET r += {..}` against the relationship context (null value removes key)

## 3. MERGE relationship creation
- [ ] 3.1 Make `MERGE (a)-[r:T {..}]->(b)` create the edge idempotently with inline properties, honouring direction and matched endpoints
- [ ] 3.2 Ensure `ON CREATE SET` / `ON MATCH SET` layer correctly on the merged relationship

## 4. RETURN projection
- [ ] 4.1 Project relationship-variable property access (`r.k`) in the same `CREATE/MERGE ... RETURN` statement

## 5. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 5.1 Update or create documentation covering the implementation
- [ ] 5.2 Write server-side round-trip tests: MERGE-rel creation, SET-on-rel persistence, rel-property RETURN projection
- [ ] 5.3 Run tests and confirm they pass (`cargo +nightly test -p nexus-server`)
