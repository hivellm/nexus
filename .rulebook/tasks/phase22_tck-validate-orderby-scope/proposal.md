# Proposal: phase22_tck-validate-orderby-scope

> openCypher TCK re-baseline epic. Plan: docs/analysis/tck-rebaseline/
> Phase C — F-134 (RC12) (04-read-clauses.md)

## Why
`WITH` narrows scope: only its projected aliases are visible to its own `ORDER BY`,
`SKIP`, `LIMIT`, and everything downstream. Neither rule is enforced:

```
... WITH a ORDER BY count(a)  -> succeeds   (want an error: aggregation in ORDER BY)
... WITH a AS b ORDER BY a    -> succeeds   (want an error: `a` is out of scope)
```

Weights: *Fail on sorting by an aggregation* 25, *... by any number of undefined
variables in any position* 13, *... by an undefined variable / out of scope* 5+.

## What Changes
- Reject an aggregation in an `ORDER BY` that is not itself over an aggregating
  projection, with the spec's error kind.
- Reject a variable that is out of scope after the `WITH` that narrowed it, in
  `ORDER BY`, `SKIP`, `LIMIT`, and downstream clauses.
- Share the scope-tracking helper with phase22_tck-validate-variable-reuse.

## TCK Impact
46 fails (with-orderBy 43).

## Impact
- Affected code: crates/nexus-core/src/executor/semantic_validation/mod.rs
- Breaking change: NO
- Dependencies: Runs after phase22_tck-validate-variable-reuse to reuse its scope helper.
- User benefit: sorting by something out of scope is an error instead of silently ignored
