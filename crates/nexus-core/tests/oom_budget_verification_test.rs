//! phase0_fix-cypher-oom-process-abort — stronger, DISCRIMINATING
//! verification for the `cartesian_product_max_bytes` budget gate in
//! `Executor::apply_cartesian_product`
//! (`crates/nexus-core/src/executor/eval/helpers.rs`).
//!
//! The existing end-to-end regression
//! (`crates/nexus-core/tests/cypher_oom_guard_test.rs`) only asserts
//! "Ok(_) OR Err(OutOfMemory)", which passes even if the guard were
//! deleted entirely, as long as the machine running the test has
//! enough RAM for that particular repro's product. It exists to catch
//! the "process aborts" failure mode, not to pin the budget's
//! rejection behaviour — that is this file's job.
//!
//! These tests configure an explicit, small
//! `cartesian_product_max_bytes` via `Executor::set_cartesian_product_max_bytes`
//! and drive a query whose cross-product estimate is deliberately
//! computed (not guessed) to land on either side of that budget:
//!
//! - a low budget MUST reject the query with `Error::OutOfMemory`
//!   (fails without the guard — an unbounded `apply_cartesian_product`
//!   would just return `Ok(_)`);
//! - the SAME query/data under a higher budget MUST succeed and
//!   return the exact, correctly-joined rows (proves the budget is
//!   the gate, not a broken query);
//! - the `OutOfMemory` message must name the actual product
//!   dimensions and the configured budget, not a generic string.
//!
//! Query shape: `UNWIND $rows AS r MATCH (a:P {id: r.s}), (b:P {id: r.d})
//! RETURN a.id, b.id`, with NO index on `:P(id)`. The correlated-seek
//! fix (`phase0_fix-correlated-predicate-index-seek`) only reroutes
//! INDEXED correlated predicates away from `apply_cartesian_product`
//! (see `correlated_index_seek_e2e_test.rs`); leaving `:P(id)`
//! unindexed here keeps this shape on the label-scan + cartesian-
//! product path the budget guards, regardless of that other fix.
//!
//! Sizing (verified against the ACTUAL plan via `Executor::parse_and_plan`,
//! not assumed — the planner emits `[Unwind(r), NodeByLabel(a),
//! NodeByLabel(b), Filter(a.id = r.s), Filter(b.id = r.d), Project]`:
//! BOTH `NodeByLabel` operators run before EITHER inline-property
//! `Filter`, so neither pattern is narrowed before the cross-product
//! check below runs): `NODES` `:P` nodes with `id` `0..NODES`, `ROWS`
//! driving `$rows` with `s = d = i` for `i` in `0..ROWS` (every row
//! matches exactly one node, no ambiguity in the post-filter result).
//!
//! 1. `UNWIND $rows AS r` — `ROWS` rows land in `context.result_set`,
//!    no bound variable yet.
//! 2. `MATCH (a:P {id: r.s})` — first `NodeByLabel`. `context.variables`
//!    is empty and `context.result_set.rows` is non-empty, so
//!    `seed_scan_main_loop`'s "existing rows, no variables yet" branch
//!    cross-joins `ROWS x NODES` OUTSIDE `apply_cartesian_product` and
//!    binds both `r` and `a` as `ROWS * NODES`-length variables. The
//!    `Filter { a.id = r.s }` operator has NOT run yet at this point —
//!    it is planned AFTER both `NodeByLabel`s, not interleaved with them.
//! 3. `MATCH (b:P {id: r.d})` — second `NodeByLabel`. `context.variables`
//!    now holds `r` and `a` (2), both length `ROWS * NODES`, so this
//!    call routes through `apply_cartesian_product` with
//!    `current_count = ROWS * NODES`, `new_count = NODES` (a second
//!    unindexed, unfiltered `:P` scan — both `Filter`s run AFTER this
//!    operator, too late to shrink the estimate the guard checks).
//!    `columns = context.variables.len() + 1 = 3` (`r`, `a`, plus the
//!    new `b` column). This is the exact call the budget guards.
//! 4. Both `Filter`s then run, narrowing the `ROWS * NODES * NODES`
//!    product down to exactly `ROWS` rows (one match per driving row)
//!    for the final `RETURN`.
//!
//! All estimates here are tiny in absolute terms (`ROWS * NODES * NODES`
//! cells, tens of kilobytes even at `size_of::<Value>()`) — safe to
//! actually allocate even if the guard were entirely absent.

use nexus_core::Error;
use nexus_core::executor::{Executor, Query, ResultSet};
use nexus_core::testing::create_isolated_test_executor;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::mem::size_of;

/// `:P` nodes seeded, `id` values `0..NODES`.
const NODES: i64 = 8;
/// Driving `$rows`; each row's `s` and `d` equal its own index
/// (`0..ROWS`), so every row matches exactly one `:P` node for both
/// `a` and `b` with no ambiguity.
const ROWS: i64 = 6;

/// `apply_cartesian_product`'s inputs for the SECOND `NodeByLabel`
/// (`b`) in [`QUERY`]'s plan. Neither `Filter` has run yet at this
/// point (see module doc, step 3), so `current_count` is the FIRST
/// pattern's unfiltered cross-join (`ROWS * NODES`), not `ROWS`. These
/// are computed from `NODES`/`ROWS`, not independently hardcoded, so
/// the arithmetic stays honest if the fixture sizes ever change.
const CURRENT_COUNT: i64 = ROWS * NODES;
const NEW_COUNT: i64 = NODES;
const PRODUCT: i64 = CURRENT_COUNT * NEW_COUNT;
/// `r` (UNWIND variable) + `a` (first pattern) bound at that point,
/// plus the new `b` column being added.
const COLUMNS: i64 = 2 + 1;

