//! Coordinator-side merge operators.
//!
//! The coordinator never re-executes user logic — it just re-orders,
//! truncates, or folds the rows each shard produced. Every merge
//! variant is deterministic given the same inputs in the same order,
//! which makes the unit tests straightforward.

use std::cmp::Ordering;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use super::plan::Row;

/// How per-shard rows should be combined at the coordinator.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum MergeOp {
    /// Concatenate all shard rows in shard-id order. No ordering
    /// implied — clients that want a specific order must use an
    /// `OrderBy` merge.
    #[default]
    Concat,
    /// Sort + truncate: collect rows from every shard, sort by
    /// `sort_keys`, then keep at most `limit` rows.
    ///
    /// `sort_keys[i].column` is the column index (0-based) the sort
    /// is applied to.
    OrderBy {
        /// Sort keys applied in priority order (first is primary).
        sort_keys: Vec<SortKey>,
        /// Upper bound on the returned row count.
        limit: usize,
    },
    /// Distributed aggregation: the shard-local plans emit partial
    /// aggregates that the coordinator folds together per column.
    /// `aggs[i]` corresponds to `columns[i]`.
    Aggregate {
        /// Per-output-column aggregation rule.
        aggs: Vec<AggregationMerge>,
    },
    /// Distinct-set union: keep the first occurrence of each row
    /// (by full-row equality), shards traversed in shard-id order.
    DistinctUnion,
}

/// Direction for a single [`SortKey`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderDir {
    /// Ascending.
    Asc,
    /// Descending.
    Desc,
}

/// One sort key in an `OrderBy` merge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SortKey {
    /// Column index in the row.
    pub column: usize,
    /// Sort direction.
    pub direction: OrderDir,
}

/// A single partial-aggregation rule, one per output column.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AggregationMerge {
    /// Pass the shard's value through unchanged (grouping column).
    /// All shards must produce the same value; the coordinator
    /// deduplicates.
    Identity { column: usize },
    /// Sum of per-shard partial sums / counts.
    Sum { column: usize },
    /// Minimum across shard values.
    Min { column: usize },
    /// Maximum across shard values.
    Max { column: usize },
    /// AVG decomposed as (sum, count) pair on each shard: the
    /// coordinator sums both then divides.
    Avg {
        /// Column holding the per-shard sum.
        sum_column: usize,
        /// Column holding the per-shard count.
        count_column: usize,
    },
    /// Union of per-shard COLLECT lists. Each shard emits an array;
    /// the coordinator flattens.
    CollectUnion { column: usize },
}

/// Errors produced by the merge stage.
#[derive(Debug, Error)]
pub enum MergeError {
    /// A row had fewer columns than a referenced index.
    #[error("column index {index} out of range (row width={width})")]
    ColumnOutOfRange { index: usize, width: usize },
    /// An Identity aggregation saw conflicting values across shards.
    #[error("identity aggregation conflict on column {column}: {left} vs {right}")]
    IdentityConflict {
        column: usize,
        left: String,
        right: String,
    },
    /// A Sum / Avg aggregation received a non-numeric value.
    #[error("non-numeric value {got} in numeric aggregation on column {column}")]
    NonNumeric { column: usize, got: String },
    /// A CollectUnion aggregation received a non-array value.
    #[error("non-array value in COLLECT aggregation on column {column}")]
    NonArray { column: usize },
}

/// Apply `op` to the set of per-shard row batches and return a single
/// merged batch. `per_shard[i]` is the rows returned by the i-th shard
/// in scatter order.
pub fn merge(op: &MergeOp, per_shard: Vec<Vec<Row>>) -> Result<Vec<Row>, MergeError> {
    match op {
        MergeOp::Concat => Ok(per_shard.into_iter().flatten().collect()),
        MergeOp::OrderBy { sort_keys, limit } => merge_order_by(per_shard, sort_keys, *limit),
        MergeOp::Aggregate { aggs } => merge_aggregate(per_shard, aggs),
        MergeOp::DistinctUnion => merge_distinct(per_shard),
    }
}

fn merge_order_by(
    per_shard: Vec<Vec<Row>>,
    sort_keys: &[SortKey],
    limit: usize,
) -> Result<Vec<Row>, MergeError> {
    let mut rows: Vec<Row> = per_shard.into_iter().flatten().collect();
    rows.sort_by(|a, b| compare_rows(a, b, sort_keys));
    if rows.len() > limit {
        rows.truncate(limit);
    }
    Ok(rows)
}

fn compare_rows(a: &Row, b: &Row, keys: &[SortKey]) -> Ordering {
    for key in keys {
        let va = a.get(key.column);
        let vb = b.get(key.column);
        let ord = compare_values(va, vb);
        let ord = match key.direction {
            OrderDir::Asc => ord,
            OrderDir::Desc => ord.reverse(),
        };
        if ord != Ordering::Equal {
            return ord;
        }
    }
    Ordering::Equal
}

