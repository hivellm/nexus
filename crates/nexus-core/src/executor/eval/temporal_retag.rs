//! Strict re-derivation of a tagged temporal value from a stored (or
//! otherwise plain-`Value::String`) canonical ISO-8601 rendering.
//!
//! ## Design decision: re-derive on read, never store the tagged form
//!
//! Storage persists a node/relationship property map as one
//! `serde_json::to_vec`-serialized blob (see
//! `crate::storage::property_store::crud`); it has no type discriminator
//! beyond whatever shape the JSON itself carries. A temporal property
//! therefore reaches disk as the plain canonical ISO-8601 `Value::String`
//! [`super::temporal_value::canonicalize_value_in_place`] produces at the
//! CREATE/MERGE/SET write boundary — never the tagged
//! `{_nexus_temporal_type: ..., ...}` intermediate object. That "tagged
//! form must never reach disk" rule is a deliberate, previously-reviewed
//! invariant, restated in the project `CHANGELOG.md`: "internal
//! representations never reach disk or the wire".
//!
//! This module is the alternative this codebase chose over revisiting
//! that invariant: instead of a type-marked storage encoding (which would
//! mean auditing every write path — CREATE, MERGE, SET, REMOVE, FOREACH's
//! inline RETURN, bulk/WAL replay — against ever persisting or reading a
//! stale/legacy plain string once a versioned tagged encoding existed, and
//! would still require every READ site to canonicalize back to a string
//! anyway, since the wire format is frozen to the ISO string regardless of
//! what storage carries), the two read sites that need typed behaviour —
//! property access (`d.year`, wired into `projection::core`'s
//! `PropertyAccess` dispatch) and arithmetic (`temporal`'s duration
//! add/subtract/scale and datetime add/subtract/diff) — re-derive the
//! tagged value from the plain string they read, on demand, right where
//! it's needed.
//!
//! ### The string/temporal ambiguity trade-off
//!
//! Because storage cannot distinguish "a STRING property that happens to
//! read `'2015-07-21'`" from "a DATE property whose canonical rendering is
//! `'2015-07-21'`" — both are the identical JSON string on disk — this
//! module accepts that ambiguity deliberately rather than pretending it
//! doesn't exist: [`retag_canonical_string`] treats *any* string that
//! parses as one of the six canonical renderings as if it were that
//! temporal kind, for the narrow purpose of the caller's typed operation
//! (property access or arithmetic). This never changes what a plain
//! `RETURN`/property read yields (still the original, untouched string —
//! the wire format is not this module's concern) and never mutates
//! storage. To keep the false-positive surface as small as it can
//! possibly be, matching is intentionally **strict**:
//!
//! - Only the *exact* shapes [`super::temporal_value::canonicalize_temporal`]
//!   itself would produce are accepted — not the much larger openCypher
//!   input grammar a temporal *constructor* accepts (no compact
//!   `YYYYMMDD` dates, no ISO week dates, no truncated `YYYY-MM`, no
//!   non-canonical zero-padding). A string is accepted only if
//!   re-rendering the value parsed from it reproduces the original string
//!   byte-for-byte — the strongest defensive check available, since it
//!   catches any parser edge case a hand-rolled matcher alone would miss.
//! - A `duration` is disambiguated first and unconditionally by its `P`
//!   prefix, which no other kind's canonical rendering ever starts with
//!   (a leading `-P` is also probed, since a per-component-negative
//!   duration is a valid ISO-8601 shape in principle, but the canonical
//!   renderer never actually produces one — the byte-for-byte round-trip
//!   check above rejects it either way, so this never changes what's
//!   accepted, only what's attempted).
//! - Every other kind is disambiguated structurally before any numeric
//!   parsing happens: the presence/absence of `T` (date+time vs. date-only
//!   vs. time-only), a trailing `Z`/`±HH:MM[:SS]` offset (`time`/
//!   `datetime` vs. `localtime`/`localdatetime`), and a trailing
//!   `[Zone/Name]` bracket (`datetime` only).
//!
//! A user-controlled string property that is NOT one of these six exact
//! canonical shapes — including one that merely resembles a temporal in a
//! way our own renderer would never produce — is guaranteed to keep its
//! plain `STRING` behaviour: [`retag_canonical_string`] returns `None`,
//! and every call site falls back to ordinary string handling. This is
//! the load-bearing guarantee the regression suite's "non-canonical user
//! string must not gain temporal behaviour" control test exercises.
//!
//! ### Two further, narrower consequences of the same trade-off
//!
//! - **`+` between two stored temporal-shaped strings is arithmetic, not
//!   concatenation.** `add_values` (`arithmetic.rs`) tries datetime/
//!   duration arithmetic *before* the generic STRING+STRING
//!   concatenation fallback, so `dur.date + dur2.date` (openCypher TCK
//!   `Temporal8.feature` scenario 6 — both operands read from storage)
//!   resolves as duration addition. The unavoidable, narrow cost: a
//!   literal `'2015-07-21' + 'P1D'` (two ordinary strings the user wrote
//!   by hand, one of which happens to be an exact canonical duration
//!   shape) also does date arithmetic instead of string concatenation —
//!   strict-shape matching keeps this surface as small as it can be, but
//!   cannot eliminate it entirely without a typed storage encoding (see
//!   the design decision above). Any operand that ISN'T an exact
//!   canonical shape on the duration side still concatenates normally
//!   (`'P1D' + 'x'` → `'P1Dx'`, never arithmetic — see this module's test
//!   suite for the regression lock).
//! - **Old-format data can silently stop comparing equal after a
//!   canonical-rendering change.** This module's strict round-trip check
//!   means a rendering-format change (e.g. this same changeset's "omit
//!   seconds when zero, `Z` for a zero UTC offset" fix) is a real,
//!   user-visible compatibility break for data written by a *prior*
//!   version: a node property stored as `'12:00:00'` under the old
//!   renderer no longer matches `WHERE n.t = localtime('12:00:00')`,
//!   because `localtime('12:00:00')` now canonicalizes to `'12:00'` and
//!   the stored string was never rewritten. There is no migration path in
//!   this changeset — old rows keep their old string verbatim (still a
//!   perfectly readable plain `STRING` on `RETURN`) but stop being
//!   recognized as the matching temporal shape against freshly
//!   constructed values. This is judged acceptable (option (c): declare
//!   it, do not chase it) because the single-renderer round-trip
//!   invariant this module depends on for correctness is *exactly* what
//!   would have to be relaxed to keep old and new renderings both
//!   matching, which reopens the false-positive surface strict-shape
//!   matching exists to close. See `CHANGELOG.md` for the user-facing
//!   note.

