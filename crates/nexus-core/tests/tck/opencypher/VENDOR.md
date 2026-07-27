# Vendored openCypher TCK

This directory contains the **official openCypher Technology Compatibility Kit**,
vendored verbatim from upstream. Do not hand-edit the `.feature` files: any local
change makes the conformance number meaningless, which is the whole reason this
corpus exists.

## Provenance

| | |
|---|---|
| Upstream | https://github.com/opencypher/openCypher |
| Pinned commit | `677cbafabb8c3c5eed458fd3b1ec0daec8d67d23` |
| Upstream path | `tck/features/` |
| Vendored on | 2026-07-19 |
| Contents | 220 feature files, 1615 scenarios, ~2.0 MB |
| Licence | Apache 2.0 (`LICENSE`, `NOTICE`) — each `.feature` also carries its own attribution header |

Nexus is Apache 2.0, so the licences are compatible. The upstream `LICENSE` and
`NOTICE` are copied alongside the corpus; the per-file attribution headers required
by the openCypher community's Attribution Notice are preserved because the files are
copied byte-for-byte.

## Categories

- `features/clauses/` — 17 dirs: call, create, delete, match, match-where, merge,
  remove, return, return-orderby, return-skip-limit, set, union, unwind, with,
  with-orderBy, with-skip-limit, with-where
- `features/expressions/` — 18 dirs: aggregation, boolean, comparison, conditional,
  existentialSubqueries, graph, list, literals, map, mathematical, null, path,
  pattern, precedence, quantifier, string, temporal, typeConversion
- `features/useCases/` — countingSubgraphMatches, triadicSelection

## Relationship to the Nexus-authored spatial corpus

The sibling directory `../spatial/` holds 22 **Nexus-authored** scenarios and is
deliberately kept separate. Upstream ships no spatial scenarios at all — see
`../spatial/VENDOR.md` for that decision and its reproduction command. Do not merge
the two: one measures conformance against an external standard, the other is our own
regression corpus, and blending them would let local tests inflate the conformance
figure.

## What this is NOT

This corpus is **not** the same thing as
`scripts/compatibility/test-neo4j-nexus-compatibility-200.ps1`. That script is a
325-case *differential* suite that requires a live Neo4j and compares responses; it
measures agreement with one implementation, not conformance to the specification.
Historically the two were conflated in project documentation, which is how the
compatibility claim came to span 40 points across six files.

The measured conformance number lives in
`docs/compatibility/OPENCYPHER_TCK_REPORT.md`.

## Refreshing the corpus

```bash
git clone --depth 1 https://github.com/opencypher/openCypher.git
cp -r openCypher/tck/features crates/nexus-core/tests/tck/opencypher/
cp openCypher/LICENSE openCypher/NOTICE crates/nexus-core/tests/tck/opencypher/
```

Then update the pinned commit above, re-run the runner, and refresh the report. A
refresh that changes the pass rate must say so explicitly in the report — a moving
denominator with a static percentage is how conformance claims rot.
