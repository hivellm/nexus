# Proposal: phase22_tck-runtime-type-guards

> openCypher TCK re-baseline epic. Plan: docs/analysis/tck-rebaseline/
> Phase C — F-111 (RC11) (02-projection-and-expressions.md)

## Why
Two defects sharing a fix site.

**(a) Missing guards** — the function silently produces null or succeeds where the spec
requires a typed error:

```
WITH [1,2,3] AS list, true AS idx RETURN list[idx]  -> succeeds  (want TypeError)
RETURN [x IN [true, []] | toBoolean(x)] AS list     -> succeeds  (want TypeError)
RETURN [x IN [r, 0]     | type(x)]     AS list      -> succeeds  (want TypeError)
MATCH p = (a) RETURN labels(p) AS l                 -> succeeds  (want TypeError)
```

**(b) Wrong `OpenCypherErrorKind`** — the error is raised but classified
`Uncategorized`:

```
WITH true AS list, 0 AS idx RETURN list[idx]
  -> "ERR_INVALID_KEY: property key must be STRING (got INTEGER)" / Uncategorized  (want TypeError)
CREATE ()-->()  -> "Relationship must have a type" / Uncategorized                 (want SyntaxError)
```

Also here: `toInteger('1.7')` must be `1` and `toInteger('2.9')` must be `2` (parse as
float, then truncate); both are null today.

**Highest regression risk in the plan** — these functions currently absorb bad input
silently and internal tests may depend on that.

## What Changes
- An argument-type contract per function for the scalar, list, and graph families
  (`toBoolean`/`toInteger`/`toFloat`, `type`, `labels`, `properties`, `keys`, list
  indexing, `range`, `size`), raising `TypeError` / `ArgumentError` instead of
  returning null.
- Extend the error-kind classification table so existing errors carry the right
  `OpenCypherErrorKind` rather than `Uncategorized`.
- `toInteger` on a numeric string with a fraction parses then truncates.

## TCK Impact
103 fails (list 44, typeConversion 22, graph 15, map 9, aggregation 7). **Blocks** phase22_tck-validate-negative-tests.

## Impact
- Affected code: crates/nexus-core/src/executor/eval/projection/fn_list.rs, fn_graph.rs, fn_string.rs, crates/nexus-core/src/error.rs (kind classification)
- Breaking change: NO
- Dependencies: None. **Blocks** phase22_tck-validate-negative-tests.
- User benefit: bad arguments fail loudly instead of yielding null and a wrong answer downstream
