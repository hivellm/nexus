# Migration guide structure for displaced graph-DB users: schema map → dialect deltas → data-loading → embedded→server → gotchas → before/after cookbooks

**Category**: documentation
**Tags**: migration, documentation, kuzu, cypher, competitive

## Description

When writing a migration guide from another graph DB (Kùzu, RedisGraph, JanusGraph, etc), the seven-section template is: (1) TL;DR table summarising effort per concern, (2) Schema mapping with type tables and DDL examples, (3) Cypher dialect differences with side-by-side examples, (4) Data-loading workflow (CSV+LOAD CSV for medium, bulk RPC for large), (5) Embedded-mode replacement story (most third-party DBs ship in-proc; Nexus is RPC), (6) Vector + FTS index migration with score-sign caveat, (7) Performance expectations, (8) Gotchas. Pair the doc with a CLI tool (`from_<source>.py`) that has at least three subcommands: `load-csv` (emit driver Cypher), `bulk-rpc` (stream into running server), `rewrite-cypher` (regex translator with `-- TRANSLATOR-NOTE` comments on every rewrite). Ship 3 before/after cookbooks under `scripts/migration/cookbook/<usecase>/` so the diff is one file open away.

## Example

docs/migration/FROM_KUZU.md
scripts/migration/from_kuzu.py            # CLI: load-csv, bulk-rpc, rewrite-cypher
scripts/migration/cookbook/graphrag/
    kuzu_before.py
    nexus_after.py
    README.md
tests/migration/test_from_kuzu.py         # parsers, emitters, translator, CLI

## When to Use

When a competitor's graph DB hits an end-of-life signal (archive, layoffs, public roadmap shrink) and Nexus targets the same use cases. The migration guide is a low-effort, high-leverage retention play.

## When NOT to Use

For minor version migrations within the same DB family — those go in MIGRATION_v{prev}_to_v{next}.md with the operator-checklist + rollback-procedure shape, not the cookbook shape.
