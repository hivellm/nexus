# 03 — Temporal (`expressions/temporal`)

The single largest cluster: **1004 scenarios, 51 pass, 935 fail, 18 skip (5.1%)** —
~30% of all failing scenarios. An instance of workstream **W-D** (F-012). Anchored
in `crates/nexus-core/src/executor/eval/projection/fn_temporal.rs`,
`.../eval/temporal.rs`, `.../executor/types.rs`, `.../value.rs`, and the
`expressions/temporal/Temporal{1..10}.feature` corpus.

Scenario density (feature-table line counts, a proxy for weight): truncation ~337,
creation ~244, projection ~216, durationBetween ~164, store ~102, string-parse ~74,
arithmetic ~48, comparison ~36, rendering ~26, accessors ~14.

---

## F-015 — No first-class temporal value; duration-as-object is an unbreakable wall

**Evidence.** Result rows carry `serde_json::Value` (`executor/types.rs:103-106`);
`types.rs` has no temporal enum. A native `NexusValue` exists and even reserves
`DURATION`/`DATE` variants (`value.rs:37-39,42`) but it is a **storage** encoding,
not threaded through eval/`Row`. Temporals are faked two ways in `fn_temporal.rs`:
date/time/datetime/localdatetime/localtime → `Value::String` (`:74-510`); duration →
`Value::Object` `{years,months,days,hours,minutes,seconds}` (`:256-289`). The TCK
comparator parses an expected cell like `'P14DT16H12M'` into a plain
`Value::String`, and `Object` never equals `String` (`tck_common/mod.rs:117-153,
313-321`).

**Impact.** **Every duration that reaches RETURN fails**, unconditionally — it is an
`Object`, the expectation is a `String`. String-faked *scalars* can pass only when
their rendering is byte-exact-canonical (the current 51 passes are those
coincidences). Accessors, comparison, truncation, and arithmetic are all impossible
to do correctly without a value that carries kind + components. **This is the gate:
nothing else in the cluster can be reliably fixed until a typed temporal value with
canonical rendering exists.**

**Confidence.** High (structural, proven against the matcher).

---

## F-016 — Timezone database is not wired in (but is deferrable)

**Evidence.** `chrono 0.4` is a direct dep (`crates/nexus-core/Cargo.toml:70`);
`chrono-tz`, `jiff`, and `time` resolve only **transitively** in `Cargo.lock` — no
nexus crate depends on them directly. `datetime()`/`time()` ignore the `timezone`
map key and use `chrono::Local` (`fn_temporal.rs:119,137,145-174`).

**Impact.** Named zones, DST, historical offsets (e.g. `[Europe/Stockholm]` →
`+00:53:28` in 1818), and `epochMillis` are unattainable with chrono alone. **But a
large fraction of the 935 needs no tz DB**: `date`, `localtime`, `localdatetime`,
and `duration` scenarios (creation, truncation, local accessors, duration
arithmetic) are tz-free. Promote `chrono-tz` (or adopt `jiff`) to a direct dep —
both already in `Cargo.lock`, so low supply-chain friction — but **schedule zoned
`datetime`/`time` after** the tz-free slice.

**Confidence.** High.

---

## F-017 — ISO-8601 parsing is a 2–3 format subset

**Evidence.** `date()` parses only `"%Y-%m-%d"` (`fn_temporal.rs:88`); no
week/ordinal/compact/partial forms. `duration()` accepts **only a map, never an ISO
string** (`fn_temporal.rs:256-289`), so `duration('P14DT16H12M')` returns `Null`.
`Temporal2.feature` exercises the full surface, including unit carry
(`'P5M1.5D' → 'P5M1DT12H'`, `'P0.75M' → 'P22DT19H51M49.5S'`).

**Impact.** Blocks the string-construction scenarios directly (~37) and underpins
every round-trip. Needs a real ISO-8601 parser (calendar/week/ordinal/compact/partial
dates, partial time + offsets, datetime, and ISO duration with fractional components
and carry). chrono cannot parse ISO week/ordinal/duration — another reason for
chrono-tz/jiff (F-016).

**Confidence.** High.

---

## F-018 — Accessors are top-level functions; the TCK uses property access

