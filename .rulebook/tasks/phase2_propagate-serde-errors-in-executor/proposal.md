# Proposal: phase2_propagate-serde-errors-in-executor

## Why

The executor uses `serde_json::to_string(...).unwrap_or_default()` and
`serde_json::from_value(...).unwrap_or_default()` at the cores of GROUP BY,
aggregation, DISTINCT, and expand — e.g.
`executor/mod.rs:3610, 3849, 5038`. When serialisation fails, the code
silently substitutes an empty string or `Vec::new()` and keeps going. Two
concrete risks:

1. **Wrong results with no error**: GROUP BY produces a bogus group key
   `""` that collapses unrelated rows into one bucket.
2. **Hidden compatibility regressions**: a future Cypher type or a
   non-UTF-8 property silently degrades query output instead of failing
   loudly.

Similar silent swallowing happens around `warm_cache_lazy` at line 158.

## What Changes

- Replace the `unwrap_or_default` / `unwrap_or_else(_ → Vec::new())` calls
  with explicit `.map_err(|e| Error::CypherExecution(...))` propagation.
- Where silent fallback is a deliberate compatibility shim, keep it but
  gate it behind a `tracing::warn!` that includes the value type and a
  metric counter `executor_serde_fallback_total{site=…}`.
- Add unit tests that feed values known to break current serialisation
  and assert either a typed error or at least a warn-level log event.

## Impact

- Affected specs: `docs/specs/cypher-subset.md`
- Affected code:
  - `nexus-core/src/executor/mod.rs:158, 3610, 3849, 5038` and other
    `unwrap_or_default()` sites near serde boundaries
- Breaking change: queries that *previously* silently returned wrong
  answers now return errors — strictly an improvement but visible to
  callers relying on the bug
- User benefit: correctness over superficially successful responses
