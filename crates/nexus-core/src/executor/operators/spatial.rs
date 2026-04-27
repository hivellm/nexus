//! `SpatialSeek` operator
//! (phase6_spatial-planner-seek §1.3 + §1.4).
//!
//! Probes [`crate::index::IndexManager::rtree`] (a registry of
//! packed Hilbert R-trees keyed by `{Label}.{property}`) and
//! emits one row per matching node — without going through a
//! `NodeByLabel` driver. The planner rewrites three Cypher
//! shapes into this operator when an R-tree index exists:
//!
//! - `WHERE point.withinBBox(n.prop, $lower, $upper)` ->
//!   [`SeekMode::Bbox`]
//! - `WHERE point.withinDistance(n.prop, $p, $d)` ->
//!   [`SeekMode::WithinDistance`]
//! - `ORDER BY distance(n.prop, $p) ASC LIMIT k` and the
//!   function-style `point.nearest(n.prop, k)` ->
//!   [`SeekMode::Nearest`]
//!
//! Each emitted row carries:
//!
//! - the bound pattern variable (a node value with `_nexus_id`)
//! - for `Nearest`, an additional `distance` column with the
//!   Cartesian distance from the query point.
//!
//! Plan dispatch goes through `dispatch.rs`, which routes
//! `Operator::SpatialSeek` here.

use super::super::context::ExecutionContext;
use super::super::engine::Executor;
use super::super::types::{ResultSet, Row, SeekMode};
use crate::index::rtree::Metric;
use crate::{Error, Result};
use serde_json::Value;

