# Proposal: phase6_relwalk-warn-at-fallback

Source: GitHub issue #20 (https://github.com/hivellm/nexus/issues/20)

## Why
The hub-degree telemetry added in #12
(`crates/nexus-core/src/engine/mod.rs:3343`) checks `hops >= 1000` AFTER
the chain-walk while-loop finishes, so a 10,000-hop scan only warns once it
has already completed. A no-query CPU climb from O(degree) edge-MERGE
existence checks therefore isn't surfaced until the expensive scan is
already done.

## What Changes
- Emit a `warn` (or `debug`) at the moment the exact-edge fast path misses
  and the chain-walk fallback begins, and/or fire the 1000-hop threshold
  warning DURING the loop rather than after it completes.
- Keep it cheap (no per-hop logging; one log at fallback entry + one at the
  threshold crossing).

## Impact
- Affected specs: observability
- Affected code: `crates/nexus-core/src/engine/mod.rs` (find_relationship_between)
- Breaking change: NO
- User benefit: the O(degree) hub pathology is observable in real time, not
  only post-mortem.

## Notes
- Audit finding #7 (follow-up to #12; complements #18/#4 which reduce how
  often the fallback is hit).
