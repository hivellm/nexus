//! Shared pure helpers for openCypher-TCK-shaped Gherkin runners.
//!
//! `tests/tck_common/mod.rs` is treated by Cargo as a regular module
//! (not its own test binary) because it lives under `tests/<subdir>/`.
//! Any `[[test]]` binary in this crate can pull it in via `mod
//! tck_common;` to reuse result-table comparison and TCK cell parsing
//! without duplicating the logic.

use cucumber::gherkin;
use nexus_core::executor::ResultSet;
use serde_json::Value;

// ─────────────────── Result-table comparison ───────────────────

/// Compare a captured `ResultSet` against a TCK Gherkin table.
///
/// The first row of the table is the header (column names). The
/// remaining rows are expected values. When `ordered = false` the
/// rows are compared as multisets; when `ordered = true` row order
/// must match.
///
/// Cell text is normalised to JSON via `tck_cell_to_json` before
/// comparison; floats are compared with a 1e-9 absolute tolerance.
pub fn compare_table(result: &ResultSet, table: &gherkin::Table, ordered: bool) {
    let rows = &table.rows;
    assert!(
        !rows.is_empty(),
        "TCK table must have at least the header row"
    );
    let header = &rows[0];
    let expected_rows: Vec<&Vec<String>> = rows.iter().skip(1).collect();

    // Column-count check.
    assert_eq!(
        result.columns.len(),
        header.len(),
        "column-count mismatch: result has {} columns ({:?}), table has {} ({:?})",
        result.columns.len(),
        result.columns,
        header.len(),
        header,
    );
    // Column-name check (TCK headers are positional, not by-name —
    // but Nexus column names should align in well-formed scenarios).
    for (pos, name) in header.iter().enumerate() {
        assert_eq!(
            &result.columns[pos], name,
            "column[{pos}] name mismatch: result={:?}, table={:?}",
            result.columns[pos], name
        );
    }

    // Row-count check.
    assert_eq!(
        result.rows.len(),
        expected_rows.len(),
        "row-count mismatch: result has {} rows, table has {}",
        result.rows.len(),
        expected_rows.len()
    );

    // Build the expected `Vec<Vec<Value>>` once.
    let expected: Vec<Vec<Value>> = expected_rows
        .iter()
        .map(|row| row.iter().map(|cell| tck_cell_to_json(cell)).collect())
        .collect();

    let actual: Vec<Vec<Value>> = result.rows.iter().map(|row| row.values.clone()).collect();

    if ordered {
        for (i, (got, want)) in actual.iter().zip(expected.iter()).enumerate() {
            assert_rows_equal(got, want, i);
        }
    } else {
        // Multiset comparison: every expected row must match exactly
        // one unmatched actual row.
        let mut taken = vec![false; actual.len()];
        for (wi, want) in expected.iter().enumerate() {
            let pos = actual
                .iter()
                .enumerate()
                .position(|(i, got)| !taken[i] && rows_equal(got, want));
            match pos {
                Some(p) => taken[p] = true,
                None => panic!(
                    "expected row #{wi} {:?} not found in result {:?}",
                    want, actual
                ),
            }
        }
    }
}

pub fn rows_equal(got: &[Value], want: &[Value]) -> bool {
    // `values_equal(a, b)` requires `a` to be the expected (TCK-tagged) value
    // and `b` to be the actual Nexus value — its marker dispatch (`@tck_node`
    // / `@tck_rel` / `@tck_path` / `@tck_float`) only ever inspects the first
    // argument. Pass `want` (expected) first so graph-element and
    // special-float cells route into their structural matchers instead of
    // silently falling through to the generic key-set comparison, which can
    // never match Nexus's `_nexus_id`/`_nexus_labels`-carrying result shape.
    got.len() == want.len() && got.iter().zip(want.iter()).all(|(g, w)| values_equal(w, g))
}

pub fn assert_rows_equal(got: &[Value], want: &[Value], idx: usize) {
    assert!(
        rows_equal(got, want),
        "row[{idx}] mismatch:\n  got:  {got:?}\n  want: {want:?}"
    );
}

