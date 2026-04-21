# Proposal: phase6_fulltext-analyzer-catalogue

## Why

v1.8 ships FTS with a single hard-coded `standard` analyzer.
Neo4j's catalogue exposes `whitespace`, `simple`, `standard`,
`keyword`, and `ngram`, plus language-aware stopword filters and
configurable n-gram sizes. Users migrating off Neo4j need the same
tokenisation choices to preserve query-recall parity; a default-
only deployment silently under-matches compared to Neo4j on the
same corpus.

## What Changes

1. Register the full analyzer set (`whitespace`, `simple`, `standard`,
   `keyword`, `ngram`) as first-class Tantivy `TextAnalyzer` builders.
2. Wire the `config` procedure argument through to pick an analyzer
   per index; surface the resolved analyzer name in `db.indexes()`.
3. Add multilingual stopword filters for the `standard` analyzer
   (at minimum: English, Spanish, Portuguese, German, French).
4. Make `ngram` parameterised by `min` / `max` gram size in the
   config map.
5. Ship a 50-query parser regression suite covering the Lucene-like
   surface (terms, phrases, boolean connectives, fielded, fuzzy,
   prefix, range).
6. `listAvailableAnalyzers()` returns the catalogue instead of the
   single `standard` row.

## Impact

- Affected specs: `docs/guides/FULL_TEXT_SEARCH.md` analyzer table.
- Affected code: `crates/nexus-core/src/index/fulltext.rs`, `fulltext_registry.rs`, the `execute_fts_list_analyzers` procedure path.
- Breaking change: NO (existing `standard` default preserved).
- User benefit: Neo4j-parity tokenisation; language-aware matching; n-gram autocomplete.
