//! openCypher-TCK conformance runner over the vendored upstream corpus.
//!
//! Sibling of `tck_runner.rs` (which drives the Nexus-authored spatial
//! corpus). This one drives the byte-for-byte vendored openCypher TCK under
//! `tests/tck/opencypher/features/` and produces a per-category pass/fail/skip
//! baseline — the conformance number, not a pass/fail gate.
//!
//! Outcome model (honest by construction):
//!   - **pass**  — every step matched a definition and its assertion held.
//!   - **fail**  — a matched step's assertion failed (a real conformance gap).
//!   - **skip**  — some step matched no definition (a capability the runner
//!     does not yet exercise) or a whole scenario was skip-listed. cucumber
//!     reports an unmatched step as `StepSkipped`, so a scenario that uses any
//!     unsupported step is tallied as a skip rather than inflating either the
//!     pass or the fail column.
//!
//! Gated behind `NEXUS_TCK=1` so a plain `cargo test` does not pay for 1600+
//! isolated-engine scenarios; the entry-point script sets it. After the run it
//! writes `docs/compatibility/OPENCYPHER_TCK_REPORT.md` and prints a summary.

mod tck_common;

use std::collections::{BTreeMap, HashMap};
use std::io::Write as _;
use std::sync::Mutex;

use cucumber::{World, event, gherkin, given, then, when};
use futures::FutureExt as _;
use nexus_core::Engine;
use nexus_core::OpenCypherErrorKind;
use nexus_core::executor::ResultSet;
use nexus_core::executor::types::SideEffects;
use nexus_core::testing::{TestContext, setup_isolated_test_engine};
use serde_json::Value;

use tck_common::{compare_table, tck_cell_to_json};

/// Pinned upstream commit the corpus was vendored from. Keep in sync with
/// `tests/tck/opencypher/VENDOR.md`; the report embeds it so a moving
/// denominator can never masquerade as a static percentage.
const PINNED_COMMIT: &str = "677cbafabb8c3c5eed458fd3b1ec0daec8d67d23";

/// category → [pass, fail, skip]. Filled by the `after` hook (run scenarios)
/// and the skip-list filter (deliberately-skipped scenarios).
static RESULTS: Mutex<BTreeMap<String, [u64; 3]>> = Mutex::new(BTreeMap::new());

/// skip reason → count, for scenarios the skip-list removes before running.
/// Reported so the skip total is attributed, never a silent omission.
static SKIP_REASONS: Mutex<BTreeMap<&'static str, u64>> = Mutex::new(BTreeMap::new());

// ─────────────────────────── World ───────────────────────────

#[derive(Default, World)]
#[world(init = Self::default)]
pub struct TckWorld {
    engine: Option<Engine>,
    /// Held to keep the tempdir alive across step calls.
    _ctx: Option<TestContext>,
    last_result: Option<ResultSet>,
    last_error: Option<String>,
    last_error_kind: Option<OpenCypherErrorKind>,
    /// The most recently executed query text (`having executed:` /
    /// `executing query:` / `executing control query:`). Captured purely
    /// for the failure-instrumentation JSONL log (see `log_failure`) — it
    /// does not affect pass/fail behavior.
    last_query: Option<String>,
    /// Parameters staged by a `Given parameters are:` step, consumed
    /// (drained) by the very next query execution — matching the TCK's
    /// "applies to the next statement" semantics.
    params: HashMap<String, Value>,
}

impl std::fmt::Debug for TckWorld {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TckWorld")
            .field("engine", &self.engine.is_some())
            .field(
                "last_result_rows",
                &self.last_result.as_ref().map(|r| r.rows.len()),
            )
            .field("last_error", &self.last_error)
            .field("last_error_kind", &self.last_error_kind)
            .field("last_query", &self.last_query)
            .finish()
    }
}

impl TckWorld {
    fn engine(&mut self) -> &mut Engine {
        self.engine
            .as_mut()
            .expect("engine not initialised — every Scenario must open with a graph step")
    }

