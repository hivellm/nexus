# Proposal: phase21_tck-parser-set-replace

> Part of the openCypher TCK 100%-conformance epic. Plan: docs/analysis/tck/.
> Phase 3 — parser breadth W-E (06-clauses-read-write.md)

## Why
parser/clauses/write.rs:157 does not support `SET n = {map}` (else arm -> hard parse error), killing the Set4 family. With Phase 1 counting this recovers clauses/set from ~2% to ~80%.

## What Changes
- Add SetItem::Replace AST variant + parser arm.
- write_exec: apply whole-entity replace.
- Support parenthetical SET (n).prop syntax.

## TCK Impact
clauses/set (Set4 family), with Phase 1 counting.

## Impact
- Affected code: crates/nexus-core/src/executor/parser/clauses/write.rs, engine/write_exec.rs
- Breaking change: NO
- Dependencies: Phase 1 (counting-semantics).
- User benefit: closes the conformance gap toward the TCK 100% target (baseline 13.2%).
