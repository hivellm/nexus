# Nexus migration tooling

Scripts and cookbook examples for moving existing graph deployments
onto Nexus.

## Sources

| Source | Status | Guide |
|---|---|---|
| KuzuDB (any 0.6.x – 0.10.x) | Production-ready | [`docs/migration/FROM_KUZU.md`](../../docs/migration/FROM_KUZU.md) |

## Scripts

| File | Purpose |
|---|---|
| [`from_kuzu.py`](./from_kuzu.py) | Translate Kùzu CSV/Parquet exports into Nexus. Three subcommands: `load-csv` (emit a driver Cypher), `bulk-rpc` (stream into a running Nexus), `rewrite-cypher` (translate Kùzu queries). |

## Cookbook

End-to-end before/after pairs for the three Kùzu use cases that
matter most:

- [`cookbook/graphrag/`](./cookbook/graphrag/) — chunk → embed →
  KNN search → graph-traversal expansion.
- [`cookbook/recommendation/`](./cookbook/recommendation/) — co-purchase
  graph + cosine-similarity neighbour surfacing.
- [`cookbook/knowledge-graph/`](./cookbook/knowledge-graph/) — hybrid
  graph + vector + FTS retrieval.

## Tests

```bash
python -m pytest tests/migration/
```

19 unit tests cover the spec parsers, Cypher emitters, CSV
streamers, dialect translator, and CLI subcommands.