/// Tolerant value comparison.
///
/// `a` is the value parsed from the TCK table cell (expected); `b` is
/// the Nexus result value (actual).
///
/// - Floats: 1e-9 absolute tolerance.
/// - Numbers: integer-vs-float coercion allowed (`1` == `1.0`).
/// - Maps: unordered, every key must match.
/// - Lists: ordered.
/// - Node/relationship/path literals (tagged via `@tck_node` /
///   `@tck_rel` / `@tck_path`, see `tck_cell_to_json`): matched
///   structurally against Nexus's node/relationship/path value shapes.
pub fn values_equal(a: &Value, b: &Value) -> bool {
    if tck_marker(a, "@tck_node") {
        return tck_node_matches(a, b);
    }
    if tck_marker(a, "@tck_rel") {
        return tck_rel_matches(a, b);
    }
    if tck_marker(a, "@tck_path") {
        return tck_path_matches(a, b);
    }
    if a.get("@tck_float").is_some() {
        return tck_float_matches(a, b);
    }
    match (a, b) {
        (Value::Null, Value::Null) => true,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::String(x), Value::String(y)) => x == y,
        (Value::Number(x), Value::Number(y)) => {
            let xf = x.as_f64();
            let yf = y.as_f64();
            match (xf, yf) {
                (Some(xv), Some(yv)) => (xv - yv).abs() < 1e-9,
                _ => x == y,
            }
        }
        (Value::Array(xa), Value::Array(ya)) => {
            xa.len() == ya.len() && xa.iter().zip(ya.iter()).all(|(x, y)| values_equal(x, y))
        }
        (Value::Object(xo), Value::Object(yo)) => {
            xo.len() == yo.len()
                && xo
                    .iter()
                    .all(|(k, xv)| yo.get(k).is_some_and(|yv| values_equal(xv, yv)))
        }
        _ => false,
    }
}

/// True when `v` is a JSON object carrying the boolean marker `key`
/// set to `true` (used for the `@tck_node` / `@tck_rel` / `@tck_path`
/// tags produced by the TCK cell parser).
fn tck_marker(v: &Value, key: &str) -> bool {
    v.get(key) == Some(&Value::Bool(true))
}

/// Match a parsed TCK node literal (`expected`) against a Nexus result
/// value (`actual`).
///
/// Nexus emits node labels under the reserved `_nexus_labels` key; the
/// literal's `@labels` are compared against it as a set (openCypher labels
/// are unordered), and the remaining properties must match exactly.
fn tck_node_matches(expected: &Value, actual: &Value) -> bool {
    let Some(actual_obj) = actual.as_object() else {
        return false;
    };
    if !actual_obj.contains_key("_nexus_id") || actual_obj.contains_key("_nexus_rel_type") {
        return false;
    }
    let expected_obj = expected.as_object().expect("tck node is always an object");
    if !label_sets_equal(
        expected_obj.get("@labels").and_then(Value::as_array),
        actual_obj.get("_nexus_labels").and_then(Value::as_array),
    ) {
        return false;
    }
    props_match(
        expected_obj,
        &["@tck_node", "@labels"],
        actual_obj,
        &["_nexus_id", "_nexus_labels"],
    )
}

/// Compare two label lists (either may be absent ⇒ empty) as unordered sets
/// of strings.
fn label_sets_equal(a: Option<&Vec<Value>>, b: Option<&Vec<Value>>) -> bool {
    let to_set = |v: Option<&Vec<Value>>| -> std::collections::BTreeSet<String> {
        v.into_iter()
            .flatten()
            .filter_map(|x| x.as_str().map(str::to_owned))
            .collect()
    };
    to_set(a) == to_set(b)
}

/// Match a parsed TCK relationship literal (`expected`) against a
/// Nexus result value (`actual`).
fn tck_rel_matches(expected: &Value, actual: &Value) -> bool {
    let Some(actual_obj) = actual.as_object() else {
        return false;
    };
    if !actual_obj.contains_key("_nexus_rel_type") {
        return false;
    }
    let expected_obj = expected.as_object().expect("tck rel is always an object");
    let expected_type = expected_obj.get("@type").and_then(Value::as_str);
    let actual_type = actual_obj.get("_nexus_rel_type").and_then(Value::as_str);
    if expected_type != actual_type {
        return false;
    }
    props_match(
        expected_obj,
        &["@tck_rel", "@type"],
        actual_obj,
        &["_nexus_id", "_nexus_rel_type", "type"],
    )
}

