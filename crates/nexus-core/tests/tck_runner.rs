//! openCypher-TCK-shaped Gherkin runner for the spatial Cypher
//! surface (phase6_opencypher-tck-spatial).
//!
//! Discovers `.feature` files under `tests/tck/spatial/` and drives
//! every scenario through `Engine::execute_cypher`. Each scenario
//! gets a fresh isolated `Engine` (its own tempdir) so scenarios
//! cannot leak state.
//!
//! See `tests/tck/spatial/VENDOR.md` for why the corpus is
//! Nexus-authored rather than vendored upstream.
//!
//! Step grammar (mirrors openCypher TCK):
//!   - `Given an empty graph`
//!   - `And having executed: """<cypher>"""`
//!   - `When executing query: """<cypher>"""`
//!   - `Then the result should be, in any order: <table>`
//!   - `Then the result should be: <table>` (ordered)
//!   - `Then the result should be empty`
//!   - `And no side effects`
//!
//! Run with:
//!   `cargo +nightly test -p nexus-core --test tck_runner --all-features`

use std::collections::HashMap;

use cucumber::{World, gherkin, given, then, when};
use nexus_core::Engine;
use nexus_core::executor::ResultSet;
use nexus_core::testing::{TestContext, setup_isolated_test_engine};
use serde_json::Value;

// ─────────────────────────── World ───────────────────────────

#[derive(Default, World)]
#[world(init = Self::default)]
pub struct SpatialWorld {
    engine: Option<Engine>,
    /// Held to keep the tempdir alive across step calls; dropped
    /// on World drop, which removes the test's data directory.
    _ctx: Option<TestContext>,
    last_result: Option<ResultSet>,
    last_error: Option<String>,
}

impl std::fmt::Debug for SpatialWorld {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SpatialWorld")
            .field("engine", &self.engine.is_some())
            .field("ctx", &self._ctx.is_some())
            .field(
                "last_result_rows",
                &self.last_result.as_ref().map(|r| r.rows.len()),
            )
            .field("last_error", &self.last_error)
            .finish()
    }
}

impl SpatialWorld {
    fn engine(&mut self) -> &mut Engine {
        self.engine.as_mut().expect(
            "engine not initialised — every Scenario must start with `Given an empty graph`",
        )
    }

    fn run_cypher(&mut self, cypher: &str) -> ResultSet {
        let cypher = cypher.trim();
        self.engine()
            .execute_cypher(cypher)
            .unwrap_or_else(|e| panic!("cypher `{cypher}` failed: {e}"))
    }

    /// Like `run_cypher` but captures errors instead of panicking.
    /// Used by `executing query:` so subsequent error-assertion
    /// steps (`Then a TypeError should be raised…`) can inspect the
    /// failure message.
    fn try_run_cypher(&mut self, cypher: &str) {
        let cypher = cypher.trim();
        match self.engine().execute_cypher(cypher) {
            Ok(rs) => {
                self.last_result = Some(rs);
                self.last_error = None;
            }
            Err(e) => {
                self.last_result = None;
                self.last_error = Some(e.to_string());
            }
        }
    }
}

// ─────────────────────────── Steps ───────────────────────────

#[given(regex = r"^an empty graph$")]
fn empty_graph(world: &mut SpatialWorld) {
    let (engine, ctx) = setup_isolated_test_engine().expect("setup_isolated_test_engine");
    world.engine = Some(engine);
    world._ctx = Some(ctx);
    world.last_result = None;
}

#[given(regex = r"^having executed:$")]
fn having_executed(world: &mut SpatialWorld, step: &gherkin::Step) {
    let docstring = step
        .docstring
        .as_ref()
        .expect("`having executed:` requires a docstring (\"\"\"…\"\"\")");
    let _ = world.run_cypher(docstring);
}

#[when(regex = r"^executing query:$")]
fn executing_query(world: &mut SpatialWorld, step: &gherkin::Step) {
    let docstring = step
        .docstring
        .as_ref()
        .expect("`executing query:` requires a docstring (\"\"\"…\"\"\")");
    world.try_run_cypher(docstring);
}

#[then(regex = r"^the result should be, in any order:$")]
fn result_in_any_order(world: &mut SpatialWorld, step: &gherkin::Step) {
    let table = step
        .table
        .as_ref()
        .expect("`the result should be, in any order:` requires a table");
    let result = match (&world.last_result, &world.last_error) {
        (Some(r), _) => r,
        (None, Some(e)) => panic!("query raised an error instead of returning a result: {e}"),
        (None, None) => panic!("no result captured — was `When executing query:` run?"),
    };
    compare_table(result, table, false);
}