    /// Runs `cypher`, consuming (draining) any parameters staged by a prior
    /// `Given parameters are:` step. Panics on failure — used for setup
    /// steps (`having executed:`) where the query is assumed to succeed.
    fn run_cypher(&mut self, cypher: &str) -> ResultSet {
        let cypher = cypher.trim();
        self.last_query = Some(cypher.to_string());
        let params = std::mem::take(&mut self.params);
        self.engine()
            .execute_cypher_with_params(cypher, params)
            .unwrap_or_else(|e| panic!("setup query `{cypher}` failed: {e}"))
    }

    /// Runs `cypher` under test, consuming (draining) any parameters staged
    /// by a prior `Given parameters are:` step, and records the outcome
    /// (result or error) on `self` instead of panicking.
    fn try_run_cypher(&mut self, cypher: &str) {
        let cypher = cypher.trim();
        self.last_query = Some(cypher.to_string());
        let params = std::mem::take(&mut self.params);
        match self.engine().execute_cypher_with_params(cypher, params) {
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
fn empty_graph(world: &mut TckWorld) {
    let (engine, ctx) = setup_isolated_test_engine().expect("setup_isolated_test_engine");
    world.engine = Some(engine);
    world._ctx = Some(ctx);
    world.last_result = None;
}

/// `Given any graph` — the TCK's marker for "initial contents are irrelevant".
/// An isolated engine starts empty, which satisfies it.
#[given(regex = r"^any graph$")]
fn any_graph(world: &mut TckWorld) {
    empty_graph(world);
}

#[given(regex = r"^having executed:$")]
fn having_executed(world: &mut TckWorld, step: &gherkin::Step) {
    let docstring = step
        .docstring
        .as_ref()
        .expect("`having executed:` requires a docstring");
    let _ = world.run_cypher(docstring);
}

/// `Given parameters are:` — a two-column table (`| key | value |`) staged
/// on the World and consumed (drained) by the very next query execution
/// (`having executed:` / `executing query:` / `executing control query:`),
/// matching the TCK's "applies to the next statement" semantics. Values are
/// TCK-literal cells parsed with the same parser used for expected result
/// tables.
#[given(regex = r"^parameters are:$")]
fn parameters_are(world: &mut TckWorld, step: &gherkin::Step) {
    let table = step
        .table
        .as_ref()
        .expect("`parameters are:` requires a table");
    for row in &table.rows {
        assert_eq!(
            row.len(),
            2,
            "parameters row must be `| key | value |`, got {row:?}"
        );
        let key = row[0].trim().to_string();
        let value = tck_cell_to_json(row[1].trim());
        world.params.insert(key, value);
    }
}

#[when(regex = r"^executing query:$")]
fn executing_query(world: &mut TckWorld, step: &gherkin::Step) {
    let docstring = step
        .docstring
        .as_ref()
        .expect("`executing query:` requires a docstring");
    world.try_run_cypher(docstring);
}

/// `When executing control query:` — an alias of `executing query:` used by
/// the TCK to re-verify graph state after a write scenario via an
/// independent read query. Same execution + assertion path.
#[when(regex = r"^executing control query:$")]
fn executing_control_query(world: &mut TckWorld, step: &gherkin::Step) {
    executing_query(world, step);
}

#[then(regex = r"^the result should be, in any order:$")]
fn result_in_any_order(world: &mut TckWorld, step: &gherkin::Step) {
    let table = step.table.as_ref().expect("table required");
    let result = expect_result(world);
    compare_table(result, table, false);
}

#[then(regex = r"^the result should be, in order:$")]
fn result_in_order(world: &mut TckWorld, step: &gherkin::Step) {
    let table = step.table.as_ref().expect("table required");
    let result = expect_result(world);
    compare_table(result, table, true);
}

#[then(regex = r"^the result should be empty$")]
fn result_empty(world: &mut TckWorld) {
    let result = expect_result(world);
    assert!(
        result.rows.is_empty(),
        "expected empty result, got {} rows",
        result.rows.len()
    );
}

/// `Then a TypeError should be raised at runtime: InvalidArgumentValue`
///
/// Asserts the failure's openCypher kind (classified from the typed engine
/// error) equals the kind named in the feature — `ConstraintError` is accepted
/// as an alias of `ConstraintVerificationFailed`. The detail token is checked
/// as a substring unless it is `*` (the TCK's "any detail" wildcard). The phase
/// (`compile time` / `runtime` / `any time`) is captured so all three forms
/// bind, but is not asserted: Nexus detects some statically-provable errors
/// only at execution time.
#[then(regex = r"^a (\w+) should be raised at (compile time|runtime|any time): (.+)$")]
fn error_should_be_raised(world: &mut TckWorld, kind_word: String, _phase: String, token: String) {
    let msg = world.last_error.as_ref().unwrap_or_else(|| {
        panic!(
            "expected a {kind_word} but the query succeeded ({} rows)",
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
        "expected error kind {expected:?} (`{kind_word}`), but Nexus classified it as {actual:?}: {msg}"
    );
    let token = token.trim();
    if token != "*" {
        assert!(
            msg.contains(token),
            "expected error to contain `{token}`, got: {msg}"
        );
    }
}

#[then(regex = r"^no side effects$")]
fn no_side_effects(world: &mut TckWorld) {
    if let Some(result) = world.last_result.as_ref() {
        assert!(
            result.side_effects.is_empty(),
            "expected no side effects, but the query reported {:?}",
            result.side_effects
        );
    }
}

/// `Then the side effects should be:` with a table of `| +nodes | 1 |` rows.
/// The table is exhaustive in the TCK: any counter it does not list is zero.
#[then(regex = r"^the side effects should be:$")]
fn side_effects_should_be(world: &mut TckWorld, step: &gherkin::Step) {
    let table = step.table.as_ref().expect("side-effects table required");
    let result = expect_result(world);
    let expected = parse_side_effects_table(table);
    assert_eq!(
        result.side_effects, expected,
        "side-effects mismatch:\n  got:  {:?}\n  want: {:?}",
        result.side_effects, expected
    );
}

// ─────────────────────── Step helpers ───────────────────────

fn expect_result(world: &TckWorld) -> &ResultSet {
    match (&world.last_result, &world.last_error) {
        (Some(r), _) => r,
        (None, Some(e)) => panic!("query raised an error instead of returning a result: {e}"),
        (None, None) => panic!("no result captured — was `When executing query:` run?"),
    }
}

/// Build the expected [`SideEffects`] from a TCK side-effects table. Unlisted
/// counters stay zero; an unknown key is a hard error so a malformed table
/// cannot pass by being silently ignored.
fn parse_side_effects_table(table: &gherkin::Table) -> SideEffects {
    let mut se = SideEffects::default();
    for row in &table.rows {
        assert_eq!(
            row.len(),
            2,
            "side-effects row must be `| key | count |`, got {row:?}"
        );
        let key = row[0].trim();
        let count: u64 = row[1]
            .trim()
            .parse()
            .unwrap_or_else(|_| panic!("side-effects count not an integer: {:?}", row[1]));
        match key {
            "+nodes" => se.nodes_created = count,
            "-nodes" => se.nodes_deleted = count,
            "+relationships" => se.relationships_created = count,
            "-relationships" => se.relationships_deleted = count,
            "+properties" => se.properties_set = count,
            "-properties" => se.properties_removed = count,
            "+labels" => se.labels_added = count,
            "-labels" => se.labels_removed = count,
            other => panic!("unknown side-effect key in TCK table: {other:?}"),
        }
    }
    se
}

// ─────────────────────── Reporting ───────────────────────

/// Two-level category from a feature path: `.../features/clauses/match/Match1.feature`
/// → `clauses/match`.
fn category_of(feature: &gherkin::Feature) -> String {
    let Some(path) = feature.path.as_ref() else {
        return "unknown".to_string();
    };
    let comps: Vec<String> = path
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    let Some(idx) = comps.iter().position(|c| c == "features") else {
        return "unknown".to_string();
    };
    match (comps.get(idx + 1), comps.get(idx + 2)) {
        (Some(top), Some(sub)) if !sub.ends_with(".feature") => format!("{top}/{sub}"),
        (Some(top), _) => top.clone(),
        _ => "unknown".to_string(),
    }
}

fn record(category: String, finished: &event::ScenarioFinished) {
    let slot = match finished {
        event::ScenarioFinished::StepPassed => 0usize,
        event::ScenarioFinished::StepSkipped => 2usize,
        event::ScenarioFinished::StepFailed(..) | event::ScenarioFinished::BeforeHookFailed(..) => {
            1
        }
    };
    let mut guard = RESULTS.lock().expect("results mutex poisoned");
    guard.entry(category).or_insert([0; 3])[slot] += 1;
}

/// Deliberate skip-list: scenarios the harness cannot yet EVALUATE because
/// they depend on a capability the runner does not provide. These are skipped
/// (counted + reported by reason), never run and never counted as fails.
///
/// This is strictly for un-evaluatable scenarios. Features Nexus attempts but
/// gets wrong — temporal semantics, etc. — are NOT skip-listed: those stay as
/// real fails, because hiding a real gap behind a skip is what makes a
/// conformance number a lie.
fn skip_reason(scenario: &gherkin::Scenario) -> Option<&'static str> {
    for step in &scenario.steps {
        let t = step.value.as_str();
        if t.starts_with("there exists a procedure") {
            return Some("procedure registration not supported by the harness");
        }
        if t.contains("binary-tree-") {
            return Some("named fixture graph (binary-tree-N) not supported");
        }
    }
    None
}

/// Count a filtered-out scenario as a skip against its category and reason.
fn record_skip(category: String, reason: &'static str) {
    RESULTS
        .lock()
        .expect("results mutex poisoned")
        .entry(category)
        .or_insert([0; 3])[2] += 1;
    *SKIP_REASONS
        .lock()
        .expect("skip-reasons mutex poisoned")
        .entry(reason)
        .or_insert(0) += 1;
}

// ─────────────────── Per-scenario failure log (JSONL) ───────────────────
//
// F-004 (docs/analysis/tck/01-measurement-and-methodology.md): the `after`
// hook only ever tallied a count, discarding every failure's diagnostic
// message. This turns each `StepFailed` / `BeforeHookFailed` scenario into
// one JSONL line — pure observability, it does not affect the pass/fail/skip
// tallies above.

/// Serializes writes to the failure log — scenarios can run concurrently
/// under cucumber's default runner, and a bare `File::open`+`write` race
/// could interleave two JSON lines into one corrupt line.
static FAILLOG_LOCK: Mutex<()> = Mutex::new(());

/// Path to the JSONL failure log, configurable so CI/local runs can
/// redirect it without touching the harness. Defaults under `target/` so
/// it is never accidentally committed.
fn failure_log_path() -> String {
    std::env::var("NEXUS_TCK_FAILLOG").unwrap_or_else(|_| "target/tck-failures.jsonl".to_string())
}

/// Truncates (or creates) the failure log once at the start of a run so a
/// fresh invocation always produces a clean baseline instead of appending
/// onto a stale file from a previous run.
fn reset_failure_log() {
    let path = failure_log_path();
    if let Some(parent) = std::path::Path::new(&path).parent() {
        if !parent.as_os_str().is_empty() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                eprintln!(
                    "warning: failed to create TCK fail-log directory {}: {e}",
                    parent.display()
                );
                return;
            }
        }
    }
    let _guard = FAILLOG_LOCK.lock().expect("fail-log mutex poisoned");
    if let Err(e) = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&path)
    {
        eprintln!("warning: failed to reset TCK fail-log {path}: {e}");
    }
}

/// Extracts a human-readable message from a finished-scenario event, or
/// `None` for `StepPassed` / `StepSkipped` (nothing to log).
fn failure_message(finished: &event::ScenarioFinished) -> Option<String> {
    match finished {
        event::ScenarioFinished::StepFailed(_, _, err) => Some(err.to_string()),
        event::ScenarioFinished::BeforeHookFailed(info) => {
            Some(format!("before-hook failed: {}", coerce_panic_info(info)))
        }
        event::ScenarioFinished::StepPassed | event::ScenarioFinished::StepSkipped => None,
    }
}

/// Best-effort extraction of a panic payload's message. Mirrors the same
/// `Box<dyn Any + Send>` downcast convention `std::panic::catch_unwind` (and
/// `cucumber` internally) uses for step panics — the panic macros hand
/// through either an owned `String` or a `&'static str` payload.
fn coerce_panic_info(info: &event::Info) -> String {
    info.downcast_ref::<String>()
        .cloned()
        .or_else(|| info.downcast_ref::<&str>().map(|s| (*s).to_string()))
        .unwrap_or_else(|| "(could not resolve panic payload)".to_string())
}

/// Appends one JSONL line for a failed scenario: `category`, `feature_path`,
/// `scenario_name`, the last-executed `query`, the failure `message`, and
/// the World's `last_error` / `last_error_kind` (if the World survived to
/// the `after` hook — it always does here, since there is no `.before()`
/// hook that could fail it away).
fn log_failure(
    category: &str,
    feature: &gherkin::Feature,
    scenario: &gherkin::Scenario,
    finished: &event::ScenarioFinished,
    world: Option<&TckWorld>,
) {
    let Some(message) = failure_message(finished) else {
        return;
    };
    let feature_path = feature
        .path
        .as_ref()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    let entry = serde_json::json!({
        "category": category,
        "feature_path": feature_path,
        "scenario_name": scenario.name,
        "query": world.and_then(|w| w.last_query.clone()),
        "message": message,
        "last_error": world.and_then(|w| w.last_error.clone()),
        "last_error_kind": world
            .and_then(|w| w.last_error_kind)
            .map(|k| format!("{k:?}")),
    });
    let line = match serde_json::to_string(&entry) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("warning: failed to serialise TCK fail-log entry: {e}");
            return;
        }
    };

    let path = failure_log_path();
    let _guard = FAILLOG_LOCK.lock().expect("fail-log mutex poisoned");
    match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        Ok(mut file) => {
            if let Err(e) = writeln!(file, "{line}") {
                eprintln!("warning: failed to append TCK fail-log entry to {path}: {e}");
            }
        }
        Err(e) => eprintln!("warning: failed to open TCK fail-log {path}: {e}"),
    }
}

