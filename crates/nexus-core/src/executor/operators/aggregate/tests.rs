//! §4.4 byte-for-byte parity — the columnar SUM / MIN / MAX / AVG
//! fast path and the scalar row path must produce identical
//! `Value`s on 10 000-row numeric fixtures. Flip
//! `columnar_threshold` between `usize::MAX` (forces row path) and
//! `4096` (default — fast path fires) and assert equality.
//!
//! The fixture uses integer ages and half-step scores specifically
//! so every sum / average is exactly representable as an `f64` —
//! keeping the comparison strict rather than tolerance-based.

use super::*;
use crate::executor::context::ExecutionContext;
use crate::testing::create_test_executor;

fn build_person(id: u64, age: i64, score: f64) -> Value {
    let mut node = serde_json::Map::new();
    node.insert("_nexus_id".to_string(), Value::Number(id.into()));
    node.insert("age".to_string(), Value::Number(age.into()));
    node.insert(
        "score".to_string(),
        Value::Number(serde_json::Number::from_f64(score).expect("fixture score is always finite")),
    );
    Value::Object(node)
}

fn aggregate_with_threshold(nodes: &[Value], agg: Aggregation, columnar_threshold: usize) -> Value {
    let (mut executor, _ctx) = create_test_executor();
    executor.config.columnar_threshold = columnar_threshold;
    let mut context = ExecutionContext::new(HashMap::new(), None);
    context.set_variable("n", Value::Array(nodes.to_vec()));
    executor
        .execute_aggregate(&mut context, &[], std::slice::from_ref(&agg), None)
        .expect("aggregate should succeed");
    context
        .result_set
        .rows
        .first()
        .and_then(|r| r.values.first())
        .cloned()
        .expect("aggregate must produce a row")
}

fn assert_parity(nodes: &[Value], agg: Aggregation, label: &str) {
    let row_path = aggregate_with_threshold(nodes, agg.clone(), usize::MAX);
    let columnar = aggregate_with_threshold(nodes, agg, 4096);
    assert_eq!(
        row_path, columnar,
        "row/columnar parity broken for `{}`: row={:?} columnar={:?}",
        label, row_path, columnar
    );
}

fn agg_alias(op: &str, col: &str) -> String {
    format!("{}({})", op, col)
}

#[test]
fn aggregate_columnar_matches_row_path_on_10k_i64() {
    let nodes: Vec<Value> = (0..10_000)
        .map(|i| build_person(i, i as i64, i as f64 * 0.5))
        .collect();
    assert!(nodes.len() > 4096, "fixture must exceed columnar threshold");

    for (op, agg) in [
        (
            "sum",
            Aggregation::Sum {
                column: "n.age".into(),
                alias: agg_alias("sum", "n.age"),
            },
        ),
        (
            "min",
            Aggregation::Min {
                column: "n.age".into(),
                alias: agg_alias("min", "n.age"),
            },
        ),
        (
            "max",
            Aggregation::Max {
                column: "n.age".into(),
                alias: agg_alias("max", "n.age"),
            },
        ),
        (
            "avg",
            Aggregation::Avg {
                column: "n.age".into(),
                alias: agg_alias("avg", "n.age"),
            },
        ),
    ] {
        assert_parity(&nodes, agg, &format!("{}(n.age)", op));
    }
}

proptest::proptest! {
    #![proptest_config(proptest::test_runner::Config {
        cases: 20,
        ..proptest::test_runner::Config::default()
    })]

    /// For randomised integer fixtures bigger than the columnar
    /// threshold, every groupless `SUM`/`MIN`/`MAX`/`AVG` on both
    /// the integer column (`n.age`) and the derived half-integer
    /// float column (`n.score = age * 0.5`) must match the
    /// scalar baseline bit-for-bit. The `a * 0.5` derivation
    /// keeps every score exactly representable as `f64` so the
    /// equality stays strict — no tolerance fudge required.
    #[test]
    fn prop_aggregate_columnar_matches_row_path(
        ages in proptest::collection::vec(-10_000i64..10_000, 4100..4200usize)
    ) {
        let nodes: Vec<Value> = ages
            .iter()
            .enumerate()
            .map(|(i, &a)| build_person(i as u64, a, a as f64 * 0.5))
            .collect();

        for op in ["sum", "min", "max", "avg"] {
            for col in ["n.age", "n.score"] {
                let agg = match op {
                    "sum" => Aggregation::Sum {
                        column: col.into(),
                        alias: agg_alias(op, col),
                    },
                    "min" => Aggregation::Min {
                        column: col.into(),
                        alias: agg_alias(op, col),
                    },
                    "max" => Aggregation::Max {
                        column: col.into(),
                        alias: agg_alias(op, col),
                    },
                    "avg" => Aggregation::Avg {
                        column: col.into(),
                        alias: agg_alias(op, col),
                    },
                    _ => unreachable!(),
                };
                let row_path = aggregate_with_threshold(&nodes, agg.clone(), usize::MAX);
                let columnar = aggregate_with_threshold(&nodes, agg, 4096);
                proptest::prop_assert_eq!(
                    row_path,
                    columnar,
                    "parity broken for {}({})",
                    op,
                    col
                );
            }
        }
    }
}