fn compare_values(a: Option<&Value>, b: Option<&Value>) -> Ordering {
    match (a, b) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Less,
        (Some(_), None) => Ordering::Greater,
        (Some(x), Some(y)) => value_cmp(x, y),
    }
}

fn value_cmp(a: &Value, b: &Value) -> Ordering {
    match (a, b) {
        (Value::Null, Value::Null) => Ordering::Equal,
        (Value::Null, _) => Ordering::Less,
        (_, Value::Null) => Ordering::Greater,
        (Value::Bool(x), Value::Bool(y)) => x.cmp(y),
        (Value::Number(x), Value::Number(y)) => {
            let xf = x.as_f64().unwrap_or(f64::NAN);
            let yf = y.as_f64().unwrap_or(f64::NAN);
            xf.partial_cmp(&yf).unwrap_or(Ordering::Equal)
        }
        (Value::String(x), Value::String(y)) => x.cmp(y),
        _ => {
            // Mixed / compound — fall back to lex compare on the JSON
            // repr. Deterministic and good enough for tests.
            a.to_string().cmp(&b.to_string())
        }
    }
}

fn merge_aggregate(
    per_shard: Vec<Vec<Row>>,
    aggs: &[AggregationMerge],
) -> Result<Vec<Row>, MergeError> {
    if per_shard.is_empty() || per_shard.iter().all(|s| s.is_empty()) {
        return Ok(vec![zero_row(aggs)]);
    }
    // Each shard contributes exactly one row of partial aggregates.
    // Validate and fold.
    let mut acc: Vec<AggState> = aggs.iter().map(|a| AggState::zero(a)).collect();
    for shard_rows in per_shard.into_iter() {
        // Multiple rows per shard → fold each one into acc.
        for row in shard_rows {
            for (i, agg) in aggs.iter().enumerate() {
                acc[i].fold(agg, &row)?;
            }
        }
    }
    let out: Row = acc
        .into_iter()
        .zip(aggs.iter())
        .map(|(s, a)| s.finalize(a))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(vec![out])
}

fn zero_row(aggs: &[AggregationMerge]) -> Row {
    aggs.iter()
        .map(|a| match a {
            AggregationMerge::Sum { .. } => Value::from(0u64),
            AggregationMerge::Min { .. } | AggregationMerge::Max { .. } => Value::Null,
            AggregationMerge::Avg { .. } => Value::Null,
            AggregationMerge::CollectUnion { .. } => Value::Array(vec![]),
            AggregationMerge::Identity { .. } => Value::Null,
        })
        .collect()
}

enum AggState {
    Identity(Option<Value>),
    SumF(f64),
    MinF(Option<f64>),
    MaxF(Option<f64>),
    Avg { sum: f64, count: u64 },
    Collect(Vec<Value>),
}

impl AggState {
    fn zero(a: &AggregationMerge) -> Self {
        match a {
            AggregationMerge::Identity { .. } => Self::Identity(None),
            AggregationMerge::Sum { .. } => Self::SumF(0.0),
            AggregationMerge::Min { .. } => Self::MinF(None),
            AggregationMerge::Max { .. } => Self::MaxF(None),
            AggregationMerge::Avg { .. } => Self::Avg { sum: 0.0, count: 0 },
            AggregationMerge::CollectUnion { .. } => Self::Collect(Vec::new()),
        }
    }

    fn fold(&mut self, agg: &AggregationMerge, row: &Row) -> Result<(), MergeError> {
        match (self, agg) {
            (Self::Identity(slot), AggregationMerge::Identity { column }) => {
                let v = col(row, *column)?;
                match slot {
                    Some(existing) if existing != v => {
                        return Err(MergeError::IdentityConflict {
                            column: *column,
                            left: existing.to_string(),
                            right: v.to_string(),
                        });
                    }
                    Some(_) => {}
                    None => *slot = Some(v.clone()),
                }
            }
            (Self::SumF(acc), AggregationMerge::Sum { column }) => {
                *acc += as_f64(col(row, *column)?, *column)?;
            }
            (Self::MinF(acc), AggregationMerge::Min { column }) => {
                let v = as_f64(col(row, *column)?, *column)?;
                *acc = Some(acc.map_or(v, |a| a.min(v)));
            }
            (Self::MaxF(acc), AggregationMerge::Max { column }) => {
                let v = as_f64(col(row, *column)?, *column)?;
                *acc = Some(acc.map_or(v, |a| a.max(v)));
            }
            (
                Self::Avg { sum, count },
                AggregationMerge::Avg {
                    sum_column,
                    count_column,
                },
            ) => {
                *sum += as_f64(col(row, *sum_column)?, *sum_column)?;
                let c = col(row, *count_column)?;
                let inc = c.as_u64().ok_or_else(|| MergeError::NonNumeric {
                    column: *count_column,
                    got: c.to_string(),
                })?;
                *count += inc;
            }
            (Self::Collect(acc), AggregationMerge::CollectUnion { column }) => {
                match col(row, *column)? {
                    Value::Array(items) => acc.extend(items.iter().cloned()),
                    _ => return Err(MergeError::NonArray { column: *column }),
                }
            }
            _ => unreachable!("AggState and AggregationMerge variants out of sync"),
        }
        Ok(())
    }

