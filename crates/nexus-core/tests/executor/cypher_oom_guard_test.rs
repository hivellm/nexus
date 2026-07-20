//! phase0_fix-cypher-oom-process-abort §4.2 — end-to-end regression
//! coverage for the §1.1 minimal repro shape.
//!
//! `UNWIND $rows AS r MATCH (a:P {id: r.s}), (b:P {id: r.d}) RETURN
//! a.id, b.id` used to make the server attempt a multi-terabyte
//! allocation (`Vec::with_capacity` on an unchecked
//! `current_count * new_count` product in
//! `Executor::apply_cartesian_product`,
//! `crates/nexus-core/src/executor/eval/helpers.rs`) and abort the
//! whole process — no error response, no rollback, every other
//! connected client dropped. `apply_cartesian_product` now estimates
//! the product's byte size and returns `Error::OutOfMemory` before
//! allocating when it exceeds `ExecutorConfig::cartesian_product_max_bytes`
//! (default 1 GiB). The unit-level coverage for that ceiling itself
//! (fires deterministically, configurable, does not reject legitimate
//! small products) lives in `crates/nexus-core/src/executor/eval/helpers.rs`'s
//! own `#[cfg(test)]` module.
//!
//! This test drives the exact query shape through the public
//! `Executor::execute` entry point and asserts the process survives:
//! the call must return either `Ok(_)` or `Err(Error::OutOfMemory(_))`
//! — never abort — which is the entire point of the fix.

use nexus_core::Error;
use nexus_core::executor::Query;
use nexus_core::testing::create_isolated_test_executor;
use serde_json::{Value, json};
use std::collections::HashMap;

/// Number of `:P` nodes seeded before the repro query runs, and number
/// of `$rows` pairs driving the `UNWIND`.
///
/// Sizing (see `Executor::apply_cartesian_product` and
/// `Executor::seed_scan_main_loop` in `crates/nexus-core/src/executor/`):
///
/// 1. `UNWIND $rows AS r` produces `ROWS` result rows via
///    `context.result_set`, with no bound "variable" yet — this step
///    is not itself budget-checked.
/// 2. `MATCH (a:P {id: r.s})` is a *correlated* property predicate
///    (`r.s` is not a literal). No index exists on `:P(id)` in this
///    test (none is created), so the planner cannot do anything but a
///    full label scan: it returns every `:P` node (`NODES` of them) as
///    candidates regardless of `r.s`. This first cross-product
///    (`ROWS x NODES`) runs in `seed_scan_main_loop`'s "existing rows,
///    no variables yet" branch, OUTSIDE `apply_cartesian_product`, and
///    is intentionally kept small here: `ROWS * NODES` = 100 * 1 000 =
///    100 000 rows (~3 MB at `size_of::<serde_json::Value>()` = 32
///    bytes/cell) — far under any budget, and irrelevant to what this
///    test is pinning.
/// 3. `MATCH (b:P {id: r.d})` is the SECOND comma-separated pattern.
///    Context now holds >= 1 bound variable (`a`, from step 2), so
///    this call routes through `Executor::apply_cartesian_product` —
///    the function this task fixed. Its `current_count` is the
///    100 000-row product from step 2 and its `new_count` is `NODES`
///    again (another full unfiltered `:P` scan, same reasoning as
///    step 2). The estimated byte size is
///    `current_count * new_count * columns * size_of::<Value>()` =
///    `100 000 * 1 000 * columns * 32`, which clears the default
///    1 GiB (1 073 741 824 byte) budget even at the smallest
///    plausible `columns` value of 1:
///    `100_000 * 1_000 * 32` = 3 200 000 000 bytes (~3 GiB), ~3x over
///    budget. With the real `columns` count (>= 2: `a` plus the new
///    `b` column) the margin is wider still.
const ROWS: usize = 100;
const NODES: usize = 1_000;

fn run(
    executor: &mut nexus_core::executor::Executor,
    cypher: &str,
    params: HashMap<String, Value>,
) -> Result<nexus_core::executor::ResultSet, Error> {
    executor.execute(&Query {
        cypher: cypher.to_string(),
        params,
    })
}

fn build_rows_param() -> Value {
    Value::Array(
        (0..ROWS)
            .map(|i| json!({ "s": i as i64, "d": (i + 1) as i64 }))
            .collect(),
    )
}

#[test]
fn unwind_comma_match_repro_survives_instead_of_aborting_process() {
    let (mut executor, _ctx) = create_isolated_test_executor();

    // Seed NODES :P nodes. Their `id` values are irrelevant to the
    // SIZE of the cartesian product — `apply_cartesian_product` sizes
    // its budget check from row/candidate COUNTS, before any per-row
    // `{id: r.s}` filtering happens. (That filtering not seeking the
    // index is a distinct, separately tracked defect —
    // phase0_fix-correlated-predicate-index-seek — and does not need
    // to be fixed for this test to be a faithful regression check:
    // this test does not create an index on `:P(id)` at all, so the
    // scan is unavoidably a full label scan regardless of whether that
    // other task has landed.)
    run(
        &mut executor,
        &format!("UNWIND range(1, {NODES}) AS i CREATE (:P {{id: i}})"),
        HashMap::new(),
    )
    .expect("seeding :P nodes must succeed");

    let mut params = HashMap::new();
    params.insert("rows".to_string(), build_rows_param());

    // §1.1 minimal repro shape: UNWIND + comma-separated multi-pattern
    // MATCH. This is the exact query shape that previously made the
    // server attempt a multi-terabyte allocation and abort the whole
    // process (the proposal's confirmed run: a 4.0e12-byte allocation
    // on 5 000 rows / 5 000 nodes, `4_000_000_000_000` bytes reported
    // by the allocator right before the process died).
    let result = run(
        &mut executor,
        "UNWIND $rows AS r \
         MATCH (a:P {id: r.s}), (b:P {id: r.d}) \
         RETURN a.id, b.id",
        params,
    );

    // The entire point of this test is that the process is still
    // alive to make this assertion. Only two outcomes are acceptable:
    //   - Ok(_): the query ran to completion within budget.
    //   - Err(Error::OutOfMemory(_)): the budget check caught the
    //     oversized product and rejected the query as data, not as a
    //     process-ending abort.
    // Any OTHER error variant is a genuine regression (e.g. a parse or
    // planner error unrelated to the OOM guard) and must fail the test
    // loudly rather than being silently accepted — blanket-accepting
    // any `Err(_)` here would hide such regressions behind a
    // false-green test (see the
    // `soft-tests-that-accept-errors-as-not-yet-implemented-hide-total-feature-breakage`
    // anti-pattern).
    match result {
        Ok(_) => {}
        Err(Error::OutOfMemory(msg)) => {
            assert!(
                !msg.is_empty(),
                "OutOfMemory error must carry a diagnostic message"
            );
        }
        Err(other) => {
            panic!("expected Ok(_) or Err(Error::OutOfMemory(_)), got a different error: {other}")
        }
    }
}
