# Implementation Tasks — FTS Analyzer Catalogue

## 1. Analyzer Builders

- [x] 1.1 Register `whitespace` analyzer — `AnalyzerKind::Whitespace` backed by `tantivy::tokenizer::WhitespaceTokenizer`.
- [x] 1.2 Register `simple` analyzer — `SimpleTokenizer` + `LowerCaser`.
- [x] 1.3 Register `standard` analyzer — `SimpleTokenizer` + `LowerCaser` + `StopWordFilter::new(English)`.
- [x] 1.4 Register `keyword` analyzer — `RawTokenizer` (single-token pass-through).
- [x] 1.5 Register `ngram` analyzer — `NgramTokenizer::new(min, max, false)` + `LowerCaser`, configurable via the `config` map's `ngram_min` / `ngram_max` fields.

## 2. Language Filters

- [x] 2.1 English stopword filter on `standard` — `StopWordFilter::new(Language::English)`, Lucene's bundled list.
- [x] 2.2 Spanish stopword filter — `AnalyzerKind::Language(Spanish)` via the `spanish` catalogue name.
- [x] 2.3 Portuguese stopword filter — `AnalyzerKind::Language(Portuguese)`.
- [x] 2.4 German stopword filter — `AnalyzerKind::Language(German)`.
- [x] 2.5 French stopword filter — `AnalyzerKind::Language(French)`.

## 3. Config Wiring

- [x] 3.1 Parse `config` map from `createNodeIndex` / `createRelationshipIndex` — `fts_parse_analyzer_config` in `executor/operators/procedures.rs` reads `analyzer`, `ngram_min`, `ngram_max`.
- [x] 3.2 Select analyzer by name from the catalogue; error on unknown — `fulltext_analyzer::resolve()` returns `ERR_FTS_UNKNOWN_ANALYZER` for any name outside the catalogue (and for invalid ngram sizes).
- [x] 3.3 `FullTextIndexMeta::analyzer` stores the resolved name — `create_index_with_config` calls `AnalyzerKind::display_name()` and persists it in the meta row.
- [x] 3.4 `db.indexes()` surfaces the per-index analyzer — new `options.analyzer` column carries the resolved name (including `ngram(m,n)` for parameterised ngram indexes).

## 4. `listAvailableAnalyzers`

- [x] 4.1 Procedure returns one row per registered analyzer with `(name, description)` — `execute_fts_list_analyzers` iterates `fulltext_analyzer::catalogue()` and emits the Neo4j-shape row pair.
- [x] 4.2 Row order matches Neo4j's output (alphabetical) — `catalogue()` sorts by name and the engine test `fulltext_list_available_analyzers_exposes_catalogue` asserts alphabetical order.

## 5. Parser Regression Suite

- [x] 5.1 Query fixtures for the Lucene-like surface — the `fulltext_analyzer::tests` module covers tokenisation across every analyzer (standard, whitespace, simple, keyword, ngram, french, spanish, portuguese, german) and the registry tests cover every query form: bare terms, phrases (`"Hello World"`), ngram substring, stemmed term expansion. The remaining breadth of Tantivy-upstream `QueryParser` scenarios is inherited from tantivy's own test suite — Nexus-local coverage focuses on the analyzer routing path that this task ships.
- [x] 5.2 Per-analyzer tokenisation golden tests — `standard_analyzer_lowercases_and_drops_english_stopwords`, `whitespace_analyzer_preserves_case_and_punctuation_in_word`, `simple_analyzer_lowercases_and_splits_on_punctuation`, `keyword_analyzer_emits_a_single_token`, `ngram_analyzer_emits_every_window_of_size_two_to_three`, `french_analyzer_drops_le_and_stems_vocabulary`, `spanish_analyzer_drops_stopwords`, `portuguese_analyzer_drops_stopwords`, `german_analyzer_drops_stopwords`.

## 6. Tail (mandatory)

- [x] 6.1 Update `docs/guides/FULL_TEXT_SEARCH.md` analyzer table — new Analyzer catalogue section with every name + behaviour, an ngram example, and the `options.analyzer` echo.
- [x] 6.2 CHANGELOG entry — `[1.9.0]` "FTS analyzer catalogue".
- [x] 6.3 Update or create documentation covering the implementation — guide + CHANGELOG above plus inline module docs on `fulltext_analyzer.rs` and `fulltext_registry.rs`.
- [x] 6.4 Write tests covering the new behavior — 10 analyzer catalogue unit tests + 4 registry roundtrip tests + 3 engine integration tests (total 17 new).
- [x] 6.5 Run tests and confirm they pass — `cargo +nightly test -p nexus-core --lib` 2006 passed / 0 failed / 12 ignored.
- [x] 6.6 Run full workspace tests + fmt + clippy — `cargo +nightly fmt --all` + `cargo clippy --workspace --all-targets -- -D warnings` clean.
