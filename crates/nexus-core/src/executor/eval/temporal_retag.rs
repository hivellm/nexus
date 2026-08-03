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
fn strip_zone_bracket(s: &str) -> (&str, Option<&str>) {
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

/// Resolves an already-extracted `timezone` map value's string form to a
/// UTC offset in seconds — the one piece every `timezone` map key in this
/// crate shares: `fn_temporal.rs`'s `time`/`datetime` map constructors and
/// `temporal_truncate.rs`'s `<kind>.truncate(...)` override map. Kept as a
/// single function so both report byte-for-byte identical error text for
/// the one case they both reject — a named IANA zone (e.g.
/// `'Europe/Stockholm'`), which cannot be resolved to a real offset without
/// a timezone database (see `temporal_value::make_datetime`'s doc comment).
/// `'UTC'` resolves to `0`; anything [`strict_parse_offset`] itself accepts
/// (a numeric offset or `'Z'`) resolves through it unchanged. Each caller
/// still decides its own key-presence/JSON-type handling before calling
/// this — those legitimately differ (e.g. a missing key's default value).
pub(in crate::executor) fn resolve_timezone_string(tz: &str) -> crate::Result<i32> {
    if tz == "UTC" {
        return Ok(0);
    }
    strict_parse_offset(tz).ok_or_else(|| {
        crate::Error::CypherExecution(format!(
            "InvalidArgumentValue: timezone '{tz}' requires a timezone database, which is not \
             available; use a numeric UTC offset (e.g. '+02:00') or 'Z'/'UTC' instead"
        ))
    })
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

    #[test]
    fn resolve_timezone_string_accepts_utc_z_and_a_numeric_offset() {
        assert_eq!(resolve_timezone_string("UTC").unwrap(), 0);
        assert_eq!(resolve_timezone_string("Z").unwrap(), 0);
        assert_eq!(resolve_timezone_string("+02:00").unwrap(), 7200);
        assert_eq!(resolve_timezone_string("-01:00").unwrap(), -3600);
    }

    #[test]
    fn resolve_timezone_string_rejects_a_named_zone() {
        let err = resolve_timezone_string("Europe/Stockholm").unwrap_err();
        assert!(matches!(err, crate::Error::CypherExecution(_)));
    }
}