use super::temporal_parse;
use super::temporal_value::{self, TemporalKind};
use serde_json::Value;

fn all_ascii_digits(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_digit())
}

/// Strips a trailing `[Zone/Name]` bracket suffix, if present, returning
/// `(base, zone_name)`. Only a `datetime` canonical rendering ever carries
/// one, and never an empty one (`make_datetime` never sets a zone name to
/// `""`) — an empty bracket (`'...Z[]'`) is therefore left un-stripped,
/// which fails every downstream shape check and correctly rejects the
/// whole string via [`retag_canonical_string`].
///
/// `pub(in crate::executor)`, not private: `projection::fn_temporal`'s
/// `datetime('...')` STRING constructor reuses this same stripping for a
/// fresh literal (not just a value re-derived from storage), so
/// `datetime('2017-10-28T23:00+02:00[Europe/Stockholm]')` round-trips the
/// zone name identically to a value that reached this module via storage.
pub(in crate::executor) fn strip_zone_bracket(s: &str) -> (&str, Option<&str>) {
    if let Some(stripped) = s.strip_suffix(']') {
        if let Some(open) = stripped.rfind('[') {
            let name = &stripped[open + 1..];
            if !name.is_empty() {
                return (&stripped[..open], Some(name));
            }
        }
    }
    (s, None)
}

/// Splits a time-and-offset tail into `(time_part, offset_part)` if it
/// carries a trailing offset (`Z` or `±HH:MM[:SS]`); `None` if it doesn't
/// (a `localtime`/`localdatetime` tail never does). A `+`/`-` can only
/// ever appear as the offset sign in this shape — the time-of-day portion
/// is digits, `:`, and `.` only — so the first one found unambiguously
/// starts the offset.
pub(in crate::executor) fn split_offset_suffix(s: &str) -> Option<(&str, &str)> {
    if let Some(stripped) = s.strip_suffix('Z') {
        return Some((stripped, "Z"));
    }
    for (idx, ch) in s.char_indices() {
        if ch == '+' || ch == '-' {
            return Some((&s[..idx], &s[idx..]));
        }
    }
    None
}

/// Strictly parses an offset suffix (`"Z"` or `"±HH:MM[:SS]"` — exactly
/// the shapes [`super::temporal_value::render_offset`] produces) into
/// signed seconds.
pub(in crate::executor) fn strict_parse_offset(s: &str) -> Option<i32> {
    if s == "Z" {
        return Some(0);
    }
    let (sign, rest) = match s.strip_prefix('+') {
        Some(r) => (1i32, r),
        None => (-1i32, s.strip_prefix('-')?),
    };
    let parts: Vec<&str> = rest.split(':').collect();
    let (hh, mm, ss) = match parts.as_slice() {
        [hh, mm] => (*hh, *mm, None),
        [hh, mm, ss] => (*hh, *mm, Some(*ss)),
        _ => return None,
    };
    if hh.len() != 2 || mm.len() != 2 || !all_ascii_digits(hh) || !all_ascii_digits(mm) {
        return None;
    }
    let hours: i32 = hh.parse().ok()?;
    let minutes: i32 = mm.parse().ok()?;
    let seconds: i32 = match ss {
        None => 0,
        Some(s) if s.len() == 2 && all_ascii_digits(s) => s.parse().ok()?,
        Some(_) => return None,
    };
    Some(sign * (hours * 3600 + minutes * 60 + seconds))
}

/// A wall-clock reading, threaded through [`resolve_timezone_string`] and
/// back out again — identical to the input on every path except the
/// "spring-forward gap" case (see that function's doc comment), where it
/// carries the shifted-forward reading instead.
pub(in crate::executor) type WallClock = (i32, u32, u32, u32, u32, u32, u32);

