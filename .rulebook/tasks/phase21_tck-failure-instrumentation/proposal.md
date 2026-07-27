# Proposal: phase21_tck-failure-instrumentation

> Part of the openCypher TCK 100%-conformance epic. Plan: docs/analysis/tck/.
> Phase 0 — measurement foundation (01-measurement-and-methodology.md)

## Why
The TCK harness discards the reason for every failure (tck_opencypher.rs:293-303), so 3175 failures are opaque. This XS change converts them into measurable buckets and is the HARD PREREQUISITE for accurate sizing of every other task. Also wires the `parameters are:` step (unblocks ~62 skip-listed scenarios immediately).

## What Changes
- Emit a JSONL line per StepFailed in the `after` hook (category, feature_path, scenario_name, query, message, last_error, last_error_kind).
- Add `Given "parameters are:"` step parser wired to execute_cypher_with_params (already exists).
- Add `When "executing control query:"` alias.
- Optional low-priority: binary-tree-N fixture step.

## TCK Impact
~62 skip-list scenarios unblocked; reliable per-category failure metric for all subsequent sizing.

## Impact
- Affected code: tests/tck_opencypher.rs (harness only)
- Breaking change: NO
- Dependencies: None (foundational, do first).
- User benefit: closes the conformance gap toward the TCK 100% target (baseline 13.2%).