**Evidence.** `year()…nanosecond()` and duration `years()…seconds()` exist as
functions (`fn_temporal.rs:512-898`), but the corpus uses **property access**
(`d.year`, `d.epochMillis`, `d.weekYear`, `d.ordinalDay`, `d.offsetMinutes`, duration
total-vs-remainder pairs like `seconds` vs `secondsOfMinute`) — ~50 derived names
(`Temporal5.feature`). `PropertyAccess` only descends into `Value::Object`
(`eval/projection/core.rs:36,557-560`); on a `Value::String` it returns `Null`.

**Impact.** Every accessor scenario fails today. Requires a PropertyAccess dispatch
on temporals plus the ~50 derived component names. Depends on F-015 (the value must
carry components).

**Confidence.** High.

---

## F-019 — Whole operations are absent: truncation, duration ×/÷, canonical rendering

**Evidence.** No `truncate()` anywhere (grep finds only `Vec::truncate`) — yet
`Temporal9.feature` (truncation) is the **largest single sub-block** (~337 lines,
5 types × ~10 units). Arithmetic (`eval/temporal.rs:116-248`) covers date/datetime ±
duration and duration ± duration but is missing duration `×`/`÷` number, fractional
components, and `±duration` for time/localtime/localdatetime. There is no canonical
duration rendering (always a JSON object), no sub-second precision, no sign rules
(`'PT-1.999S'`), no unit normalisation (`seconds:70 → …M…S`), no tz/offset formatting
(`Temporal6.feature`).

**Impact.** Truncation alone is a large `L`; arithmetic completion and
durationBetween canonicalisation are each `M`. All depend on F-015 + F-017.

**Confidence.** High.

---

## F-020 — The plan: one gate, then stack semantics; no parser or format-contract risk

**Recommended strategy (least invasive, matches project precedent).** A
tagged-JSON-object convention for temporals in *intermediate* values (mirroring the
existing `_nexus_*` markers), with an **airtight canonicalisation pass at the
projection/RETURN boundary** that renders every tagged-temporal leaf to its ISO
`String`. The alternative — promoting `NexusValue` to the executor's runtime currency
— is correct long-term but a crate-wide `XL+` refactor; defer it.

- **No parser work.** Cypher has no temporal literals; temporals are always
  function-constructed, and their string/map arguments are ordinary literals
  (F-013 does not touch temporal).
- **No row-format conflict.** `CLAUDE.md` Constraint #1 governs the array-of-arrays
  row *shape*, not value rendering; temporals serialise as JSON strings inside the
  array exactly as Neo4j's HTTP API renders them. **The one hazard:** a tagged
  intermediate object must never leak into the `/cypher` response (it would render
  `{...}` instead of `"2015-07-21"` — a genuine format regression). The
  canonicalisation pass is therefore mandatory, not optional.

**Sizing verdict: XL** (largest cluster). Ordered sub-workstreams (→ = hard dep):

| # | Sub-workstream | Band | Depends |
|---|---|---|---|
| 1 | **Typed temporal value + canonical rendering + projection-boundary serialisation** (breaks the Object-vs-String wall) | **XL, the gate** | — |
| 2 | Full ISO-8601 parsing (dates + ISO duration with carry) | L | 1 |
| 3 | Creation-from-map incl. `timezone`, epoch | L | 1 (+11 for zones) |
| 4 | Property-access accessor dispatch + ~50 derived names | M | 1 |
| 5 | Comparison/ordering within kind; duration equality-only | M | 1 |
| 6 | Arithmetic completion (×/÷, fractional, all-type ±duration) | M | 1 |
| 7 | durationBetween family (canonical split, sign, DST) | M-L | 1,2 |
| 8 | Truncation (biggest sub-block) | L | 1,3 |
| 9 | Projection-from-temporal (select/override components) | M | 1,3,4 |
| 10 | Store/round-trip + arrays + null (needs `NexusValue` encoding) | M | 1 |
| 11 | **Timezone database** (chrono-tz/jiff): named zones, DST, historical offsets, epochMillis | M, hardest correctness | feeds 3,5,7,8 |

**Everything hinges on #1.** #11 (tz DB) is an independent hard sub-problem but
**deferrable** — the tz-free slice (date/local*/duration: creation, truncation,
local accessors, duration arithmetic) is a substantial share of the 935 and is
reachable first. **Confidence.** High on direction and ordering.
