# Proposal: phase22_tck-temporal-subsecond-components

> openCypher TCK re-baseline epic. Plan: docs/analysis/tck-rebaseline/
> Phase A — F-121 (RC04) (03-temporal.md)

## Why
`millisecond`, `microsecond`, and `nanosecond` are additive sub-second components that
sum into the nanosecond field. Nexus honours only `nanosecond` and silently drops the
other two — the cheapest 100 scenarios in the corpus.

```
RETURN localtime({hour:12,minute:31,second:14, nanosecond:789, millisecond:123, microsecond:456})
  -> 12:31:14.000000789                    (want 12:31:14.123456789)
RETURN datetime({year:1984,month:10,day:11,hour:12,minute:31,second:14, microsecond:645876, timezone:'+01:00'})
  -> 1984-10-11T12:31:14+01:00             (want 1984-10-11T12:31:14.645876+01:00)
```

## What Changes
- Compose the sub-second field as
  `millisecond * 1_000_000 + microsecond * 1_000 + nanosecond` for `localtime`, `time`,
  `localdatetime`, and `datetime`.
- Enforce the spec's component-ordering rule (a smaller unit may not be given without
  its larger neighbours where the outline requires it) with the right error kind — those
  negative rows belong to the same scenarios.

## TCK Impact
102 fails — `Temporal1.feature` scenarios 5-10 (3+5+24+24+23+23).

## Impact
- Affected code: crates/nexus-core/src/executor/eval/temporal_value.rs, crates/nexus-core/src/executor/eval/fn_temporal or projection/fn_temporal.rs (map-constructor path)
- Breaking change: NO
- Dependencies: None — independent.
- User benefit: sub-millisecond precision is preserved when values are constructed from components