/// Resolves an already-extracted `timezone` map value's string form
/// against a specific local wall-clock reading (`year`/`month`/`day`/
/// `hour`/`minute`/`second`/`nanosecond`) — the one piece every `timezone`
/// map key in this crate shares: `fn_temporal.rs`'s `time`/`datetime` map
/// constructors and `temporal_truncate.rs`'s `<kind>.truncate(...)`
/// override map. Kept as a single function so every caller reports
/// byte-for-byte identical error text for the same malformed input, and so
/// the DST-aware named-zone resolution below (chrono-tz's bundled IANA
/// database) is implemented exactly once.
///
/// Returns `(offset_seconds, wall_clock)`. `'UTC'` resolves to `(0, input
/// unchanged)`; anything [`strict_parse_offset`] itself accepts (a numeric
/// offset or `'Z'`) resolves through it unchanged, `wall_clock` untouched.
/// A named IANA zone (e.g. `'Europe/Stockholm'`) resolves through
/// `chrono-tz`'s bundled tzdata, honoring the zone's real historical DST
/// rules — the offset at a given wall-clock reading depends on the date,
/// which is exactly why this function takes one instead of just the
/// `timezone` string. An unrecognised zone name is a hard, explicit error
/// (mirrors the pre-existing "malformed input" contract; no timezone
/// database gap remains).
///
/// ## DST edge cases (`chrono::LocalResult`)
///
/// - `Single`: the common case — one unambiguous offset.
/// - `Ambiguous(offset_before, offset_after)`: the wall-clock reading falls
///   in a fall-back "repeated hour" (two real UTC instants share it).
///   `chrono-tz`'s `offset_from_local_datetime` orders its two candidates
///   chronologically — the first is the offset active *before* the
///   transition (e.g. Stockholm's `+02:00` CEST immediately before an
///   October fall-back), the second is the offset *after* (`+01:00` CET).
///   This resolves to the FIRST (`offset_before`) — mirroring java.time's
///   documented default for an overlap (`ZoneRules.getOffset(LocalDateTime)`
///   returns `ZoneOffsetTransition.getOffsetBefore()` when
///   `LocalDateTime.atZone(ZoneId)` supplies no preferred offset), the
///   algorithm Neo4j's own temporal construction is built on. No
///   openCypher TCK row in this codebase's vendored corpus happens to pin
///   an ambiguous-hour construction directly, so this choice is verified
///   against java.time's documented default rather than a TCK fixture.
/// - `None`: the wall-clock reading falls in a spring-forward "skipped
///   hour" gap (no real instant has this local reading). java.time's
///   `ZonedDateTime.ofLocal` resolves a gap by shifting the local
///   date-time FORWARD by the transition's OWN DURATION
///   (`localDateTime.plusSeconds(gap)`) and resolving with the offset
///   that becomes active AFTER the gap — critically, this preserves both
///   the real elapsed instant AND injectivity (two distinct gap readings,
///   e.g. `02:30` and `02:59` in a one-hour gap, must shift to two
///   distinct results, `03:30` and `03:59`, not collapse to the same
///   post-gap instant). [`resolve_named_zone_gap`] computes the exact gap
///   duration as `offset_after - offset_before` (both found via
///   [`probe_adjacent_offset`], a galloping search — never a fixed
///   minute-granularity probe, since some historical transitions land on
///   a sub-minute boundary, e.g. Riga's 1926 change) and shifts the
///   ORIGINAL input by exactly that duration, rather than snapping to
///   whatever the first valid reading past the gap happens to be. Also
///   unverified by any TCK row in this corpus; documented per the same
///   java.time-parity rationale as the ambiguous case above.
pub(in crate::executor) fn resolve_timezone_string(
    tz: &str,
    year: i32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
    nanosecond: u32,
) -> crate::Result<(i32, WallClock)> {
    let wall_clock: WallClock = (year, month, day, hour, minute, second, nanosecond);
    if tz == "UTC" {
        return Ok((0, wall_clock));
    }
    if let Some(offset) = strict_parse_offset(tz) {
        return Ok((offset, wall_clock));
    }
    let zone = parse_named_zone(tz)?;
    resolve_named_zone(zone, wall_clock)
}

/// Resolves a `timezone` value against the CURRENT INSTANT rather than a
/// specific wall-clock reading — the shape `time({..., timezone: <zone>})`
/// needs, since `TIME` has no calendar date of its own to anchor a named
/// zone's DST lookup against. Deliberately host-timezone-independent
/// (`chrono::Utc::now()`, never `chrono::Local::now()`) and never mutates
/// the caller's requested hour/minute/second — mirrors java.time's
/// `ZoneRules.getOffset(Instant.now())`, which resolving a named zone
/// against a real, single UTC instant makes unambiguous by construction
/// (there is no `Ambiguous`/`None` case at the UTC side the way there is
/// resolving a local wall-clock reading). `'UTC'`/`'Z'`/a numeric offset
/// resolve exactly as [`resolve_timezone_string`] does, unaffected by the
/// current instant.
pub(in crate::executor) fn resolve_timezone_string_at_current_instant(
    tz: &str,
) -> crate::Result<i32> {
    if tz == "UTC" {
        return Ok(0);
    }
    if let Some(offset) = strict_parse_offset(tz) {
        return Ok(offset);
    }
    use chrono::{Offset, TimeZone};
    let zone = parse_named_zone(tz)?;
    let now = chrono::Utc::now().naive_utc();
    Ok(zone.offset_from_utc_datetime(&now).fix().local_minus_utc())
}

fn parse_named_zone(tz: &str) -> crate::Result<chrono_tz::Tz> {
    tz.parse().map_err(|_| {
        crate::Error::CypherExecution(format!(
            "InvalidArgumentValue: timezone '{tz}' is not a recognised UTC offset or IANA time \
             zone name"
        ))
    })
}

/// True when `tz` names a real IANA timezone (rather than `'UTC'`/`'Z'`/a
/// numeric offset) — the discriminator every `timezone` map-key caller
/// needs to decide whether the constructed value should carry a zone
/// name alongside the resolved offset (see [`super::temporal_value::make_datetime`]'s
/// `tz_name` parameter).
pub(in crate::executor) fn is_named_zone(tz: &str) -> bool {
    tz != "UTC" && strict_parse_offset(tz).is_none()
}

