## 1. Research
- [ ] 1.1 Audit Kuzu Cypher dialect (last v0.8.x release) for syntactic differences vs Nexus
- [ ] 1.2 Audit Kuzu vector + FTS index DDL for migration mapping
- [ ] 1.3 Identify Kuzu data export formats (CSV, Parquet, native `.kz`)

## 2. Documentation
- [ ] 2.1 Create `docs/migration/FROM_KUZU.md` — schema mapping section
- [ ] 2.2 Document Cypher dialect differences with examples (Kuzu → Nexus)
- [ ] 2.3 Document data-loading workflow (Kuzu export → Nexus LOAD CSV / bulk RPC)
- [ ] 2.4 Document embedded-mode replacement story (Nexus single-binary RPC vs Kuzu in-proc)
- [ ] 2.5 Document vector + FTS index migration
- [ ] 2.6 Add gotchas section (single-writer, no WASM yet, no in-proc binding)

## 3. Tooling
- [ ] 3.1 Implement `scripts/migration/from_kuzu.py` (Kuzu CSV → Nexus bulk RPC)
- [ ] 3.2 Write integration test covering a small synthetic Kuzu dump

## 4. Cookbook
- [ ] 4.1 Add GraphRAG pipeline end-to-end example
- [ ] 4.2 Add recommendation-system example
- [ ] 4.3 Add knowledge-graph example with vector + FTS hybrid query

## 5. Cross-links
- [ ] 5.1 Add README callout linking to the migration guide
- [ ] 5.2 Add CHANGELOG entry under "Documentation" for next release

## 6. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 6.1 Update or create documentation covering the implementation
- [ ] 6.2 Write tests covering the new behavior
- [ ] 6.3 Run tests and confirm they pass
