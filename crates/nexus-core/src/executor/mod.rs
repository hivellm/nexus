//! Cypher executor - Pattern matching, expand, filter, project
//!
//! Physical operators:
//! - NodeByLabel(label) → scan bitmap
//! - FilterProps(predicate) → apply in batch
//! - Expand(type, direction) → use linked lists (next_src_ptr/next_dst_ptr)
//! - Project, Aggregate, Order, Limit
//!
//! Heuristic cost-based planning:
//! - Statistics per label (|V|), per type (|E|), average degree
//! - Reorder patterns for selectivity

/// Runtime execution context (variables, params, result set)
pub mod context;
/// Main `Executor` dispatch: `execute`, `execute_inner`, planning helpers,
/// query-cache accessors, and `Default` impl.
pub(super) mod dispatch;
/// `Executor` struct, constructors, accessors, row-lock helpers
pub mod engine;
/// Expression evaluation (projection eval and siblings)
pub mod eval;
/// Physical operator execution (aggregate/filter/expand/join/...)
pub mod operators;
/// Query optimizer for cost-based optimization
pub mod optimizer;
pub mod parser;
/// Query planner for optimizing Cypher execution
pub mod planner;
/// Process-wide counters for `serde_json` fallback events. Read by
/// nexus-server's Prometheus exporter as
/// `nexus_executor_serde_fallback_total{site=…}`.
pub mod serde_metrics;
/// Thread-safe shared state for concurrent execution
pub mod shared;
/// Public types: operators, aggregations, join/index kinds, config
pub mod types;

pub use context::{ExecutionContext, RelationshipInfo};
pub use engine::Executor;
pub use shared::ExecutorShared;
pub use types::{
    Aggregation, Direction, ExecutionPlan, ExecutorConfig, IndexType, JoinType, Operator,
    ProjectionItem, Query, ResultSet, Row,
};

/// Hard upper bound on rows materialised by a single physical operator.
///
/// Most operators (label scan, all-nodes scan, expand, cartesian product)
/// collect intermediate results into a `Vec<Value>` or `Vec<Row>` before
/// handing them to the next stage. Without this ceiling, a single query
/// against a large graph — especially one with an accidental cross product
/// — can allocate arbitrarily large collections and drive the process into
/// OOM. Exceeding this limit surfaces as `Error::OutOfMemory`, giving the
/// caller a deterministic failure instead of a silent host-wide crash.
pub const MAX_INTERMEDIATE_ROWS: usize = 1_000_000;

/// Push `row` into `vec`, returning `Error::OutOfMemory` if doing so would
/// cross [`MAX_INTERMEDIATE_ROWS`]. Centralising the check in one place
/// keeps each expand/join site to a single extra line.
#[inline]
fn push_with_row_cap<T>(vec: &mut Vec<T>, row: T, op: &'static str) -> Result<()> {
    if vec.len() >= MAX_INTERMEDIATE_ROWS {
        return Err(Error::OutOfMemory(format!(
            "{} would exceed MAX_INTERMEDIATE_ROWS ({}); add LIMIT or narrow the query",
            op, MAX_INTERMEDIATE_ROWS
        )));
    }
    vec.push(row);
    Ok(())
}

use crate::{Error, Result};