/// True when `tz` is a real, chrono-tz-recognised IANA zone identifier.
/// Distinct from [`is_named_zone`] (which only rules out `'UTC'`/`'Z'`/a
/// numeric offset without checking the name resolves to anything real):
/// the `datetime('...[Zone]')` STRING constructor needs this to validate
/// a bracket's zone name even on a literal that ALSO carries an explicit
/// written offset — a shape [`resolve_timezone_string`] itself never sees
/// (its own `Tz::from_str` validation only runs when an offset needs
/// deriving from the zone).
pub(in crate::executor) fn is_valid_named_zone(tz: &str) -> bool {
    tz.parse::<chrono_tz::Tz>().is_ok()
}

fn wall_clock_datetime(wall_clock: WallClock) -> Option<chrono::NaiveDateTime> {
    let (year, month, day, hour, minute, second, nanosecond) = wall_clock;
    let date = chrono::NaiveDate::from_ymd_opt(year, month, day)?;
    let time = chrono::NaiveTime::from_hms_nano_opt(hour, minute, second, nanosecond)?;
    Some(chrono::NaiveDateTime::new(date, time))
}

fn naive_datetime_to_wall_clock(dt: chrono::NaiveDateTime) -> WallClock {
    use chrono::{Datelike, Timelike};
    (
        dt.year(),
        dt.month(),
        dt.day(),
        dt.hour(),
        dt.minute(),
        dt.second(),
        dt.nanosecond(),
    )
}

fn resolve_named_zone(
    zone: chrono_tz::Tz,
    wall_clock: WallClock,
) -> crate::Result<(i32, WallClock)> {
    use chrono::{LocalResult, Offset, TimeZone};

    let (year, month, day, hour, minute, second, _) = wall_clock;
    let Some(local) = wall_clock_datetime(wall_clock) else {
        return Err(crate::Error::CypherExecution(format!(
            "InvalidArgumentValue: '{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}' \
             is not a valid wall-clock reading"
        )));
    };
    match zone.offset_from_local_datetime(&local) {
        LocalResult::Single(offset) => Ok((offset.fix().local_minus_utc(), wall_clock)),
        LocalResult::Ambiguous(offset_before, _offset_after) => {
            Ok((offset_before.fix().local_minus_utc(), wall_clock))
        }
        LocalResult::None => resolve_named_zone_gap(zone, local),
    }
}

/// The galloping-search bound (seconds) [`probe_adjacent_offset`] gives up
/// at — roughly 3 days. Most IANA gaps are an hour or two (ordinary DST
/// spring-forward), but three real zones skip a full calendar day by
/// crossing the international date line: `Pacific/Apia` (2011-12-30,
/// UTC-11 -> UTC+13, Samoa's date-line switch), `Pacific/Kiritimati`, and
/// `Pacific/Kanton` (both 1994-12-31). A bound smaller than 86,400 seconds
/// hard-errors on those three specific dates instead of resolving them —
/// verified to resolve exactly to java.time's answer (offset_after, wall
/// clock shifted by exactly one day) in at most 66 probes each. No two
/// gaps in the bundled tzdata sit within 3 days of each other, so this
/// bound cannot skip past a legitimately short adjacent timespan.
const GAP_PROBE_BOUND_SECONDS: i64 = 262_144;

