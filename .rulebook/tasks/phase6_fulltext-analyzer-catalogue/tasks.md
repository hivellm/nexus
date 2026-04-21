# Implementation Tasks — FTS Analyzer Catalogue

## 1. Analyzer Builders

- [ ] 1.1 Register `whitespace` analyzer
- [ ] 1.2 Register `simple` analyzer (lowercase + alphabetic split)
- [ ] 1.3 Register `standard` analyzer (default; tokenizer + lowercase + stopwords)
- [ ] 1.4 Register `keyword` analyzer (single-token pass-through)
- [ ] 1.5 Register `ngram` analyzer with configurable `min` / `max`

## 2. Language Filters

- [ ] 2.1 English stopword filter on `standard`
- [ ] 2.2 Spanish stopword filter
- [ ] 2.3 Portuguese stopword filter
- [ ] 2.4 German stopword filter
- [ ] 2.5 French stopword filter

## 3. Config Wiring

- [ ] 3.1 Parse `config` map from `createNodeIndex` / `createRelationshipIndex` arguments
- [ ] 3.2 Select analyzer by name from the catalogue; error on unknown
- [ ] 3.3 `FullTextIndexMeta::analyzer` stores the resolved name
- [ ] 3.4 `db.indexes()` surfaces the per-index analyzer

## 4. `listAvailableAnalyzers`

- [ ] 4.1 Procedure returns one row per registered analyzer with `(name, description)`
- [ ] 4.2 Row order matches Neo4j's output (alphabetical)

## 5. Parser Regression Suite

- [ ] 5.1 50+ query fixtures: terms, phrases, boolean, fielded, fuzzy, prefix, range
- [ ] 5.2 Per-analyzer tokenisation golden tests

## 6. Tail (mandatory)

- [ ] 6.1 Update `docs/guides/FULL_TEXT_SEARCH.md` analyzer table
- [ ] 6.2 CHANGELOG entry
- [ ] 6.3 Run full workspace tests + fmt + clippy