    fn finalize(self, agg: &AggregationMerge) -> Result<Value, MergeError> {
        match (self, agg) {
            (Self::Identity(v), _) => Ok(v.unwrap_or(Value::Null)),
            (Self::SumF(x), _) => {
                // Preserve integer-ness when possible.
                if x.fract() == 0.0 && x.is_finite() && x.abs() < i64::MAX as f64 {
                    Ok(Value::from(x as i64))
                } else {
                    Ok(serde_json::json!(x))
                }
            }
            (Self::MinF(x), _) | (Self::MaxF(x), _) => match x {
                Some(v) if v.fract() == 0.0 && v.is_finite() && v.abs() < i64::MAX as f64 => {
                    Ok(Value::from(v as i64))
                }
                Some(v) => Ok(serde_json::json!(v)),
                None => Ok(Value::Null),
            },
            (Self::Avg { sum, count }, _) => {
                if count == 0 {
                    Ok(Value::Null)
                } else {
                    Ok(serde_json::json!(sum / count as f64))
                }
            }
            (Self::Collect(items), _) => Ok(Value::Array(items)),
        }
    }
}

fn col(row: &Row, idx: usize) -> Result<&Value, MergeError> {
    row.get(idx).ok_or(MergeError::ColumnOutOfRange {
        index: idx,
        width: row.len(),
    })
}

fn as_f64(v: &Value, col: usize) -> Result<f64, MergeError> {
    v.as_f64().ok_or_else(|| MergeError::NonNumeric {
        column: col,
        got: v.to_string(),
    })
}

