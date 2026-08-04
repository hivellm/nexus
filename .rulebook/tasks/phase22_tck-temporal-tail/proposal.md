# Proposal: phase22_tck-temporal-tail

> openCypher TCK re-baseline epic. Plan: docs/analysis/tck-rebaseline/
> Phase D — F-124 (RC23) (03-temporal.md)

## Why
Four unrelated temporal leftovers, explicitly last:

| Symptom | Want | Got |
|---|---|---|
| `datetime.fromepoch(416779, 999999999)` | `1970-01-05T19:46:19.999999999Z` | `null` |
| `datetime.fromepochmillis(237821673987)` | `1977-07-15T13:34:33.987Z` | `null` |
| `duration.between(date('-999999999-01-01'), date('+999999999-12-31'))` | `P1999999998Y11M30D` | `null` |
| `duration.inSeconds(localtime(), localtime())` | `PT0S` | `PT0.0000199S` |
| `duration.between(...10:00:00.1, ...10:00:00.2).nanosecondsOfSecond` | `100000000` | `-900000000` |

The `PT0S` case is not a duration bug: it is a **statement clock** requirement — every
clock read inside one statement must return the same instant. That also affects
`date()` / `datetime()` reproducibility, so it is worth doing even though its scenario
count is 4. The `nanosecondsOfSecond` sign is a real borrow bug in the negative-duration
field split.

## What Changes
- `datetime.fromepoch` and `datetime.fromepochmillis`.
- The ±999999999-year range for date/localdatetime parsing and duration arithmetic.
- A statement clock: one instant per statement, shared by every clock-reading function.
- Correct borrow when splitting a negative duration into fields.

## TCK Impact
8 fails, plus statement-clock reproducibility beyond the TCK.

## Impact
- Affected code: crates/nexus-core/src/executor/eval/temporal_parse.rs, temporal_duration_between.rs, the clock source used by the temporal builtins
- Breaking change: NO
- Dependencies: Last — lowest yield per effort.
- User benefit: epoch constructors work and repeated clock reads in one statement agree
