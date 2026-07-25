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
use nexus_core::OpenCypherErrorKind;
use nexus_core::executor::ResultSet;
use nexus_core::testing::{TestContext, setup_isolated_test_engine};
use serde_json::Value;

mod tck_common;
use tck_common::compare_table;

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
    /// openCypher classification of `last_error`, captured from the typed
    /// engine error before it was stringified. `None` when the last query
    /// succeeded or none has run.
    last_error_kind: Option<OpenCypherErrorKind>,
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
            .field("last_error_kind", &self.last_error_kind)
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
                self.last_error_kind = None;
            }
            Err(e) => {
                self.last_result = None;
                self.last_error_kind = Some(e.opencypher_kind());
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
/// `Then a SyntaxError should be raised at compile time: UndefinedVariable`
///
/// Asserts two things, so a scenario cannot pass by coincidence:
///   1. the failure's openCypher *kind* (as classified by the engine from
///      the typed `Error`, via `Error::opencypher_kind`) equals the kind
///      named in the feature — `ConstraintError` is accepted as an alias
///      of `ConstraintVerificationFailed`;
///   2. the error message carries the expected detail token.
///
/// The phase (`compile time` vs `runtime`) is captured so both forms bind
/// to this step, but is not asserted: Nexus detects some statically-provable
/// errors only at execution time, and failing on that distinction would be a
/// false negative rather than a real conformance gap.
#[then(regex = r"^a (\w+) should be raised at (compile time|runtime): (.+)$")]
fn error_should_be_raised(
    world: &mut SpatialWorld,
    kind_word: String,
    _phase: String,
    token: String,
) {
    let msg = world.last_error.as_ref().unwrap_or_else(|| {
        panic!(
            "expected a {kind_word} containing `{token}` but the query succeeded; \
             last_result has {} rows",
            world
                .last_result
                .as_ref()
                .map(|r| r.rows.len())
                .unwrap_or(0)
        )
    });
    let expected = OpenCypherErrorKind::parse_tck_name(kind_word.trim())
        .unwrap_or_else(|| panic!("unknown openCypher error kind in feature: `{kind_word}`"));
    let actual = world
        .last_error_kind
        .expect("an error was captured but not classified");
    assert_eq!(
        actual, expected,
        "expected error kind {expected:?} (`{kind_word}`), but Nexus classified it as \
         {actual:?}: {msg}"
    );
    let token = token.trim();
    assert!(
        msg.contains(token),
        "expected error to contain `{token}`, got: {msg}"
    );
}

#[then(regex = r"^no side effects$")]
fn no_side_effects(world: &mut SpatialWorld) {
    // The engine now surfaces the openCypher-TCK mutation counters on
    // `ResultSet.side_effects` (nodes/relationships created+deleted,
    // properties set/removed, labels added/removed). "no side effects"
    // asserts the query mutated nothing. A query that errored committed
    // nothing and left no `last_result`, so there is nothing to check on
    // that path.
    if let Some(result) = world.last_result.as_ref() {
        assert!(
            result.side_effects.is_empty(),
            "expected no side effects, but the query reported {:?}",
            result.side_effects
        );
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