impl Executor {
    /// Execute a `SpatialSeek` against the engine's R-tree
    /// registry. Errors out with `ERR_SPATIAL_INDEX_NOT_FOUND`
    /// when the requested index is not registered — the planner
    /// is responsible for falling back to a `NodeByLabel +
    /// Filter` plan in that case, so a missing index here is a
    /// programming error rather than a query error.
    pub(in crate::executor) fn execute_spatial_seek(
        &self,
        context: &mut ExecutionContext,
        index_id: &str,
        variable: &str,
        mode: &SeekMode,
    ) -> Result<()> {
        let registry = self.shared.rtree_registry.clone();
        let Some(tree) = registry.snapshot(index_id) else {
            return Err(Error::CypherExecution(format!(
                "ERR_SPATIAL_INDEX_NOT_FOUND: no R-tree index registered as {index_id:?}"
            )));
        };

        let (ids, distances): (Vec<u64>, Vec<Option<f64>>) = match mode {
            SeekMode::Bbox {
                min_x,
                min_y,
                max_x,
                max_y,
            } => {
                let raw = tree.query_bbox(*min_x, *min_y, *max_x, *max_y);
                let n = raw.len();
                (raw, vec![None; n])
            }
            SeekMode::WithinDistance {
                center_x,
                center_y,
                meters,
            } => {
                let raw = tree
                    .within_distance(*center_x, *center_y, *meters, Metric::Cartesian)
                    .map_err(|e| Error::CypherExecution(format!("ERR_SPATIAL_SEEK_FAILED: {e}")))?;
                let n = raw.len();
                (raw, vec![None; n])
            }
            SeekMode::Nearest {
                center_x,
                center_y,
                k,
            } => {
                let hits = tree
                    .nearest(*center_x, *center_y, *k, Metric::Cartesian)
                    .map_err(|e| Error::CypherExecution(format!("ERR_SPATIAL_SEEK_FAILED: {e}")))?;
                let mut ids = Vec::with_capacity(hits.len());
                let mut dists = Vec::with_capacity(hits.len());
                for h in hits {
                    ids.push(h.node_id);
                    dists.push(Some(h.distance));
                }
                (ids, dists)
            }
        };

        let with_distance = matches!(mode, SeekMode::Nearest { .. });
        let mut columns: Vec<String> = vec![variable.to_string()];
        if with_distance {
            columns.push("distance".to_string());
        }

        let mut rows: Vec<Row> = Vec::with_capacity(ids.len());
        for (idx, node_id) in ids.iter().enumerate() {
            let node = match self.read_node_as_value(*node_id) {
                Ok(v) => v,
                Err(_) => {
                    // The R-tree may carry an entry for a node
                    // that was tombstoned or never materialised
                    // (recovery scenarios). Skip silently — the
                    // executor's MVCC layer drops invisible ids.
                    continue;
                }
            };
            let mut values = Vec::with_capacity(columns.len());
            values.push(node);
            if with_distance {
                let d = distances.get(idx).copied().flatten();
                values.push(match d {
                    Some(d) => serde_json::Number::from_f64(d)
                        .map(Value::Number)
                        .unwrap_or(Value::Null),
                    None => Value::Null,
                });
            }
            rows.push(Row { values });
        }

        // Bind the pattern variable so downstream operators
        // (`Filter`, `Project`, `Return`) can reference `n` /
        // `n.prop` the same way they would after a `NodeByLabel`.
        if let Some(first) = rows.first().and_then(|r| r.values.first()).cloned() {
            context.set_variable(variable, first);
        }
        context.result_set = ResultSet { columns, rows };
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::types::SeekMode;
    use crate::index::rtree::RTree;
    use crate::testing::create_isolated_test_executor;

    fn seed_places(executor: &mut crate::executor::Executor, coords: &[(f64, f64)]) -> Vec<u64> {
        let mut ids = Vec::new();
        for (i, (x, y)) in coords.iter().enumerate() {
            let q = format!("CREATE (p:Place {{name: 'P{i}', x: {x}, y: {y}}}) RETURN p");
            let rs = executor
                .execute(&crate::executor::Query {
                    cypher: q,
                    params: std::collections::HashMap::new(),
                })
                .unwrap();
            let node = rs.rows[0].values[0].as_object().unwrap();
            let id = node.get("_nexus_id").and_then(|v| v.as_u64()).unwrap();
            ids.push(id);
        }
        ids
    }

    fn install_rtree(
        executor: &crate::executor::Executor,
        index_id: &str,
        entries: &[(u64, f64, f64)],
    ) {
        let mut tree = RTree::new();
        for (id, x, y) in entries {
            tree.insert(*id, *x, *y);
        }
        executor.rtree_registry().swap_in(index_id, tree);
    }

    #[test]
    fn spatial_seek_bbox_emits_only_matching_rows() {
        let (mut executor, _ctx) = create_isolated_test_executor();
        let coords = [(0.0, 0.0), (5.0, 5.0), (10.0, 10.0), (50.0, 50.0)];
        let ids = seed_places(&mut executor, &coords);
        install_rtree(
            &executor,
            "Place.loc",
            &ids.iter()
                .copied()
                .zip(coords.iter().copied())
                .map(|(id, (x, y))| (id, x, y))
                .collect::<Vec<_>>(),
        );

        let mut ctx =
            crate::executor::ExecutionContext::new(std::collections::HashMap::new(), None);
        executor
            .execute_spatial_seek(
                &mut ctx,
                "Place.loc",
                "p",
                &SeekMode::Bbox {
                    min_x: -1.0,
                    min_y: -1.0,
                    max_x: 11.0,
                    max_y: 11.0,
                },
            )
            .unwrap();
        // Three points lie in the bbox: indices 0, 1, 2.
        assert_eq!(ctx.result_set.columns, vec!["p"]);
        assert_eq!(ctx.result_set.rows.len(), 3);
    }

    #[test]
    fn spatial_seek_within_distance_filters_by_radius() {
        let (mut executor, _ctx) = create_isolated_test_executor();
        let coords = [(0.0, 0.0), (1.0, 0.0), (3.0, 0.0), (10.0, 0.0)];
        let ids = seed_places(&mut executor, &coords);
        install_rtree(
            &executor,
            "Place.loc",
            &ids.iter()
                .copied()
                .zip(coords.iter().copied())
                .map(|(id, (x, y))| (id, x, y))
                .collect::<Vec<_>>(),
        );

        let mut ctx =
            crate::executor::ExecutionContext::new(std::collections::HashMap::new(), None);
        executor
            .execute_spatial_seek(
                &mut ctx,
                "Place.loc",
                "p",
                &SeekMode::WithinDistance {
                    center_x: 0.0,
                    center_y: 0.0,
                    meters: 1.5,
                },
            )
            .unwrap();
        // Within radius 1.5 → indices 0 (d=0) and 1 (d=1).
        assert_eq!(ctx.result_set.rows.len(), 2);
    }

    #[test]
    fn spatial_seek_nearest_emits_distance_column() {
        let (mut executor, _ctx) = create_isolated_test_executor();
        let coords = [(0.0, 0.0), (1.0, 0.0), (3.0, 0.0)];
        let ids = seed_places(&mut executor, &coords);
        install_rtree(
            &executor,
            "Place.loc",
            &ids.iter()
                .copied()
                .zip(coords.iter().copied())
                .map(|(id, (x, y))| (id, x, y))
                .collect::<Vec<_>>(),
        );

        let mut ctx =
            crate::executor::ExecutionContext::new(std::collections::HashMap::new(), None);
        executor
            .execute_spatial_seek(
                &mut ctx,
                "Place.loc",
                "p",
                &SeekMode::Nearest {
                    center_x: 0.0,
                    center_y: 0.0,
                    k: 2,
                },
            )
            .unwrap();
        assert_eq!(ctx.result_set.columns, vec!["p", "distance"]);
        assert_eq!(ctx.result_set.rows.len(), 2);
        // First row is the closest (distance 0).
        let d0 = ctx.result_set.rows[0].values[1].as_f64().unwrap();
        assert!((d0 - 0.0).abs() < 1e-9);
        // Second row distance is 1.0.
        let d1 = ctx.result_set.rows[1].values[1].as_f64().unwrap();
        assert!((d1 - 1.0).abs() < 1e-9);
    }

    #[test]
    fn spatial_seek_unknown_index_errors_typed() {
        let (executor, _ctx) = create_isolated_test_executor();
        let mut ctx =
            crate::executor::ExecutionContext::new(std::collections::HashMap::new(), None);
        let err = executor
            .execute_spatial_seek(
                &mut ctx,
                "Missing.loc",
                "p",
                &SeekMode::Bbox {
                    min_x: 0.0,
                    min_y: 0.0,
                    max_x: 1.0,
                    max_y: 1.0,
                },
            )
            .unwrap_err();
        assert!(
            err.to_string().contains("ERR_SPATIAL_INDEX_NOT_FOUND"),
            "expected ERR_SPATIAL_INDEX_NOT_FOUND, got: {err}"
        );
    }

    #[test]
    fn spatial_seek_returns_no_rows_for_empty_index() {
        let (executor, _ctx) = create_isolated_test_executor();
        // Register but do not populate.
        executor.rtree_registry().register_empty("Empty.loc");
        let mut ctx =
            crate::executor::ExecutionContext::new(std::collections::HashMap::new(), None);
        executor
            .execute_spatial_seek(
                &mut ctx,
                "Empty.loc",
                "p",
                &SeekMode::Nearest {
                    center_x: 0.0,
                    center_y: 0.0,
                    k: 5,
                },
            )
            .unwrap();
        assert!(ctx.result_set.rows.is_empty());
    }
}