#[then(regex = r"^the result should be:$")]
fn result_ordered(world: &mut SpatialWorld, step: &gherkin::Step) {
    let table = step
        .table
        .as_ref()
        .expect("`the result should be:` requires a table");
    let result = match (&world.last_result, &world.last_error) {
        (Some(r), _) => r,
        (None, Some(e)) => panic!("query raised an error instead of returning a result: {e}"),
        (None, None) => panic!("no result captured"),
    };
    compare_table(result, table, true);
}

#[then(regex = r"^the result should be empty$")]
fn result_empty(world: &mut SpatialWorld) {
    let result = match (&world.last_result, &world.last_error) {
        (Some(r), _) => r,
        (None, Some(e)) => panic!("query raised an error instead of returning a result: {e}"),
        (None, None) => panic!("no result captured"),
    };
    assert!(
        result.rows.is_empty(),
        "expected empty result, got {} rows",
        result.rows.len()
    );
}

/// `Then a TypeError should be raised at runtime: ERR_CRS_MISMATCH`
///
/// Asserts the captured error message contains the named token. The
/// "TypeError" / "SyntaxError" / "ConstraintError" prefix is matched
/// loosely — Nexus's error taxonomy doesn't yet split errors into
/// strict openCypher categories, so the harness only verifies the
/// failure happened and the error message contains the expected
/// token (typically a `ERR_*` code).
#[then(regex = r"^a (\w+) should be raised at runtime: (.+)$")]
fn error_at_runtime(world: &mut SpatialWorld, _kind: String, token: String) {
    let err = world.last_error.as_ref().unwrap_or_else(|| {
        panic!(
            "expected an error containing `{token}` but the query succeeded; \
             last_result has {} rows",
            world
                .last_result
                .as_ref()
                .map(|r| r.rows.len())
                .unwrap_or(0)
        )
    });
    let token = token.trim();
    assert!(
        err.contains(token),
        "expected error to contain `{token}`, got: {err}"
    );
}

#[then(regex = r"^no side effects$")]
fn no_side_effects(_world: &mut SpatialWorld) {
    // Nexus does not currently surface a side-effect counter in
    // ResultSet; the corpus uses this step for documentation
    // parity with the upstream TCK shape. When the engine starts
    // surfacing nodes_created / properties_set / etc. on the
    // ResultSet, replace this no-op with the real assertion.
}

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
fn compare_table(result: &ResultSet, table: &gherkin::Table, ordered: bool) {
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

fn rows_equal(got: &[Value], want: &[Value]) -> bool {
    got.len() == want.len() && got.iter().zip(want.iter()).all(|(g, w)| values_equal(g, w))
}

fn assert_rows_equal(got: &[Value], want: &[Value], idx: usize) {
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
fn values_equal(a: &Value, b: &Value) -> bool {
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
fn tck_cell_to_json(cell: &str) -> Value {
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

// ─────────────────────────── Entry ───────────────────────────

#[allow(dead_code)]
fn _params_helper() -> HashMap<String, Value> {
    // Reserved: scenarios that need parameters can be added later
    // via `Given parameters: { ... }` step. Kept here so the
    // import block does not warn unused.
    HashMap::new()
}

fn main() {
    // Windows default main-thread stack is 1 MiB which the cucumber
    // runtime + tokio current-thread executor + Engine setup have
    // overflowed in CI. Spawn a worker thread with an 8 MiB stack
    // and run the async runtime there.
    //
    // Cucumber's `run_and_exit` parses `std::env::args()` through
    // its own clap-derived CLI which rejects libtest flags like
    // `--test-threads=10` that `cargo test -- ...` injects into
    // every test binary (including this `harness = false` one).
    // Pass an explicit empty `cli::Opts` via `with_cli` so cucumber
    // skips argv parsing entirely; the shape we actually want is
    // "no filter, default writer, default runner, default parser",
    // which is exactly what `Opts::default()` produces.
    let handle = std::thread::Builder::new()
        .name("tck-runner".into())
        .stack_size(8 * 1024 * 1024)
        .spawn(|| {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("build tokio runtime");
            rt.block_on(async {
                let cli: cucumber::cli::Opts<
                    cucumber::parser::basic::Cli,
                    cucumber::runner::basic::Cli,
                    cucumber::writer::basic::Cli,
                > = cucumber::cli::Opts::default();
                SpatialWorld::cucumber()
                    .with_cli(cli)
                    .fail_on_skipped()
                    .run_and_exit("tests/tck/spatial")
                    .await;
            });
        })
        .expect("spawn tck-runner thread");
    handle.join().expect("tck-runner thread");
}
