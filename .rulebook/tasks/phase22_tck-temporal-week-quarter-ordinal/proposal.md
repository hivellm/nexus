# Proposal: phase22_tck-temporal-week-quarter-ordinal

> openCypher TCK re-baseline epic. Plan: docs/analysis/tck-rebaseline/
> Phase C — F-122 (RC05) (03-temporal.md)

## Why
Only Y/M/D reach the date constructors, so every alternative date-component family
collapses to January 1st of the given year:

```
RETURN date({year: 1816, week: 52})               -> 1816-01-01   (want 1816-12-23)
RETURN date({year: 1817, week: 1})                -> 1817-01-01   (want 1816-12-30)
RETURN date({year: 1984, week: 10, dayOfWeek: 3}) -> 1984-01-01   (want 1984-03-07)
```

Note `{year: 1817, week: 1}` -> **1816**-12-30: the `year` in a week-date map is the
ISO-8601 **week-based year**, not the calendar year. That is the part implementations
most often get wrong.

## What Changes
- Support the three alternative date-component families: `week` + `dayOfWeek` (ISO week
  date, week-based year), `quarter` + `dayOfQuarter`, and `ordinalDay`.
- Enforce mutual exclusivity with each other and with `month` + `day`, with the spec's
  error kind.
- Apply to `date`, `localdatetime`, and `datetime` alike.

## TCK Impact
47 fails — `Temporal1.feature` scenarios 1-4 (14+14+14+5).

## Impact
- Affected code: crates/nexus-core/src/executor/eval/temporal_value.rs (date-component resolution)
- Breaking change: NO
- Dependencies: Runs after phase22_tck-temporal-select-from-temporal (same constructor path).
- User benefit: ISO week dates, quarters, and ordinal days can be used to construct dates