#[test]
fn prefer_columnar_hint_forces_aggregate_fast_path_below_threshold() {
    // 500 rows is below the default 4096 threshold — without a
    // hint, the columnar cache is skipped and the scalar arms
    // run. `PreferColumnar(true)` must make the fast path fire
    // regardless, with identical output.
    let nodes: Vec<Value> = (0..500)
        .map(|i| build_person(i as u64, i as i64, i as f64 * 0.5))
        .collect();

    let (mut executor, _ctx) = create_test_executor();
    executor.config.columnar_threshold = 4096;
    let mut context = ExecutionContext::new(HashMap::new(), None);
    context.set_plan_hints(vec![crate::executor::planner::PlanHint::PreferColumnar(
        true,
    )]);
    context.set_variable("n", Value::Array(nodes.clone()));
    let agg = Aggregation::Sum {
        column: "n.age".into(),
        alias: agg_alias("sum", "n.age"),
    };
    executor
        .execute_aggregate(&mut context, &[], std::slice::from_ref(&agg), None)
        .expect("aggregate should succeed");
    let hinted = context
        .result_set
        .rows
        .first()
        .and_then(|r| r.values.first())
        .cloned()
        .expect("aggregate must produce a row");

    let baseline = aggregate_with_threshold(&nodes, agg, usize::MAX);
    assert_eq!(hinted, baseline, "hint must not change output values");
}

#[test]
fn disable_columnar_hint_forces_aggregate_row_path_above_threshold() {
    // 5 000 rows would normally trip the columnar cache.
    // `PreferColumnar(false)` forces the scalar arms.
    let nodes: Vec<Value> = (0..5_000)
        .map(|i| build_person(i as u64, i as i64, i as f64 * 0.5))
        .collect();

    let (mut executor, _ctx) = create_test_executor();
    executor.config.columnar_threshold = 4096;
    let mut context = ExecutionContext::new(HashMap::new(), None);
    context.set_plan_hints(vec![crate::executor::planner::PlanHint::PreferColumnar(
        false,
    )]);
    context.set_variable("n", Value::Array(nodes.clone()));
    let agg = Aggregation::Max {
        column: "n.age".into(),
        alias: agg_alias("max", "n.age"),
    };
    executor
        .execute_aggregate(&mut context, &[], std::slice::from_ref(&agg), None)
        .expect("aggregate should succeed");
    let hinted = context
        .result_set
        .rows
        .first()
        .and_then(|r| r.values.first())
        .cloned()
        .expect("aggregate must produce a row");

    let baseline = aggregate_with_threshold(&nodes, agg, 4096);
    assert_eq!(hinted, baseline, "hint must not change output values");
}

#[test]
fn aggregate_columnar_matches_row_path_on_10k_f64() {
    let nodes: Vec<Value> = (0..10_000)
        .map(|i| build_person(i, i as i64, i as f64 * 0.5))
        .collect();
    assert!(nodes.len() > 4096, "fixture must exceed columnar threshold");

    for (op, agg) in [
        (
            "sum",
            Aggregation::Sum {
                column: "n.score".into(),
                alias: agg_alias("sum", "n.score"),
            },
        ),
        (
            "min",
            Aggregation::Min {
                column: "n.score".into(),
                alias: agg_alias("min", "n.score"),
            },
        ),
        (
            "max",
            Aggregation::Max {
                column: "n.score".into(),
                alias: agg_alias("max", "n.score"),
            },
        ),
        (
            "avg",
            Aggregation::Avg {
                column: "n.score".into(),
                alias: agg_alias("avg", "n.score"),
            },
        ),
    ] {
        assert_parity(&nodes, agg, &format!("{}(n.score)", op));
    }
}
