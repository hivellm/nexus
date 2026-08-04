# 03 — Temporal

**334 fails, 27 distinct scenarios, 3 defects plus a tail.** The first analysis called
temporal the `XL` gate of the whole project; the typed-value subsystem, ISO-8601
parsing, accessors, truncation, arithmetic, and the timezone database all landed
(43 archived tasks, temporal chain closed `2026-08-04`). What remains is **component
coverage in the map constructors** — narrow and finite.

Distribution: `Temporal3.feature` 176, `Temporal1.feature` 157, `Temporal10` 7,
`Temporal7` 1.

---

## F-120 — RC03: construction from another temporal value is ignored · **176 fails** · M

The whole of `Temporal3.feature`. The constructors accept a map whose fields are
`date:`, `time:`, or `datetime:` carrying an existing temporal value, optionally
overridden by scalar components. Nexus **ignores those fields** and falls back to
defaults (today's date / midnight):

```
WITH date({year:1984,month:11,day:11}) AS other
RETURN date({date: other})                          → 2026-01-01        (want 1984-11-11)

WITH date({year:1984,month:10,day:11}) AS d, localtime({hour:12,minute:31,second:14,nanosecond:645876123}) AS t
RETURN localdatetime({date: d, time: t})            → 2026-01-01T00:00  (want 1984-10-11T12:31:14.645876123)

WITH localtime({hour:12,minute:31,second:14,nanosecond:645876123}) AS other
RETURN time({time: other})                          → 00:00Z            (want 12:31:14.645876123Z)

WITH localdatetime({...}) AS other
RETURN datetime(other)                              → 1984-01-01T12:31:14Z (want 1984-03-07T12:31:14.645Z)
```

Scenario weights: *select date and time into date time* 48, *construct local date time*
selection 24, *select date and time into local date time* 24, *select time into date
time* 16, *select time* 19, *select date* 18, and the rest.

Required semantics: for each of `date`/`localtime`/`time`/`localdatetime`/`datetime`,
the map may carry (a) a `date:` component supplying Y/M/D, (b) a `time:` component
supplying H/M/S/ns and, for `time`/`datetime`, its offset, (c) a `datetime:` component
supplying both, (d) scalar components that **override** the selected ones, and the
single-argument form `datetime(<temporal>)` / `localdatetime(<temporal>)` which
truncates or widens across types. Fields absent from the source default per spec
(missing date → `1970-01-01`, missing time → midnight), **not** to the current clock.

**Owning task:** `phase22_tck-temporal-select-from-temporal`.
**Confidence.** High.

---

## F-121 — RC04: sub-second components do not compose · **102 fails** · S

`millisecond`, `microsecond`, and `nanosecond` are additive sub-second components that
**sum** into the nanosecond field; Nexus honours only `nanosecond` and drops the other
two:

```
RETURN localtime({hour:12,minute:31,second:14, nanosecond:789, millisecond:123, microsecond:456})
  → 12:31:14.000000789          (want 12:31:14.123456789)

RETURN datetime({year:1984,month:10,day:11,hour:12,minute:31,second:14, microsecond:645876, timezone:'+01:00'})
  → 1984-10-11T12:31:14+01:00   (want 1984-10-11T12:31:14.645876+01:00)
```

`millisecond × 1_000_000 + microsecond × 1_000 + nanosecond`, and the spec forbids
specifying a smaller unit without its larger neighbours where the outline says so —
those negative rows are part of the same scenarios. Affects `localtime`, `time`,
`localdatetime`, `datetime` (scenarios 5–10 of `Temporal1`: 3+5+24+24+23+23).

The **cheapest 100 scenarios in the corpus.** One arithmetic site.

**Owning task:** `phase22_tck-temporal-subsecond-components`.
**Confidence.** High.

---

## F-122 — RC05: week / quarter / ordinal date components · **47 fails** · M

Only Y/M/D reach the date constructors; the three alternative date-component families
are ignored, so every value collapses to January 1st of the given year:

```
RETURN date({year: 1816, week: 52})              → 1816-01-01   (want 1816-12-23)
RETURN date({year: 1817, week: 1})               → 1817-01-01   (want 1816-12-30)
RETURN date({year: 1984, week: 10, dayOfWeek: 3})→ 1984-01-01   (want 1984-03-07)
```

Note `{year: 1817, week: 1}` → **1816**-12-30: this is the ISO-8601 *week-based year*,
not the calendar year, and it is the part implementations most often get wrong. The
three families are `week`+`dayOfWeek` (ISO week date), `quarter`+`dayOfQuarter`, and
`ordinalDay`; they are mutually exclusive with each other and with `month`+`day`.
Applies to `date`, `localdatetime`, and `datetime` alike (scenarios 1–4 of `Temporal1`:
14+14+14+5).

**Owning task:** `phase22_tck-temporal-week-quarter-ordinal`.
**Confidence.** High.

---

## F-123 — Instant-based comparison and ordering for offset-aware temporals · *(RC16 share, ~20 fails)* · M

`time` and `datetime` values carrying different offsets must compare and sort by the
**instant** they denote, not by wall-clock fields:

```
WITH time({hour:10,minute:0,timezone:'+01:00'}) AS x,
     time({hour:9,minute:35,second:14,nanosecond:645876123,timezone:'+00:00'}) AS d
RETURN x > d, x < d           → true, false        (want false, true)
```

`09:00Z` < `09:35:14Z`, so `x < d`. Nexus compares `10:00` against `09:35` literally.
The same defect drives the `Sort times/date times in ascending/descending order`
failures in `clauses/with-orderBy`, which is why this finding is filed under RC16
rather than as its own root cause — one comparator serves both.

**Owning task:** `phase22_tck-ordering-cross-type-and-instant` (shared with F-137).
**Confidence.** High.

---

## F-124 — RC23: the temporal tail · **8 fails** · M

Four unrelated leftovers, low value, explicitly last:

| Symptom | Want | Got |
|---|---|---|
| `datetime.fromepoch(416779, 999999999)` | `1970-01-05T19:46:19.999999999Z` | `null` |
| `datetime.fromepochmillis(237821673987)` | `1977-07-15T13:34:33.987Z` | `null` |
| `duration.between(date('-999999999-01-01'), date('+999999999-12-31'))` | `P1999999998Y11M30D` | `null` |
| `duration.inSeconds(localtime(), localtime())` | `PT0S` | `PT0.0000199S` |
| `duration.between(…10:00:00.1, …10:00:00.2).nanosecondsOfSecond` | `100000000` | `-900000000` |

The `PT0S` one is not a duration bug — it is a **statement clock** requirement: all
clock reads inside one statement must return the same instant. Worth noting separately
because it also affects `date()`/`datetime()` reproducibility. The negative-duration
borrow (`nanosecondsOfSecond` sign) is a genuine arithmetic bug in the field split.

**Owning task:** `phase22_tck-temporal-tail`.
**Confidence.** High.

---

## F-125 — What temporal does *not* need

Stated so no one re-opens closed work: the typed temporal value, canonical rendering,
projection-boundary serialisation, ISO-8601 parsing, ~50 property accessors,
truncation, arithmetic, `duration.between` family, `SET`-time canonicalisation, and the
named-timezone database are **all landed and passing** (`expressions/temporal` 66.7%
with 670 passes, up from near-zero). The `XL` framing in `../tck/03-temporal.md` is
historical. Do not re-plan those.

**Confidence.** High (archive + measured passes).