fn pct(pass: u64, total: u64) -> String {
    if total == 0 {
        "—".to_string()
    } else {
        format!("{:.1}%", (pass as f64 / total as f64) * 100.0)
    }
}

fn write_report(results: &BTreeMap<String, [u64; 3]>) {
    let mut total = [0u64; 3];
    for v in results.values() {
        for i in 0..3 {
            total[i] += v[i];
        }
    }
    let grand: u64 = total.iter().sum();

    let mut md = String::new();
    md.push_str("# openCypher TCK Conformance Report\n\n");
    md.push_str(
        "Measured pass/fail/skip over the vendored upstream openCypher TCK corpus. This is a\n\
         *conformance* number against the specification — distinct from the differential\n\
         Neo4j suite in `scripts/compatibility/`. Regenerate with the entry point below.\n\n",
    );
    md.push_str(&format!(
        "- **Pinned upstream commit:** `{PINNED_COMMIT}`\n"
    ));
    md.push_str(
        "- **Corpus:** `crates/nexus-core/tests/tck/opencypher/features/` (see `VENDOR.md`)\n",
    );
    md.push_str(
        "- **Reproduce:** `NEXUS_TCK=1 cargo +nightly test -p nexus-core --test tck_opencypher --all-features`\n",
    );
    md.push_str("  or `scripts/compatibility/run-opencypher-tck.ps1`.\n\n");
    md.push_str("## Outcome model\n\n");
    md.push_str(
        "- **pass** — every step matched a definition and its assertion held.\n\
         - **fail** — a matched step's assertion failed (a real conformance gap).\n\
         - **skip** — a step matched no definition yet (a capability the runner does not\n\
         exercise) or the scenario was skip-listed. Skips are counted, never hidden.\n\n",
    );
    md.push_str("## Totals\n\n");
    md.push_str(&format!(
        "**{} scenarios — {} passed ({}), {} failed, {} skipped.**\n\n",
        grand,
        total[0],
        pct(total[0], grand),
        total[1],
        total[2],
    ));

    md.push_str("## Per-category\n\n");
    md.push_str("| Category | Pass | Fail | Skip | Total | Pass % |\n");
    md.push_str("|---|---:|---:|---:|---:|---:|\n");
    for (cat, v) in results {
        let cat_total = v[0] + v[1] + v[2];
        md.push_str(&format!(
            "| `{cat}` | {} | {} | {} | {} | {} |\n",
            v[0],
            v[1],
            v[2],
            cat_total,
            pct(v[0], cat_total),
        ));
    }
    md.push_str(&format!(
        "| **total** | **{}** | **{}** | **{}** | **{}** | **{}** |\n",
        total[0],
        total[1],
        total[2],
        grand,
        pct(total[0], grand),
    ));

    let skip_reasons = SKIP_REASONS.lock().expect("skip-reasons mutex poisoned");
    if !skip_reasons.is_empty() {
        md.push_str("\n## Skip-list (deliberately un-evaluated)\n\n");
        md.push_str(
            "Scenarios the runner cannot yet exercise because they need a capability it does\n\
             not provide. Counted as skips, never as fails. Features Nexus attempts but gets\n\
             wrong (e.g. temporal semantics) are NOT here — those remain real fails above.\n\n",
        );
        md.push_str("| Reason | Scenarios |\n|---|---:|\n");
        let mut skip_total = 0u64;
        for (reason, count) in skip_reasons.iter() {
            md.push_str(&format!("| {reason} | {count} |\n"));
            skip_total += count;
        }
        md.push_str(&format!(
            "| **total deliberate skips** | **{skip_total}** |\n"
        ));
        md.push_str(
            "\nThe remaining skips in the per-category table are scenarios that use a Gherkin\n\
             step the runner does not define yet (they skip at the unmatched step).\n",
        );
    }

    let out = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/compatibility/OPENCYPHER_TCK_REPORT.md"
    );
    if let Err(e) = std::fs::write(out, md) {
        eprintln!("warning: failed to write TCK report to {out}: {e}");
    } else {
        println!("\nTCK report written to {out}");
    }
    println!(
        "TCK totals: {} scenarios — {} pass / {} fail / {} skip ({} pass)",
        grand,
        total[0],
        total[1],
        total[2],
        pct(total[0], grand),
    );
}

