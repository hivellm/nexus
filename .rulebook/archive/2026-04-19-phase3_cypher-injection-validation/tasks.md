## 1. Implementation
- [x] 1.1 Implement `validate_identifier` helper (single-line regex check) and unit-test it against positive + negative inputs — landed as `nexus-server/src/api/identifier.rs` with structured `InvalidIdentifier` error enum, 12 unit tests covering leading-digit / whitespace / hyphen / pattern-breakout / 256-byte cap, plus a `validate_all` helper for the label-list case
- [x] 1.2 Wire it in at `api/knn.rs` (label), `api/ingest.rs` (node labels + relationship type)
- [x] 1.3 Wire it in at `api/graphql/resolver.rs` for every `rel_type` interpolation (outgoing_relationships, incoming_relationships, all_relationships) and at `api/graphql/mutation.rs::create_relationship` for `rel_type`
- [x] 1.4 Return `400 Bad Request` (or GraphQL error / ingest batch error) with an explicit message listing which input failed validation; the `InvalidIdentifier` Display impl echoes the offender
- [x] 1.5 Audit `nexus-server/src/api/` with `grep -n 'format!("MATCH\|format!("CREATE\|format!("MERGE'` to confirm no handler was missed — schema.rs no longer has the format! site the original proposal mentioned (it now uses catalog methods directly); the remaining `graphql/mutation.rs` delete paths only interpolate node IDs parsed as `u64`, which cannot be injected

## 2. Tail (mandatory — enforced by rulebook v5.3.0)
- [x] 2.1 Update or create documentation covering the implementation: `docs/security/SECURITY_AUDIT.md` now has a "Cypher Injection via Unvalidated Identifiers" subsection under Attack Vector Testing documenting the surface closed, the payload, and the hardening note for future grammar extensions
- [x] 2.2 Write tests covering the new behavior: `nexus-server/tests/cypher_injection_test.rs` with 4 tests — canonical payload rejection, every-escape-character matrix, `knn_traverse` malicious-label rejection, `knn_traverse` happy-path validation pass-through
- [x] 2.3 Run tests and confirm they pass: `cargo +nightly test -p nexus-server` → all 13 test binaries green (357+ unit tests + the new injection suite + the phase2 OnceLock guard)
