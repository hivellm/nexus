# Proposal: phase22_tck-temporal-select-from-temporal

> openCypher TCK re-baseline epic. Plan: docs/analysis/tck-rebaseline/
> Phase C — F-120 (RC03) (03-temporal.md)

## Why
The whole of `Temporal3.feature` — 176 failures. Temporal constructors accept a map
whose fields carry an existing temporal value (`date:`, `time:`, `datetime:`),
optionally overridden by scalar components. Nexus ignores those fields and falls back to
defaults (today's date, midnight):

```
WITH date({year:1984,month:11,day:11}) AS other
RETURN date({date: other})                     -> 2026-01-01        (want 1984-11-11)

WITH date({...}) AS d, localtime({...}) AS t
RETURN localdatetime({date: d, time: t})       -> 2026-01-01T00:00  (want 1984-10-11T12:31:14.645876123)

WITH localtime({hour:12,minute:31,second:14,nanosecond:645876123}) AS other
RETURN time({time: other})                     -> 00:00Z            (want 12:31:14.645876123Z)

WITH localdatetime({...}) AS other
RETURN datetime(other)                         -> wrong             (want the widened value)
```

## What Changes
For each of `date`, `localtime`, `time`, `localdatetime`, `datetime`:
- a `date:` component supplies Y/M/D; a `time:` component supplies H/M/S/ns and, for
  `time`/`datetime`, its offset; a `datetime:` component supplies both;
- scalar components in the same map **override** the selected ones;
- the single-argument form `datetime(<temporal>)` / `localdatetime(<temporal>)`
  truncates or widens across types;
- fields absent from the source default per spec (missing date -> `1970-01-01`,
  missing time -> midnight) — **never** to the current clock.

## TCK Impact
176 fails — all of `Temporal3.feature` (10 scenarios, largest at 48 rows).

## Impact
- Affected code: crates/nexus-core/src/executor/eval/temporal_value.rs, the temporal map-constructor path
- Breaking change: NO
- Dependencies: Runs after phase22_tck-temporal-subsecond-components (same constructor path).
- User benefit: temporal values can be derived from other temporal values, the normal way to build them