/// Match a parsed TCK path literal (`expected`) against a Nexus
/// result value (`actual`), comparing `nodes` and `relationships`
/// element-wise in traversal order.
fn tck_path_matches(expected: &Value, actual: &Value) -> bool {
    let Some(actual_obj) = actual.as_object() else {
        return false;
    };
    let expected_obj = expected.as_object().expect("tck path is always an object");
    let expected_nodes = expected_obj.get("nodes").and_then(Value::as_array);
    let expected_rels = expected_obj.get("relationships").and_then(Value::as_array);
    let actual_nodes = actual_obj.get("nodes").and_then(Value::as_array);
    let actual_rels = actual_obj.get("relationships").and_then(Value::as_array);
    let (Some(en), Some(an), Some(er), Some(ar)) =
        (expected_nodes, actual_nodes, expected_rels, actual_rels)
    else {
        return false;
    };
    en.len() == an.len()
        && er.len() == ar.len()
        && en.iter().zip(an.iter()).all(|(x, y)| values_equal(x, y))
        && er.iter().zip(ar.iter()).all(|(x, y)| values_equal(x, y))
}

/// Match a parsed IEEE-special-float literal (`NaN` / `Infinity` /
/// `-Infinity`, tagged `@tck_float`) against a Nexus result value.
///
/// openCypher renders non-finite floats unquoted in result tables, so they
/// cannot ride through as `serde_json::Number` (which rejects NaN/±∞). Nexus
/// today converts every non-finite arithmetic result into an error rather
/// than a value, so no current Nexus result can match — such a scenario stays
/// an attributable FAIL, never a harness panic on the unparseable cell. The
/// matcher is written for the general numeric case so that a future engine
/// which represents non-finite floats numerically is measured correctly.
fn tck_float_matches(expected: &Value, actual: &Value) -> bool {
    let kind = expected
        .get("@tck_float")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let Some(f) = actual.as_f64() else {
        return false;
    };
    match kind {
        "NaN" => f.is_nan(),
        "Infinity" => f.is_infinite() && f.is_sign_positive(),
        "-Infinity" => f.is_infinite() && f.is_sign_negative(),
        _ => false,
    }
}

/// Compare the "real" (non-marker, non-ignored) properties of two
/// JSON objects for an exact key-set + value match.
fn props_match(
    expected: &serde_json::Map<String, Value>,
    expected_ignore: &[&str],
    actual: &serde_json::Map<String, Value>,
    actual_ignore: &[&str],
) -> bool {
    let expected_keys: std::collections::BTreeSet<&str> = expected
        .keys()
        .map(String::as_str)
        .filter(|k| !expected_ignore.contains(k))
        .collect();
    let actual_keys: std::collections::BTreeSet<&str> = actual
        .keys()
        .map(String::as_str)
        .filter(|k| !actual_ignore.contains(k))
        .collect();
    expected_keys == actual_keys
        && expected_keys.iter().all(|k| {
            values_equal(
                expected.get(*k).expect("key came from expected.keys()"),
                actual.get(*k).expect("key set equality checked above"),
            )
        })
}

// ───────────────────── TCK cell parser ─────────────────────