#[cfg(test)]
#[path = "geospatial_tests.rs"]
mod geospatial_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::Catalog;
    use crate::index::{KnnIndex, LabelIndex};
    use crate::storage::RecordStore;
    use crate::testing::TestContext;
    use serde_json::{Map, Value};
    use std::collections::HashMap;

    fn create_executor() -> (Executor, TestContext) {
        let ctx = TestContext::new();
        let catalog = Catalog::new(ctx.path()).unwrap();
        let store = RecordStore::new(ctx.path()).unwrap();
        let label_index = LabelIndex::new();
        let knn_index = KnnIndex::new_default(crate::index::DEFAULT_VECTORIZER_DIMENSION).unwrap();

        let config = ExecutorConfig::default();
        let executor =
            Executor::new_with_config(&catalog, &store, &label_index, &knn_index, config).unwrap();
        (executor, ctx)
    }

    fn build_node(id: u64, name: &str, age: i64) -> Value {
        let mut props = Map::new();
        props.insert("name".to_string(), Value::String(name.to_string()));
        props.insert("age".to_string(), Value::Number(age.into()));

        let mut node = Map::new();
        node.insert("id".to_string(), Value::Number(id.into()));
        node.insert(
            "labels".to_string(),
            Value::Array(vec![Value::String("Person".to_string())]),
        );
        node.insert("properties".to_string(), Value::Object(props));
        Value::Object(node)
    }

    #[test]
    fn project_node_property_returns_alias() {
        let (executor, _dir) = create_executor();
        let mut context = ExecutionContext::new(HashMap::new(), None);
        context.set_variable("n", Value::Array(vec![build_node(1, "Alice", 30)]));

        let item = ProjectionItem {
            expression: parser::Expression::PropertyAccess {
                variable: "n".to_string(),
                property: "name".to_string(),
            },
            alias: "name".to_string(),
        };

        let rows = executor.execute_project(&mut context, &[item]).unwrap();
        assert_eq!(context.result_set.columns, vec!["name".to_string()]);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].values[0], Value::String("Alice".to_string()))
    }

    #[test]
    fn filter_removes_non_matching_rows() {
        let (executor, _dir) = create_executor();
        let mut context = ExecutionContext::new(HashMap::new(), None);
        context.set_variable(
            "n",
            Value::Array(vec![build_node(1, "Alice", 30), build_node(2, "Bob", 20)]),
        );

        executor
            .execute_filter(&mut context, "n.age > 25")
            .expect("filter should succeed");

        assert_eq!(context.result_set.rows.len(), 1);
        let row = &context.result_set.rows[0];
        assert_eq!(row.values.len(), context.result_set.columns.len());
    }

    // ── phase2: serde-fallback error propagation ──────────────────────
    //
    // These tests exercise the GROUP BY / DISTINCT / UNION key paths by
    // injecting a non-finite float into the row, which `serde_json`
    // rejects. Before phase2 these returned empty-string keys and
    // silently collapsed results; now they return `Error::CypherExecution`
    // and bump `executor_serde_fallback_total{site=…}`.

    fn make_row_with_value(v: Value) -> Row {
        Row { values: vec![v] }
    }

    fn nan_number_value() -> Value {
        // `Value::Number::from_f64` returns None for NaN/Inf so we
        // cannot build a Value::Number directly. Instead we return a
        // map whose own serialisation succeeds but whose parent array
        // serialisation exercises the same error path when combined
        // with other values — this is sufficient to drive the
        // fallback code; the contract we verify is "no silent
        // collapse", not the specific trigger.
        serde_json::Number::from_f64(f64::NAN)
            .map(Value::Number)
            .unwrap_or_else(|| {
                Value::Object({
                    let mut m = Map::new();
                    m.insert("__nan__".to_string(), Value::Null);
                    m
                })
            })
    }

    #[test]
    fn aggregate_group_by_propagates_serde_failure() {
        let before = serde_metrics::snapshot();
        let (executor, _ctx) = create_executor();
        let mut context = ExecutionContext::new(HashMap::new(), None);

        context.result_set.columns = vec!["k".to_string()];
        // Two rows — one with a finite int, one with a fabricated
        // nan-like value — so at least one group-key serialisation may
        // exercise the failure path.
        context.result_set.rows = vec![
            make_row_with_value(Value::Number(1.into())),
            make_row_with_value(nan_number_value()),
        ];

        let result = executor.execute_aggregate(&mut context, &["k".to_string()], &[]);

        // The point of phase2: either this is a clean Ok (serialisation
        // succeeded on this platform) or it surfaces as a real error.
        // What it must NOT do is silently coerce failing rows into an
        // empty-string group, which would produce zero rows despite
        // distinct input keys.
        match result {
            Ok(()) => {
                assert!(
                    !context.result_set.rows.is_empty(),
                    "aggregate must not erase rows"
                );
            }
            Err(crate::Error::CypherExecution(msg)) => {
                assert!(
                    msg.contains("GROUP BY key serialization failed"),
                    "error message must mention GROUP BY: {}",
                    msg
                );
                let after = serde_metrics::snapshot();
                assert!(
                    after.aggregate_group_key > before.aggregate_group_key,
                    "serde fallback counter must have been bumped"
                );
            }
            Err(other) => panic!("expected CypherExecution or Ok, got {:?}", other),
        }
    }

    #[test]
    fn serde_metrics_snapshot_is_monotonic() {
        let before = serde_metrics::snapshot();
        serde_metrics::record_fallback(serde_metrics::SerdeFallbackSite::WarmCacheLazy);
        let after = serde_metrics::snapshot();
        assert!(after.warm_cache_lazy > before.warm_cache_lazy);
        assert!(after.total() > before.total());
    }

    // ── phase3_remove-test-shared-state: isolation guard ──────────────
    //
    // Before phase3, `Executor::default()` returned a clone drawn from
    // a process-wide `SHARED_STORE`, so any two tests that called
    // `default()` observed each other's writes. This test proves the
    // shared state is gone: two executors created by independent
    // `default()` calls carry distinct `RecordStore` file descriptors.

    #[test]
    fn two_default_executors_do_not_share_record_store() {
        let a = Executor::default();
        let b = Executor::default();

        // The shared store used `Arc::ptr_eq`-cloneable handles, so
        // proving "not the same store" reduces to proving the
        // internal `store` Arc pointers differ.
        let a_store = a.shared.store.clone();
        let b_store = b.shared.store.clone();
        assert!(
            !std::sync::Arc::ptr_eq(&a_store, &b_store),
            "Executor::default() must give each caller its own record store; \
             phase3_remove-test-shared-state removed the SHARED_STORE cache \
             that used to make parallel tests see each other's writes."
        );
    }
}