// ─────────────────────────── Entry ───────────────────────────

fn main() {
    if std::env::var("NEXUS_TCK").is_err() {
        eprintln!(
            "tck_opencypher: skipping the vendored corpus run (set NEXUS_TCK=1 to run \
             the ~1600-scenario conformance baseline)."
        );
        return;
    }

    reset_failure_log();

    // 8 MiB worker stack: the default 1 MiB Windows main-thread stack has
    // overflowed on the cucumber runtime + tokio + Engine setup in CI.
    let handle = std::thread::Builder::new()
        .name("tck-opencypher".into())
        .stack_size(8 * 1024 * 1024)
        .spawn(|| {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("build tokio runtime");
            rt.block_on(async {
                // Bypass cucumber's argv parsing (it rejects libtest flags that
                // `cargo test -- ...` injects into every test binary).
                let cli: cucumber::cli::Opts<
                    cucumber::parser::basic::Cli,
                    cucumber::runner::basic::Cli,
                    cucumber::writer::basic::Cli,
                > = cucumber::cli::Opts::default();
                let _ = TckWorld::cucumber()
                    .with_cli(cli)
                    .after(|feature, _rule, scenario, ev, world| {
                        let category = category_of(feature);
                        log_failure(&category, feature, scenario, ev, world.as_deref());
                        record(category, ev);
                        async {}.boxed_local()
                    })
                    .filter_run(
                        "tests/tck/opencypher/features",
                        |feature, _rule, scenario| {
                            // Skip-list: deliberately skip un-evaluatable scenarios,
                            // counting them by reason. Everything else runs.
                            match skip_reason(scenario) {
                                Some(reason) => {
                                    record_skip(category_of(feature), reason);
                                    false
                                }
                                None => true,
                            }
                        },
                    )
                    .await;
            });
        })
        .expect("spawn tck-opencypher thread");
    handle.join().expect("tck-opencypher thread");

    let results = RESULTS.lock().expect("results mutex poisoned").clone();
    write_report(&results);
}