/// Parse a TCK Gherkin cell into a `serde_json::Value`.
///
/// Covers every literal form the vendored openCypher corpus renders in a
/// result table:
///   - `null`, `true`, `false`
///   - integers, floats (incl. negative, scientific notation)
///   - the IEEE special floats `NaN`, `Infinity`, `-Infinity` (rendered
///     unquoted; carried as a `@tck_float` marker since `serde_json::Number`
///     cannot hold them — see `tck_float_matches`)
///   - single-quoted strings: `'foo'` → `"foo"`. Temporal and duration
///     values (`date`, `datetime`, `duration`, …) are rendered by the corpus
///     as quoted strings (e.g. `'2015-07-21'`, `'P14DT16H12M'`) and so parse
///     here as plain `Value::String` — no dedicated temporal literal exists
///     in the result tables.
///   - lists: `[1, 'a', {x: 1}]`
///   - maps with unquoted keys: `{x: 1.0, y: 2.0, crs: 'cartesian'}`
///   - node / relationship / path literals: `(:A {k: 1})`, `[:T {k: 1}]`,
///     `<(:A)-[:T]->(:B)>` (tagged `@tck_node` / `@tck_rel` / `@tck_path`)
pub fn tck_cell_to_json(cell: &str) -> Value {
    let trimmed = cell.trim();
    let mut parser = TckParser::new(trimmed);
    let v = parser.parse_value();
    parser.skip_ws();
    assert!(
        parser.eof(),
        "trailing input in TCK cell {trimmed:?} at byte {}: {:?}",
        parser.pos,
        &parser.src[parser.pos..]
    );
    v
}

/// Build the tagged marker for an IEEE special float (`NaN`, `Infinity`,
/// `-Infinity`) that `serde_json::Number` cannot represent. Compared via
/// `tck_float_matches`.
fn tck_float_marker(kind: &str) -> Value {
    let mut m = serde_json::Map::new();
    m.insert("@tck_float".to_string(), Value::String(kind.to_string()));
    Value::Object(m)
}