fn merge_distinct(per_shard: Vec<Vec<Row>>) -> Result<Vec<Row>, MergeError> {
    let mut seen: Vec<Row> = Vec::new();
    for shard_rows in per_shard.into_iter() {
        for row in shard_rows {
            if !seen.contains(&row) {
                seen.push(row);
            }
        }
    }
    Ok(seen)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r<const N: usize>(xs: [Value; N]) -> Row {
        xs.into_iter().collect()
    }

    #[test]
    fn concat_preserves_shard_order() {
        let out = merge(
            &MergeOp::Concat,
            vec![
                vec![r([Value::from(1)])],
                vec![r([Value::from(2)]), r([Value::from(3)])],
            ],
        )
        .unwrap();
        assert_eq!(out.len(), 3);
        assert_eq!(out[0][0], Value::from(1));
        assert_eq!(out[2][0], Value::from(3));
    }

    #[test]
    fn order_by_asc_then_limit() {
        let op = MergeOp::OrderBy {
            sort_keys: vec![SortKey {
                column: 0,
                direction: OrderDir::Asc,
            }],
            limit: 2,
        };
        let per_shard = vec![
            vec![r([Value::from(5)]), r([Value::from(2)])],
            vec![r([Value::from(8)]), r([Value::from(1)])],
        ];
        let out = merge(&op, per_shard).unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(out[0][0], Value::from(1));
        assert_eq!(out[1][0], Value::from(2));
    }

    #[test]
    fn order_by_desc_picks_largest() {
        let op = MergeOp::OrderBy {
            sort_keys: vec![SortKey {
                column: 0,
                direction: OrderDir::Desc,
            }],
            limit: 2,
        };
        let per_shard = vec![
            vec![r([Value::from(5)]), r([Value::from(2)])],
            vec![r([Value::from(8)]), r([Value::from(1)])],
        ];
        let out = merge(&op, per_shard).unwrap();
        assert_eq!(out[0][0], Value::from(8));
        assert_eq!(out[1][0], Value::from(5));
    }

    #[test]
    fn order_by_limit_zero_returns_empty() {
        let op = MergeOp::OrderBy {
            sort_keys: vec![SortKey {
                column: 0,
                direction: OrderDir::Asc,
            }],
            limit: 0,
        };
        let out = merge(&op, vec![vec![r([Value::from(1)])]]).unwrap();
        assert!(out.is_empty());
    }

    #[test]
    fn sum_aggregation_adds_partials() {
        let op = MergeOp::Aggregate {
            aggs: vec![AggregationMerge::Sum { column: 0 }],
        };
        let per_shard = vec![
            vec![r([Value::from(10)])],
            vec![r([Value::from(20)])],
            vec![r([Value::from(12)])],
        ];
        let out = merge(&op, per_shard).unwrap();
        assert_eq!(out, vec![r([Value::from(42)])]);
    }

    #[test]
    fn avg_aggregation_decomposes_correctly() {
        // Spec §3 Given partial (sum,count) pairs (100,5), (200,10), (300,15)
        // When merged Then final = 600/30 = 20.0
        let op = MergeOp::Aggregate {
            aggs: vec![AggregationMerge::Avg {
                sum_column: 0,
                count_column: 1,
            }],
        };
        let per_shard = vec![
            vec![r([Value::from(100), Value::from(5)])],
            vec![r([Value::from(200), Value::from(10)])],
            vec![r([Value::from(300), Value::from(15)])],
        ];
        let out = merge(&op, per_shard).unwrap();
        assert_eq!(out.len(), 1);
        let got = out[0][0].as_f64().unwrap();
        assert!((got - 20.0).abs() < 1e-9);
    }

    #[test]
    fn min_max_over_shards() {
        let op = MergeOp::Aggregate {
            aggs: vec![
                AggregationMerge::Min { column: 0 },
                AggregationMerge::Max { column: 0 },
            ],
        };
        let per_shard = vec![
            vec![r([Value::from(5), Value::from(5)])],
            vec![r([Value::from(1), Value::from(1)])],
            vec![r([Value::from(9), Value::from(9)])],
        ];
        let out = merge(&op, per_shard).unwrap();
        assert_eq!(out[0][0], Value::from(1));
        assert_eq!(out[0][1], Value::from(9));
    }

    #[test]
    fn collect_union_flattens() {
        let op = MergeOp::Aggregate {
            aggs: vec![AggregationMerge::CollectUnion { column: 0 }],
        };
        let per_shard = vec![
            vec![r([Value::Array(vec![Value::from(1), Value::from(2)])])],
            vec![r([Value::Array(vec![Value::from(3)])])],
        ];
        let out = merge(&op, per_shard).unwrap();
        assert_eq!(
            out[0][0],
            Value::Array(vec![Value::from(1), Value::from(2), Value::from(3)])
        );
    }

    #[test]
    fn identity_aggregation_rejects_conflict() {
        let op = MergeOp::Aggregate {
            aggs: vec![AggregationMerge::Identity { column: 0 }],
        };
        let per_shard = vec![
            vec![r([Value::from(1)])],
            vec![r([Value::from(2)])], // conflicts!
        ];
        let err = merge(&op, per_shard).unwrap_err();
        assert!(matches!(err, MergeError::IdentityConflict { .. }));
    }

    #[test]
    fn distinct_union_dedupes() {
        let out = merge(
            &MergeOp::DistinctUnion,
            vec![
                vec![r([Value::from(1)]), r([Value::from(2)])],
                vec![r([Value::from(2)]), r([Value::from(3)])],
            ],
        )
        .unwrap();
        assert_eq!(out.len(), 3);
    }

    #[test]
    fn aggregation_empty_shards_returns_zero_row() {
        let op = MergeOp::Aggregate {
            aggs: vec![AggregationMerge::Sum { column: 0 }],
        };
        let out = merge(&op, vec![]).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0][0], Value::from(0));
    }

    #[test]
    fn column_out_of_range_surfaces_error() {
        let op = MergeOp::Aggregate {
            aggs: vec![AggregationMerge::Sum { column: 5 }],
        };
        let err = merge(&op, vec![vec![r([Value::from(1)])]]).unwrap_err();
        assert!(matches!(err, MergeError::ColumnOutOfRange { .. }));
    }

    #[test]
    fn sum_rejects_non_numeric() {
        let op = MergeOp::Aggregate {
            aggs: vec![AggregationMerge::Sum { column: 0 }],
        };
        let err = merge(&op, vec![vec![r([Value::from("hello")])]]).unwrap_err();
        assert!(matches!(err, MergeError::NonNumeric { .. }));
    }

    #[test]
    fn order_by_multi_key() {
        let op = MergeOp::OrderBy {
            sort_keys: vec![
                SortKey {
                    column: 0,
                    direction: OrderDir::Asc,
                },
                SortKey {
                    column: 1,
                    direction: OrderDir::Desc,
                },
            ],
            limit: 10,
        };
        let per_shard = vec![vec![
            r([Value::from(1), Value::from(10)]),
            r([Value::from(1), Value::from(20)]),
            r([Value::from(2), Value::from(5)]),
        ]];
        let out = merge(&op, per_shard).unwrap();
        // Primary asc on col 0: 1,1,2. Secondary desc on col 1 breaks
        // the tie: 20 before 10.
        assert_eq!(out[0][1], Value::from(20));
        assert_eq!(out[1][1], Value::from(10));
        assert_eq!(out[2][0], Value::from(2));
    }
}
