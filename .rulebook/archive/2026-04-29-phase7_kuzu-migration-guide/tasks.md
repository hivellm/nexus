## 1. Research
- [x] 1.1 Audit Kuzu Cypher dialect (v0.6.x – v0.10.x) for syntactic differences vs Nexus
- [x] 1.2 Audit Kuzu vector + FTS index DDL for migration mapping
- [x] 1.3 Identify Kuzu data export formats (CSV, Parquet, native `.kz`)

## 2. Documentation
- [x] 2.1 Create `docs/migration/FROM_KUZU.md` — schema mapping section
- [x] 2.2 Document Cypher dialect differences with examples (Kuzu → Nexus)
- [x] 2.3 Document data-loading workflow (Kuzu export → Nexus LOAD CSV / bulk RPC)
- [x] 2.4 Document embedded-mode replacement story (Nexus single-binary RPC vs Kuzu in-proc)
- [x] 2.5 Document vector + FTS index migration
- [x] 2.6 Add gotchas section (single-writer, no WASM yet, no in-proc binding)

## 3. Tooling
- [x] 3.1 Implement `scripts/migration/from_kuzu.py` (Kuzu CSV → Nexus bulk RPC)
- [x] 3.2 Write integration test covering a small synthetic Kuzu dump (`tests/migration/test_from_kuzu.py`, 19 tests)

## 4. Cookbook
- [x] 4.1 Add GraphRAG pipeline end-to-end example
- [x] 4.2 Add recommendation-system example
- [x] 4.3 Add knowledge-graph example with vector + FTS hybrid query

## 5. Cross-links
- [x] 5.1 Add README callout linking to the migration guide
- [x] 5.2 Add CHANGELOG entry under "Documentation" for next release

## 6. Tail (mandatory — enforced by rulebook v5.3.0)
- [x] 6.1 Update or create documentation covering the implementation
- [x] 6.2 Write tests covering the new behavior
- [x] 6.3 Run tests and confirm they pass
