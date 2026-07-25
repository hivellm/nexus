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
    got.len() == want.len() && got.iter().zip(want.iter()).all(|(g, w)| values_equal(g, w))
}

pub fn assert_rows_equal(got: &[Value], want: &[Value], idx: usize) {
    assert!(
        rows_equal(got, want),
        "row[{idx}] mismatch:\n  got:  {got:?}\n  want: {want:?}"
    );
}

/// Tolerant value comparison.
///
/// - Floats: 1e-9 absolute tolerance.
/// - Numbers: integer-vs-float coercion allowed (`1` == `1.0`).
/// - Maps: unordered, every key must match.
/// - Lists: ordered.
pub fn values_equal(a: &Value, b: &Value) -> bool {
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

// ───────────────────── TCK cell parser ─────────────────────

/// Parse a TCK Gherkin cell into a `serde_json::Value`.
///
/// Supports the subset the spatial corpus needs:
///   - `null`, `true`, `false`
///   - integers, floats (incl. negative, scientific notation)
///   - single-quoted strings: `'foo'` → `"foo"`
///   - lists: `[1, 'a', {x: 1}]`
///   - maps with unquoted keys: `{x: 1.0, y: 2.0, crs: 'cartesian'}`
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
            Some(b'[') => self.parse_list(),
            Some(b'{') => self.parse_map(),
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
            other => panic!("unknown keyword `{other}` in TCK cell {:?}", self.src),
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
}
