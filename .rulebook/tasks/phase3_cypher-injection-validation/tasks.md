## 1. Implementation
- [ ] 1.1 Implement `validate_identifier` helper (single-line regex check) and unit-test it against positive + negative inputs
- [ ] 1.2 Wire it in at `api/schema.rs:317`, `api/ingest.rs:307`, `api/knn.rs:102`
- [ ] 1.3 Wire it in at `api/graphql/resolver.rs` for `rel_type` and at `api/graphql/schema.rs::build_nodes_query` for each label
- [ ] 1.4 Return `400 Bad Request` with an explicit error message listing which input failed validation
- [ ] 1.5 Audit `nexus-server/src/api/` with `grep -n 'format!("MATCH\|format!("CREATE\|format!("MERGE'` to confirm no handler was missed

## 2. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 2.1 Update `docs/SECURITY_AUDIT.md` with the injection surface closed
- [ ] 2.2 Add a `tests/cypher_injection_test.rs` covering the malicious payload case for each affected endpoint
- [ ] 2.3 Run `cargo test --workspace` and confirm all pass
