# Proposal: phase21_tck-temporal-canonicalization-on-set

> Part of the openCypher TCK 100%-conformance epic. Plan: docs/analysis/tck/.
> Surfaced during an independent review of the TCK conformance commits.
> Adjacent to phase21_tck-temporal-projection-and-store (store/reload round-trip);
> that task owns the encoding, this one owns the missing write-path boundary.

## Why
Typed temporal values carry an internal tagged form
(`{"_nexus_temporal_type": "duration", ...}`) that must be canonicalised to an
ISO-8601 string at the boundary. Canonicalisation is re-applied **manually at
each boundary point** rather than once at a choke point, and one of those points
is missing.

Present: CREATE (`executor/operators/create.rs:642`), executor output
(`executor/dispatch/execute.rs:167`), write-path RETURN
(`engine/write_exec/return_builder.rs:141` and `:263`).
Absent: `engine/write_exec/properties.rs`, which handles `SET n.p = <expr>` and
`SET n += {...}`. Nothing there canonicalises before `state.properties.insert`.

```
CREATE (n {d: duration({days: 1})})       → stores "P1D"
MATCH (n) SET n.d = duration({days: 1})   → stores {"_nexus_temporal_type": ...}
```

Two nodes holding the same logical value end up with structurally different
on-disk representations depending on which clause wrote them. Read-only TCK
scenarios still look correct because the projection-boundary pass at
`dispatch/execute.rs:167` recursively canonicalises every outgoing row value —
which masks the stale on-disk tag rather than preventing it. Any consumer that
reads storage without going back through that pass sees the inconsistency: a
future property index over a temporal column, a uniqueness or property-type
constraint check, WAL/export tooling.

The design is self-documented as fragile: the doc comment on
`executor/eval/temporal_value.rs:604-632` enumerates the three boundary points
and explicitly names `SET` as a path that "could still carry a stale tag". The
gap is disclosed, but unfixed.

## What Changes
Canonicalise temporal values on the SET/MERGE write path before they are
persisted, covering `SET n.p = <expr>`, `SET n = {map}` (whole-entity replace)
and `SET n += {map}`.

Then address the root cause rather than adding a fourth boundary point:
consolidate canonicalisation into a single property-write choke point that every
write path converges on, so a new write path cannot silently skip it. If the
boundary points must stay separate for a reason the review missed, document that
reason in place of the consolidation.

Decide and document the canonical on-disk representation, and keep reads
tolerant of values already written in the raw tagged form by the current code.

## Impact
- Affected specs: docs/specs/cypher-subset.md (temporal storage representation)
- Affected code: crates/nexus-core/src/engine/write_exec/properties.rs,
  crates/nexus-core/src/executor/eval/temporal_value.rs,
  crates/nexus-core/src/executor/operators/create.rs
- Breaking change: NO on the wire (output is already canonicalised by the
  projection boundary). YES for the on-disk representation of temporal values
  previously written by `SET` — reads must stay tolerant of the raw tagged form.
- User benefit: one representation per temporal value on disk regardless of the
  clause that wrote it; unblocks property indexes and constraints over temporal
  columns; removes the "N manual boundary points" fragility where any new write
  path silently persists an internal tagged form.
