# Temporal truncate()'s nanosecond map override ADDS to the remainder, never replaces it

Neo4j's `date.truncate`/`datetime.truncate`/`localdatetime.truncate`/`time.truncate`/
`localtime.truncate` third-argument map can carry a `nanosecond` key. The openCypher
TCK's `Temporal9.feature` proves this is an ADD onto the already-truncated nanosecond
remainder, not a REPLACE of the whole field: truncating to `millisecond` on
`nanosecond: 645876123` leaves `645000000` (below-millisecond digits zeroed);
applying `{nanosecond: 2}` on top must yield `645000002`, not `2`.

For `day`/`hour`/`minute`/`second` truncation the remainder is already `0`, so add
and replace are indistinguishable there — only `millisecond`/`microsecond`
truncation exposes the difference, which is why it's easy to miss without checking
the exact table rows.

The clean implementation (with correct calendar carry on overflow): build a
`chrono::NaiveDateTime` from the truncated `(date, time)` and call
`checked_add_signed(chrono::Duration::nanoseconds(override_value))` — chrono's own
arithmetic handles any overflow into seconds/minutes/hours/date correctly, no
hand-rolled carry logic needed.

The `day`/`dayOfWeek` map keys, by contrast, are a genuine REPLACE (`{day: 5}` sets
day-of-month to `5` outright, not "+5 days").

See `crates/nexus-core/src/executor/eval/temporal_truncate.rs`.