struct TckParser<'a> {
    src: &'a str,
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> TckParser<'a> {
    fn new(src: &'a str) -> Self {
        Self {
            src,
            bytes: src.as_bytes(),
            pos: 0,
        }
    }

    fn eof(&self) -> bool {
        self.pos >= self.bytes.len()
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn skip_ws(&mut self) {
        while let Some(b) = self.peek() {
            if b == b' ' || b == b'\t' || b == b'\n' || b == b'\r' {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn expect(&mut self, b: u8) {
        assert_eq!(
            self.peek(),
            Some(b),
            "expected `{}` at byte {} of {:?}",
            b as char,
            self.pos,
            self.src
        );
        self.pos += 1;
    }

    fn parse_value(&mut self) -> Value {
        self.skip_ws();
        match self.peek() {
            Some(b'\'') => self.parse_string(),
            Some(b'[') => self.parse_list_or_rel(),
            Some(b'{') => self.parse_map(),
            Some(b'(') => self.parse_node(),
            Some(b'<') => self.parse_path(),
            Some(b) if b == b'-' || b.is_ascii_digit() => self.parse_number(),
            Some(b) if b.is_ascii_alphabetic() => self.parse_keyword(),
            Some(b) => panic!(
                "unexpected byte `{}` at pos {} in {:?}",
                b as char, self.pos, self.src
            ),
            None => panic!("unexpected EOF in TCK cell {:?}", self.src),
        }
    }

    fn parse_string(&mut self) -> Value {
        self.expect(b'\'');
        let start = self.pos;
        while let Some(b) = self.peek() {
            if b == b'\\' {
                // Skip escaped char.
                self.pos += 2;
                continue;
            }
            if b == b'\'' {
                let s = self.src[start..self.pos].to_string();
                self.pos += 1;
                return Value::String(s);
            }
            self.pos += 1;
        }
        panic!("unterminated string in TCK cell {:?}", self.src);
    }

    fn parse_number(&mut self) -> Value {
        // IEEE special: `-Infinity` reaches this path via its leading `-`.
        // The positive `Infinity` and `NaN` are alphabetic and handled in
        // `parse_keyword`.
        if self.src[self.pos..].starts_with("-Infinity") {
            self.pos += "-Infinity".len();
            return tck_float_marker("-Infinity");
        }
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }
        while let Some(b) = self.peek() {
            if b.is_ascii_digit() || b == b'.' || b == b'e' || b == b'E' || b == b'+' || b == b'-' {
                self.pos += 1;
            } else {
                break;
            }
        }
        let text = &self.src[start..self.pos];
        if let Ok(n) = text.parse::<i64>() {
            Value::Number(n.into())
        } else {
            let f: f64 = text
                .parse()
                .unwrap_or_else(|_| panic!("bad number {text:?}"));
            Value::Number(serde_json::Number::from_f64(f).expect("finite f64"))
        }
    }

    fn parse_keyword(&mut self) -> Value {
        let start = self.pos;
        while let Some(b) = self.peek() {
            if b.is_ascii_alphanumeric() || b == b'_' {
                self.pos += 1;
            } else {
                break;
            }
        }
        let kw = &self.src[start..self.pos];
        match kw {
            "null" => Value::Null,
            "true" => Value::Bool(true),
            "false" => Value::Bool(false),
            // IEEE special floats openCypher renders unquoted; `serde_json`
            // cannot hold them as `Number`, so they carry a `@tck_float`
            // marker (the negative `-Infinity` is handled in `parse_number`).
            "NaN" | "Infinity" => tck_float_marker(kw),
            other => panic!("unknown keyword `{other}` in TCK cell {:?}", self.src),
        }
    }

    /// `[` starts either a list literal (`[1, 'a']`) or a relationship
    /// literal (`[:TYPE {...}]`). Disambiguate by looking past the `[`
    /// (and any whitespace) for a leading `:`.
    fn parse_list_or_rel(&mut self) -> Value {
        let mut lookahead = self.pos + 1;
        while let Some(b) = self.bytes.get(lookahead) {
            if *b == b' ' || *b == b'\t' || *b == b'\n' || *b == b'\r' {
                lookahead += 1;
            } else {
                break;
            }
        }
        if self.bytes.get(lookahead) == Some(&b':') {
            self.parse_rel()
        } else {
            self.parse_list()
        }
    }

    fn parse_list(&mut self) -> Value {
        self.expect(b'[');
        let mut items = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b']') {
            self.pos += 1;
            return Value::Array(items);
        }
        loop {
            items.push(self.parse_value());
            self.skip_ws();
            match self.peek() {
                Some(b',') => {
                    self.pos += 1;
                    self.skip_ws();
                }
                Some(b']') => {
                    self.pos += 1;
                    return Value::Array(items);
                }
                _ => panic!("expected `,` or `]` at pos {} in {:?}", self.pos, self.src),
            }
        }
    }

    fn parse_map(&mut self) -> Value {
        self.expect(b'{');
        let mut map = serde_json::Map::new();
        self.skip_ws();
        if self.peek() == Some(b'}') {
            self.pos += 1;
            return Value::Object(map);
        }
        loop {
            self.skip_ws();
            let key = self.parse_map_key();
            self.skip_ws();
            self.expect(b':');
            let value = self.parse_value();
            map.insert(key, value);
            self.skip_ws();
            match self.peek() {
                Some(b',') => {
                    self.pos += 1;
                }
                Some(b'}') => {
                    self.pos += 1;
                    return Value::Object(map);
                }
                _ => panic!("expected `,` or `}}` at pos {} in {:?}", self.pos, self.src),
            }
        }
    }

    fn parse_map_key(&mut self) -> String {
        match self.peek() {
            Some(b'\'') => {
                if let Value::String(s) = self.parse_string() {
                    s
                } else {
                    unreachable!()
                }
            }
            Some(b) if b.is_ascii_alphabetic() || b == b'_' => {
                let start = self.pos;
                while let Some(b) = self.peek() {
                    if b.is_ascii_alphanumeric() || b == b'_' {
                        self.pos += 1;
                    } else {
                        break;
                    }
                }
                self.src[start..self.pos].to_string()
            }
            Some(b) => panic!(
                "expected map key at pos {} in {:?} (got `{}`)",
                self.pos, self.src, b as char
            ),
            None => panic!("EOF in map key in {:?}", self.src),
        }
    }

    /// Parse a node literal: `()`, `(:A)`, `(:A:B)`, `({name: 'c'})`,
    /// `(:A {name: 'A', age: 3})`. Produces a tagged object using the
    /// `@tck_node` / `@labels` marker keys (see `values_equal`).
    fn parse_node(&mut self) -> Value {
        self.expect(b'(');
        let labels = self.parse_labels();
        self.skip_ws();
        let props = if self.peek() == Some(b'{') {
            self.parse_map()
        } else {
            Value::Object(serde_json::Map::new())
        };
        self.skip_ws();
        self.expect(b')');

        let mut map = serde_json::Map::new();
        map.insert("@tck_node".to_string(), Value::Bool(true));
        map.insert(
            "@labels".to_string(),
            Value::Array(labels.into_iter().map(Value::String).collect()),
        );
        if let Value::Object(props_map) = props {
            for (k, v) in props_map {
                map.insert(k, v);
            }
        }
        Value::Object(map)
    }

    /// Parse zero or more `:Label` segments (e.g. `:A:B`) following an
    /// opening `(`.
    fn parse_labels(&mut self) -> Vec<String> {
        let mut labels = Vec::new();
        loop {
            self.skip_ws();
            if self.peek() != Some(b':') {
                break;
            }
            self.pos += 1;
            let start = self.pos;
            while let Some(b) = self.peek() {
                if b.is_ascii_alphanumeric() || b == b'_' {
                    self.pos += 1;
                } else {
                    break;
                }
            }
            assert!(
                self.pos > start,
                "expected label name at pos {} in {:?}",
                start,
                self.src
            );
            labels.push(self.src[start..self.pos].to_string());
        }
        labels
    }

    /// Parse a relationship literal: `[:TYPE]` or `[:TYPE {props}]`.
    /// Produces a tagged object using the `@tck_rel` / `@type` marker
    /// keys (see `values_equal`).
    fn parse_rel(&mut self) -> Value {
        self.expect(b'[');
        self.skip_ws();
        self.expect(b':');
        let start = self.pos;
        while let Some(b) = self.peek() {
            if b.is_ascii_alphanumeric() || b == b'_' {
                self.pos += 1;
            } else {
                break;
            }
        }
        assert!(
            self.pos > start,
            "expected relationship type at pos {} in {:?}",
            start,
            self.src
        );
        let rel_type = self.src[start..self.pos].to_string();
        self.skip_ws();
        let props = if self.peek() == Some(b'{') {
            self.parse_map()
        } else {
            Value::Object(serde_json::Map::new())
        };
        self.skip_ws();
        self.expect(b']');

        let mut map = serde_json::Map::new();
        map.insert("@tck_rel".to_string(), Value::Bool(true));
        map.insert("@type".to_string(), Value::String(rel_type));
        if let Value::Object(props_map) = props {
            for (k, v) in props_map {
                map.insert(k, v);
            }
        }
        Value::Object(map)
    }

    /// Parse a path literal: `<()>`,
    /// `<(:A {name:'A'})-[:KNOWS {num:1}]->(:B {name:'B'})>`, with
    /// `<-[:T]-` incoming and `-[:T]-` undirected connectors also
    /// accepted. Produces a tagged object using the `@tck_path` marker
    /// key (see `values_equal`).
    fn parse_path(&mut self) -> Value {
        self.expect(b'<');
        self.skip_ws();

        let mut nodes = vec![self.parse_node()];
        let mut relationships = Vec::new();

        self.skip_ws();
        while self.peek() != Some(b'>') {
            // Connector is one of `-[...]->`, `<-[...]-`, or `-[...]-`.
            if self.peek() == Some(b'<') {
                self.pos += 1;
            }
            self.expect(b'-');
            self.skip_ws();
            relationships.push(self.parse_rel());
            self.skip_ws();
            self.expect(b'-');
            if self.peek() == Some(b'>') {
                self.pos += 1;
            }
            self.skip_ws();
            nodes.push(self.parse_node());
            self.skip_ws();
        }
        self.expect(b'>');

        let mut map = serde_json::Map::new();
        map.insert("@tck_path".to_string(), Value::Bool(true));
        map.insert("nodes".to_string(), Value::Array(nodes));
        map.insert("relationships".to_string(), Value::Array(relationships));
        Value::Object(map)
    }
}

// Unit tests for this module live in the harness=true target
// `tests/tck_cells.rs`: a `#[cfg(test)] mod` here would never run,
// because `tck_common` is only included by the `harness = false`
// runner binaries (which have no libtest to collect `#[test]` fns).