fn estimated_bytes() -> usize {
    (PRODUCT as usize) * (COLUMNS as usize) * size_of::<Value>()
}

const QUERY: &str = "UNWIND $rows AS r \
     MATCH (a:P {id: r.s}), (b:P {id: r.d}) \
     RETURN a.id, b.id";

fn build_rows_param() -> Value {
    Value::Array((0..ROWS).map(|i| json!({ "s": i, "d": i })).collect())
}

/// Seeds `NODES` `:P` nodes with `id` `0..NODES`. Deliberately no
/// `CREATE INDEX` — `:P(id)` stays unindexed so this shape cannot be
/// rerouted to the correlated `NodeIndexSeek` path and always reaches
/// `apply_cartesian_product` for the second pattern.
fn seed(executor: &mut Executor) {
    executor
        .execute(&Query {
            cypher: format!("UNWIND range(0, {}) AS i CREATE (:P {{id: i}})", NODES - 1),
            params: HashMap::new(),
        })
        .expect("seeding :P nodes must succeed");
}

fn run_query(executor: &mut Executor) -> Result<ResultSet, Error> {
    let mut params = HashMap::new();
    params.insert("rows".to_string(), build_rows_param());
    executor.execute(&Query {
        cypher: QUERY.to_string(),
        params,
    })
}

/// DISCRIMINATING — fails without the guard: with the budget set to
/// exactly one byte under the estimate, an unbounded
/// `apply_cartesian_product` would still just build the `PRODUCT`-row
/// cross product and return `Ok(_)` with rows, never
/// `Err(OutOfMemory)`.
#[test]
fn low_budget_rejects_a_query_whose_estimate_exceeds_it() {
    let (mut executor, _ctx) = create_isolated_test_executor();
    seed(&mut executor);

    let low_budget = estimated_bytes() - 1;
    executor.set_cartesian_product_max_bytes(low_budget);

    let result = run_query(&mut executor);

    match result {
        Err(Error::OutOfMemory(msg)) => {
            assert!(
                !msg.is_empty(),
                "OutOfMemory error must carry a diagnostic message"
            );
        }
        other => panic!(
            "expected Err(Error::OutOfMemory(_)) with a {low_budget}-byte budget \
             against an estimated {}-byte product, got {other:?}",
            estimated_bytes()
        ),
    }
}

/// DISCRIMINATING pair for the test above: same query, same data,
/// budget raised to just past the estimate. Proves the low-budget
/// rejection comes specifically from the budget, not from the
/// query/data being broken some other way — and that the join
/// semantics survive going through `apply_cartesian_product` at all,
/// by checking the EXACT resulting rows rather than just `Ok(_)`.
#[test]
fn raising_budget_lets_the_same_query_return_exact_rows() {
    let (mut executor, _ctx) = create_isolated_test_executor();
    seed(&mut executor);

    executor.set_cartesian_product_max_bytes(estimated_bytes() + 1);

    let result = run_query(&mut executor).expect(
        "the identical query/data that a low budget rejects must succeed \
         once the budget covers its estimate",
    );

    let rows: Vec<(i64, i64)> = result
        .rows
        .iter()
        .map(|row| {
            (
                row.values[0].as_i64().expect("a.id must be an integer"),
                row.values[1].as_i64().expect("b.id must be an integer"),
            )
        })
        .collect();

    let expected: Vec<(i64, i64)> = (0..ROWS).map(|i| (i, i)).collect();
    assert_eq!(
        rows, expected,
        "each driving row's s=d=i must join to exactly node id i for both \
         a and b, in driving-row order; got {rows:?}"
    );
}

/// DISCRIMINATING content check: the `OutOfMemory` diagnostic must
/// name the actual product dimensions (`CURRENT_COUNT x NEW_COUNT`),
/// the column count, and the configured budget — not a generic
/// "out of memory" string. A regression that keeps the `Err` variant
/// but swaps in a vague message (or wrong numbers) fails this even
/// though the test above (variant-only) would still pass.
#[test]
fn out_of_memory_message_names_dimensions_and_budget() {
    let (mut executor, _ctx) = create_isolated_test_executor();
    seed(&mut executor);

    let low_budget = 4096usize;
    // Sanity: this test's own fixture must actually exceed the budget
    // it configures, or the assertions below would be checking dead
    // code.
    assert!(
        estimated_bytes() > low_budget,
        "test fixture must produce an estimate over the configured budget; \
         estimate={}, budget={low_budget}",
        estimated_bytes()
    );
    executor.set_cartesian_product_max_bytes(low_budget);

    let result = run_query(&mut executor);

    match result {
        Err(Error::OutOfMemory(msg)) => {
            assert!(
                msg.contains(&format!("{PRODUCT} rows")),
                "message must name the product row count ({PRODUCT}): {msg}"
            );
            assert!(
                msg.contains(&format!("({CURRENT_COUNT} x {NEW_COUNT})")),
                "message must name the current x new dimensions: {msg}"
            );
            assert!(
                msg.contains(&format!("{COLUMNS} columns")),
                "message must name the column count: {msg}"
            );
            assert!(
                msg.contains(&format!("budget of {low_budget} bytes")),
                "message must name the configured budget: {msg}"
            );
        }
        other => panic!("expected Err(Error::OutOfMemory(_)), got {other:?}"),
    }
}