/// Finds the UTC offset of the timespan immediately touching a
/// spring-forward gap, in `direction` (`1` searches forward for the
/// offset that becomes active AFTER the gap, `-1` searches backward for
/// the offset that was active BEFORE it) — a galloping search (exponential
/// doubling from one second, bounded by [`GAP_PROBE_BOUND_SECONDS`])
/// followed by a binary-search refinement within the last doubling
/// interval, so the reading found sits at the CLOSEST valid second to the
/// real transition boundary rather than wherever the exponential growth
/// happened to land — both minimizing the (already remote) risk of
/// skipping past a short adjacent timespan into a third one, and
/// resolving to exact-second precision, needed for the handful of
/// pre-1900s zones whose historical transitions do not land on a whole
/// minute (e.g. Riga's 1926 change).
fn probe_adjacent_offset(
    zone: chrono_tz::Tz,
    local: chrono::NaiveDateTime,
    direction: i64,
) -> Option<i32> {
    use chrono::{LocalResult, Offset, TimeZone};

    let offset_at = |delta: i64| -> Option<i32> {
        let probe = local + chrono::Duration::seconds(direction * delta);
        match zone.offset_from_local_datetime(&probe) {
            LocalResult::Single(offset) => Some(offset.fix().local_minus_utc()),
            LocalResult::Ambiguous(offset_before, _offset_after) => {
                Some(offset_before.fix().local_minus_utc())
            }
            LocalResult::None => None,
        }
    };

    let mut lo = 0i64;
    let mut hi = 1i64;
    while offset_at(hi).is_none() {
        if hi >= GAP_PROBE_BOUND_SECONDS {
            return None;
        }
        lo = hi;
        hi *= 2;
    }
    while hi - lo > 1 {
        let mid = lo + (hi - lo) / 2;
        if offset_at(mid).is_some() {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    offset_at(hi)
}

/// Resolves a "spring-forward gap" wall-clock reading (see
/// [`resolve_timezone_string`]'s doc comment) by computing the exact gap
/// duration (`offset_after - offset_before`, both from
/// [`probe_adjacent_offset`]) and shifting the ORIGINAL `local` reading
/// forward by that duration — mirroring java.time's
/// `ZonedDateTime.ofLocal` (`localDateTime.plusSeconds(gap)`,
/// `offsetAfter`) exactly, including its injectivity: two distinct inputs
/// inside the same gap shift to two distinct outputs, never collapsing to
/// the same post-gap instant.
fn resolve_named_zone_gap(
    zone: chrono_tz::Tz,
    local: chrono::NaiveDateTime,
) -> crate::Result<(i32, WallClock)> {
    let unresolvable = || {
        crate::Error::CypherExecution(format!(
            "InvalidArgumentValue: could not resolve timezone '{}': no valid wall-clock reading \
             found within roughly {} hours of the given instant",
            zone.name(),
            GAP_PROBE_BOUND_SECONDS / 3600
        ))
    };
    let offset_before = probe_adjacent_offset(zone, local, -1).ok_or_else(unresolvable)?;
    let offset_after = probe_adjacent_offset(zone, local, 1).ok_or_else(unresolvable)?;
    let gap_seconds = i64::from(offset_after) - i64::from(offset_before);
    let shifted = local
        .checked_add_signed(chrono::Duration::seconds(gap_seconds))
        .ok_or_else(|| {
            crate::Error::CypherExecution(
                "InvalidArgumentValue: spring-forward gap shift is outside the representable \
                 date range"
                    .to_string(),
            )
        })?;
    Ok((offset_after, naive_datetime_to_wall_clock(shifted)))
}

/// Strictly parses a bare time-of-day (`HH:MM[:SS[.fraction]]` — exactly
/// the shape [`super::temporal_value::canonicalize_temporal`]'s time
/// rendering produces, including its "omit seconds when zero" rule)
/// into `(hour, minute, second, nanosecond)`.
pub(in crate::executor) fn strict_parse_time(s: &str) -> Option<(u32, u32, u32, u32)> {
    let parts: Vec<&str> = s.split(':').collect();
    let (hh, mm, rest_sec) = match parts.as_slice() {
        [hh, mm] => (*hh, *mm, None),
        [hh, mm, ss] => (*hh, *mm, Some(*ss)),
        _ => return None,
    };
    if hh.len() != 2 || mm.len() != 2 || !all_ascii_digits(hh) || !all_ascii_digits(mm) {
        return None;
    }
    let hour: u32 = hh.parse().ok()?;
    let minute: u32 = mm.parse().ok()?;
    let (second, nanosecond) = match rest_sec {
        None => (0u32, 0u32),
        Some(ss_frac) => {
            let (ss, frac) = match ss_frac.split_once('.') {
                Some((s0, f0)) => (s0, Some(f0)),
                None => (ss_frac, None),
            };
            if ss.len() != 2 || !all_ascii_digits(ss) {
                return None;
            }
            let sec: u32 = ss.parse().ok()?;
            let nanos = match frac {
                None => 0u32,
                Some(f) => {
                    // `format_nanos_fraction` always trims trailing zeros —
                    // a genuine canonical fraction never ends in '0', and
                    // is never empty or longer than 9 digits.
                    if f.is_empty() || f.len() > 9 || f.ends_with('0') || !all_ascii_digits(f) {
                        return None;
                    }
                    let padded = format!("{f:0<9}");
                    padded.parse().ok()?
                }
            };
            (sec, nanos)
        }
    };
    if hour > 23 || minute > 59 || second > 59 {
        return None;
    }
    Some((hour, minute, second, nanosecond))
}

/// Strictly parses a leading `YYYY-MM-DD` (or negative-year `-YYYY-MM-DD`)
/// date prefix, returning the parsed components and the unconsumed
/// remainder (empty for a bare `date`, or starting with `T` for a
/// date+time form).
fn strict_split_date_prefix(s: &str) -> Option<((i32, u32, u32), &str)> {
    let (sign, rest) = match s.strip_prefix('-') {
        Some(r) => (-1i32, r),
        None => (1i32, s),
    };
    let dash1 = rest.find('-')?;
    let year_str = rest.get(..dash1)?;
    if year_str.len() < 4 || !all_ascii_digits(year_str) {
        return None;
    }
    let year: i32 = year_str.parse().ok()?;
    let year = year.checked_mul(sign)?;
    let after_year = rest.get(dash1 + 1..)?;
    let month_str = after_year.get(..2)?;
    if after_year.get(2..3)? != "-" {
        return None;
    }
    let day_str = after_year.get(3..5)?;
    if !all_ascii_digits(month_str) || !all_ascii_digits(day_str) {
        return None;
    }
    let month: u32 = month_str.parse().ok()?;
    let day: u32 = day_str.parse().ok()?;
    chrono::NaiveDate::from_ymd_opt(year, month, day)?;
    let consumed = if sign < 0 { 1 } else { 0 } + dash1 + 1 + 5;
    let suffix = s.get(consumed..)?;
    Some(((year, month, day), suffix))
}

/// Strictly parses `s` as a canonical ISO-8601 rendering this module's own
/// [`super::temporal_value::canonicalize_temporal`] would produce, and if
/// so, returns the re-derived tagged intermediate value. Returns `None`
/// for anything else — including syntactically-plausible-but-non-canonical
/// ISO forms and, most importantly, any plain user string that merely
/// happens to look like one. See the module doc comment for the full
/// design rationale and the strict-shape/round-trip-verified matching
/// this relies on.
pub(in crate::executor) fn retag_canonical_string(s: &str) -> Option<Value> {
    let candidate = build_candidate(s)?;
    (temporal_value::canonicalize_temporal(&candidate).as_deref() == Some(s)).then_some(candidate)
}

fn build_candidate(s: &str) -> Option<Value> {
    if s.starts_with('P') || s.starts_with("-P") {
        let (months, days, seconds, nanos) = temporal_parse::parse_iso_duration(s)?;
        return Some(temporal_value::make_duration(months, days, seconds, nanos));
    }

    let (base, zone) = strip_zone_bracket(s);

    if let Some(((year, month, day), suffix)) = strict_split_date_prefix(base) {
        if suffix.is_empty() {
            if zone.is_some() {
                return None;
            }
            return Some(temporal_value::make_date(year, month, day));
        }
        let time_and_offset = suffix.strip_prefix('T')?;
        if let Some((time_part, offset_part)) = split_offset_suffix(time_and_offset) {
            let (hour, minute, second, nanosecond) = strict_parse_time(time_part)?;
            let offset = strict_parse_offset(offset_part)?;
            return Some(temporal_value::make_datetime(
                year,
                month,
                day,
                hour,
                minute,
                second,
                nanosecond,
                offset,
                zone.map(str::to_string),
            ));
        }
        if zone.is_some() {
            return None; // LocalDateTime never carries a zone bracket.
        }
        let (hour, minute, second, nanosecond) = strict_parse_time(time_and_offset)?;
        return Some(temporal_value::make_localdatetime(
            year, month, day, hour, minute, second, nanosecond,
        ));
    }

    if zone.is_some() {
        return None; // Only DateTime carries a zone bracket.
    }
    if let Some((time_part, offset_part)) = split_offset_suffix(base) {
        let (hour, minute, second, nanosecond) = strict_parse_time(time_part)?;
        let offset = strict_parse_offset(offset_part)?;
        return Some(temporal_value::make_time(
            hour, minute, second, nanosecond, offset,
        ));
    }
    let (hour, minute, second, nanosecond) = strict_parse_time(base)?;
    Some(temporal_value::make_localtime(
        hour, minute, second, nanosecond,
    ))
}

/// True when `retag_canonical_string(s)` would re-derive a tagged
/// `duration` — the narrow check the arithmetic layer needs (it never
/// needs the other five kinds re-derived as a standalone step; every
/// datetime-shaped operand is coerced through
/// [`super::temporal::coerce_temporal_instant`] instead, which already
/// handles both tagged and plain-string forms uniformly since a datetime
/// arithmetic delta only ever needs the operand's *rendered string*, not
/// its parsed components).
pub(in crate::executor) fn retag_duration(value: &Value) -> Option<Value> {
    match value {
        Value::Object(_)
            if temporal_value::temporal_kind(value) == Some(TemporalKind::Duration) =>
        {
            Some(value.clone())
        }
        Value::String(s) => {
            // Fast-path guard: every canonical duration rendering starts
            // with `P` (or `-P` for a leading-minus-negated one) —
            // rejecting anything else here avoids paying
            // `retag_canonical_string`'s full parse-and-verify cost (it
            // tries every temporal kind's grammar in turn) on a
            // date/time/datetime-shaped string, which is by far the
            // common case wherever this runs inside a sort comparator.
            if !(s.starts_with('P') || s.starts_with("-P")) {
                return None;
            }
            let candidate = retag_canonical_string(s)?;
            (temporal_value::temporal_kind(&candidate) == Some(TemporalKind::Duration))
                .then_some(candidate)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::executor::eval::temporal_value::{
        make_date, make_datetime, make_duration, make_localdatetime, make_localtime, make_time,
    };

    fn round_trips(s: &str) -> Option<Value> {
        retag_canonical_string(s)
    }

    #[test]
    fn retags_a_stored_date() {
        let v = round_trips("1984-10-11").expect("must retag");
        assert_eq!(v, make_date(1984, 10, 11));
    }

    #[test]
    fn retags_a_stored_localtime_omitting_zero_seconds() {
        let v = round_trips("12:00").expect("must retag");
        assert_eq!(v, make_localtime(12, 0, 0, 0));
    }

    #[test]
    fn retags_a_stored_localtime_with_fraction() {
        let v = round_trips("12:31:14.645876123").expect("must retag");
        assert_eq!(v, make_localtime(12, 31, 14, 645_876_123));
    }

    #[test]
    fn retags_a_stored_time_with_zero_offset_as_z() {
        let v = round_trips("12:00Z").expect("must retag");
        assert_eq!(v, make_time(12, 0, 0, 0, 0));
    }

    #[test]
    fn retags_a_stored_time_with_explicit_offset() {
        let v = round_trips("12:31:14.645876123+01:00").expect("must retag");
        assert_eq!(v, make_time(12, 31, 14, 645_876_123, 3600));
    }

    #[test]
    fn retags_a_stored_localdatetime() {
        let v = round_trips("1912-01-01T00:00").expect("must retag");
        assert_eq!(v, make_localdatetime(1912, 1, 1, 0, 0, 0, 0));
    }

    #[test]
    fn retags_a_stored_datetime_zero_offset_as_z() {
        let v = round_trips("1912-01-01T00:00Z").expect("must retag");
        assert_eq!(v, make_datetime(1912, 1, 1, 0, 0, 0, 0, 0, None));
    }

    #[test]
    fn retags_a_stored_datetime_with_zone_bracket() {
        let v = round_trips("2020-01-01T12:31:14+01:00[Europe/Stockholm]").expect("must retag");
        assert_eq!(
            v,
            make_datetime(
                2020,
                1,
                1,
                12,
                31,
                14,
                0,
                3600,
                Some("Europe/Stockholm".to_string())
            )
        );
    }

    #[test]
    fn retags_a_stored_duration() {
        let v = round_trips("P14DT16H12M").expect("must retag");
        assert_eq!(v, make_duration(0, 14, 16 * 3600 + 12 * 60, 0));
    }

    #[test]
    fn retags_a_negative_component_duration() {
        let v = round_trips("PT-1M-0.001S").expect("must retag");
        assert_eq!(v, make_duration(0, 0, -60, -1_000_000));
    }

    // ── The critical control: plain user strings never gain temporal
    // behaviour, including near-miss shapes our own renderer never
    // produces. ──────────────────────────────────────────────────────────

    #[test]
    fn rejects_an_ordinary_word_string() {
        assert_eq!(round_trips("hello world"), None);
    }

    #[test]
    fn rejects_a_non_canonical_padded_time() {
        // Our renderer never emits ":00" seconds when they're zero.
        assert_eq!(round_trips("12:00:00"), None);
    }

    #[test]
    fn rejects_a_non_canonical_offset_form() {
        // Our renderer never emits "+00:00" — zero offset is always "Z".
        assert_eq!(round_trips("12:00:00+00:00"), None);
    }

    #[test]
    fn rejects_a_compact_date_form() {
        // A valid openCypher constructor input, but not a canonical
        // rendering — `canonicalize_temporal` never emits it.
        assert_eq!(round_trips("19841011"), None);
    }

    #[test]
    fn rejects_a_string_with_trailing_garbage() {
        assert_eq!(round_trips("1984-10-11-extra"), None);
    }

    #[test]
    fn rejects_a_string_that_merely_starts_with_p() {
        assert_eq!(round_trips("Portland"), None);
    }

    #[test]
    fn rejects_an_out_of_range_calendar_date() {
        assert_eq!(round_trips("2015-02-30"), None);
    }

    #[test]
    fn rejects_an_empty_zone_bracket() {
        // `make_datetime` never sets a zone name to `""` — a canonical
        // rendering never has an empty bracket, so this must not be
        // silently stripped and accepted as a zone-less DateTime either.
        assert_eq!(round_trips("2020-01-01T12:31:14Z[]"), None);
    }

    #[test]
    fn retag_duration_accepts_stored_string_and_tagged_alike() {
        assert!(retag_duration(&Value::String("PT12S".to_string())).is_some());
        assert!(retag_duration(&make_duration(0, 0, 12, 0)).is_some());
        assert_eq!(retag_duration(&make_date(2020, 1, 1)), None);
        assert_eq!(
            retag_duration(&Value::String("1984-10-11".to_string())),
            None
        );
    }

    /// Shorthand for the TCK's `1984-10-11T12:31:14` wall-clock reading
    /// (`Temporal1.feature` scenario 10's plain `year/month/day` rows).
    fn wc(year: i32, month: u32, day: u32, hour: u32, minute: u32, second: u32) -> WallClock {
        (year, month, day, hour, minute, second, 0)
    }

    #[test]
    fn resolve_timezone_string_accepts_utc_z_and_a_numeric_offset() {
        let any_wall_clock = wc(2020, 1, 1, 0, 0, 0);
        assert_eq!(
            resolve_timezone_string("UTC", 2020, 1, 1, 0, 0, 0, 0).unwrap(),
            (0, any_wall_clock)
        );
        assert_eq!(
            resolve_timezone_string("Z", 2020, 1, 1, 0, 0, 0, 0).unwrap(),
            (0, any_wall_clock)
        );
        assert_eq!(
            resolve_timezone_string("+02:00", 2020, 1, 1, 0, 0, 0, 0).unwrap(),
            (7200, any_wall_clock)
        );
        assert_eq!(
            resolve_timezone_string("-01:00", 2020, 1, 1, 0, 0, 0, 0).unwrap(),
            (-3600, any_wall_clock)
        );
    }

    #[test]
    fn resolve_timezone_string_resolves_a_named_zone_in_winter() {
        // openCypher TCK `Temporal1.feature` scenario [10]: Stockholm is
        // `+01:00` (CET, standard time) on 11 October.
        let (offset, wall_clock) =
            resolve_timezone_string("Europe/Stockholm", 1984, 10, 11, 12, 31, 14, 645_876_123)
                .unwrap();
        assert_eq!(offset, 3600);
        assert_eq!(wall_clock, (1984, 10, 11, 12, 31, 14, 645_876_123));
    }

    #[test]
    fn resolve_timezone_string_resolves_a_named_zone_in_summer() {
        // openCypher TCK `Temporal1.feature` scenario [10] (`ordinalDay:
        // 202` row -> 20 July): Stockholm is `+02:00` (CEST, daylight
        // saving) in mid-summer.
        let (offset, _) =
            resolve_timezone_string("Europe/Stockholm", 1984, 7, 20, 12, 0, 0, 0).unwrap();
        assert_eq!(offset, 7200);
    }

    #[test]
    fn resolve_timezone_string_ambiguous_fall_back_hour_picks_the_earlier_offset() {
        // Stockholm's 2017 fall-back transition: clocks go from
        // 03:00 CEST (+02:00) back to 02:00 CET (+01:00), so the wall-clock
        // hour 02:00-03:00 occurs twice on 29 October 2017. Neither
        // occurrence is pinned by a TCK row directly (Temporal10 scenario
        // [8]'s fixtures use hour 0 and hour 4, both unambiguous) — this
        // locks in java.time's documented default for an overlap (the
        // EARLIER, pre-transition offset — see `resolve_timezone_string`'s
        // doc comment).
        let (offset, wall_clock) =
            resolve_timezone_string("Europe/Stockholm", 2017, 10, 29, 2, 30, 0, 0).unwrap();
        assert_eq!(offset, 7200, "must pick the earlier (+02:00 CEST) offset");
        assert_eq!(
            wall_clock,
            (2017, 10, 29, 2, 30, 0, 0),
            "an ambiguous (not skipped) reading is never wall-clock-shifted"
        );
    }

    #[test]
    fn resolve_timezone_string_spring_forward_gap_shifts_by_the_exact_gap_duration() {
        // Stockholm's 2017 spring-forward transition: clocks jump from
        // 01:59:59 CET (+01:00) straight to 03:00:00 CEST (+02:00) on 26
        // March 2017 (a one-hour gap) — the wall-clock reading
        // 02:00-02:59:59 never occurs that day. Not pinned by any TCK row
        // in this corpus; documented per java.time's `ZonedDateTime.ofLocal`
        // gap algorithm (shift the ORIGINAL local reading forward by the
        // gap's own exact duration, then resolve with the post-transition
        // offset) — see `resolve_timezone_string`'s doc comment. Two
        // distinct gap readings (`02:30`/`02:59`) must shift to two
        // distinct results (`03:30`/`03:59`), never collapsing to the
        // same post-gap instant the way snapping to "the first valid
        // minute" would.
        let (offset, wall_clock) =
            resolve_timezone_string("Europe/Stockholm", 2017, 3, 26, 2, 30, 0, 0).unwrap();
        assert_eq!(
            offset, 7200,
            "must resolve using the post-gap +02:00 CEST offset"
        );
        assert_eq!(
            wall_clock,
            (2017, 3, 26, 3, 30, 0, 0),
            "02:30 shifted forward by the exact one-hour gap duration is 03:30"
        );

        let (_, wall_clock_59) =
            resolve_timezone_string("Europe/Stockholm", 2017, 3, 26, 2, 59, 0, 0).unwrap();
        assert_eq!(
            wall_clock_59,
            (2017, 3, 26, 3, 59, 0, 0),
            "02:59 must shift to 03:59, distinct from 02:30's 03:30 (injectivity)"
        );
    }

    #[test]
    fn resolve_timezone_string_spring_forward_gap_handles_a_non_hour_length_gap() {
        // Lord Howe Island uses a unique 30-minute DST offset (not a full
        // hour): standard `+10:30` to daylight `+11:00`. On 4 October 2020
        // (the first Sunday in October, Lord Howe's DST start), clocks
        // jump from 01:59:59 to 02:30:00 — the wall-clock reading
        // 02:00-02:29:59 never occurs. Verified against chrono-tz's own
        // resolution (`Australia/Lord_Howe`'s bundled IANA rules), not a
        // TCK row.
        let (offset, wall_clock) =
            resolve_timezone_string("Australia/Lord_Howe", 2020, 10, 4, 2, 15, 0, 0).unwrap();
        assert_eq!(
            offset, 39_600,
            "must resolve using the post-gap +11:00 offset"
        );
        assert_eq!(
            wall_clock,
            (2020, 10, 4, 2, 45, 0, 0),
            "02:15 shifted forward by the exact 30-minute gap is 02:45"
        );
    }

    #[test]
    fn resolve_timezone_string_spring_forward_gap_resolves_a_sub_minute_historical_transition() {
        // Riga's 1926 transition from Local Mean Time (`+01:36:34`) to
        // Eastern European Time (`+02:00:00`, exactly) is a genuine
        // sub-minute-duration gap: `7200 - 5794 = 1406` seconds (23
        // minutes 26 seconds), not a whole number of minutes. A
        // minute-granularity probe (the prior implementation) could only
        // ever land on a whole-minute reading, overshooting the true
        // shifted instant by up to 59 seconds; this crate's
        // exact-second galloping search resolves it precisely. Verified
        // against chrono-tz's own resolution, not a TCK row (no scenario
        // in this corpus's vendored TCK exercises a pre-1900s European
        // zone's LMT-to-standard transition).
        let (offset, wall_clock) =
            resolve_timezone_string("Europe/Riga", 1926, 5, 11, 0, 0, 0, 0).unwrap();
        assert_eq!(
            offset, 7200,
            "must resolve using the post-gap +02:00 EET offset"
        );
        assert_eq!(
            wall_clock,
            (1926, 5, 11, 0, 23, 26, 0),
            "00:00 shifted forward by the exact 1406-second (23m26s) gap"
        );
    }

    #[test]
    fn resolve_timezone_string_spring_forward_gap_spans_a_full_day_across_the_date_line() {
        // Samoa's 2011-12-30 date-line switch: `Pacific/Apia` skips the
        // entire calendar day, jumping straight from 29 December 23:59:59
        // (`-11:00`) to 31 December 00:00:00 (`+14:00`) — a 24-hour gap
        // (`GAP_PROBE_BOUND_SECONDS` must cover this; a bound sized only
        // for an ordinary DST spring-forward would hard-error here
        // instead of resolving it). Verified to match java.time's own
        // answer for this well-known transition (`offset_after`, wall
        // clock shifted forward by exactly one day) — not pinned by any
        // TCK row in this corpus.
        let (offset, wall_clock) =
            resolve_timezone_string("Pacific/Apia", 2011, 12, 30, 12, 0, 0, 0).unwrap();
        assert_eq!(
            offset, 50_400,
            "must resolve using the post-gap +14:00 offset"
        );
        assert_eq!(
            wall_clock,
            (2011, 12, 31, 12, 0, 0, 0),
            "30 December shifted forward by the exact one-day gap lands on 31 December"
        );
    }

    #[test]
    fn resolve_timezone_string_rejects_an_unknown_zone_name() {
        let err = resolve_timezone_string("Not/AZone", 2020, 1, 1, 0, 0, 0, 0).unwrap_err();
        assert!(matches!(err, crate::Error::CypherExecution(_)));
    }

    #[test]
    fn is_named_zone_distinguishes_a_real_zone_from_utc_and_a_numeric_offset() {
        assert!(is_named_zone("Europe/Stockholm"));
        assert!(!is_named_zone("UTC"));
        assert!(!is_named_zone("Z"));
        assert!(!is_named_zone("+02:00"));
    }
}
