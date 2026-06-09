//! Spatial index auto-populate hooks and engine-aware spatial procedures:
//! `spatial.addPoint`, `spatial.nearest`, and the `spatial_autopopulate_node`
//! / `spatial_refresh_node` / `spatial_evict_node` maintenance hooks called
//! from CREATE / SET / DELETE paths.

use super::super::super::context::ExecutionContext;
use super::super::super::engine::Executor;
use super::super::super::parser;
use super::super::super::types::Row;
use crate::{Error, Result};
use serde_json::Value;

impl Executor {
    // ──────────── phase6_spatial-index-autopopulate hooks ────────────
    //
    // These mirror `fts_autopopulate_node` / refresh / evict but route
    // through `IndexManager::rtree` (shared with the engine via
    // `ExecutorShared::rtree_registry`). Wired into the same CREATE /
    // SET / DELETE call sites the FTS hooks use, so any path that
    // mutates the node store keeps every spatial index in lockstep
    // without requiring a manual `spatial.addPoint` call.

    /// §2 — auto-populate every registered spatial index whose
    /// `(label, property)` pair matches the node just created.
    ///
    /// For every registered index, when the node carries one of
    /// the indexed labels AND the indexed property is a Point,
    /// insert into the registry and emit `RTreeInsert` for replay.
    pub(in crate::executor) fn spatial_autopopulate_node(
        &self,
        node_id: u64,
        label_ids: &[u32],
        properties: &serde_json::Value,
    ) {
        let Some(props_obj) = properties.as_object() else {
            return;
        };
        let registry = self.shared.rtree_registry.clone();
        for (idx_name, idx_label, prop_key) in registry.definitions() {
            let label_id = match self.catalog().get_label_id(&idx_label) {
                Ok(id) => id,
                Err(_) => continue,
            };
            if !label_ids.contains(&label_id) {
                continue;
            }
            let Some(val) = props_obj.get(&prop_key) else {
                continue;
            };
            let point = match crate::geospatial::Point::from_json_value(val) {
                Ok(p) => p,
                Err(_) => continue,
            };
            registry.insert_point(&idx_name, node_id, point.x, point.y);
            // Replay-WAL journaling lives on the engine-side hook
            // (`Engine::spatial_autopopulate_node`); the executor
            // hook only keeps the in-memory tree current. This
            // matches the FTS auto-populate layering — the
            // executor crate does not own the WAL handle.
            let _ = (node_id, point.x, point.y, &idx_name);
        }
    }

    /// §3 — delete-then-conditional-add the node's point in every
    /// spatial index after a SET / REMOVE.
    pub(in crate::executor) fn spatial_refresh_node(
        &self,
        node_id: u64,
        label_ids: &[u32],
        new_props: &serde_json::Value,
    ) {
        let registry = self.shared.rtree_registry.clone();

        // Phase 1: evict the stale entry from every index the node
        // currently belongs to.
        for name in registry.indexes_containing(node_id) {
            registry.delete_point(&name, node_id);
            // See spatial_autopopulate_node note: WAL journalling
            // lives on the engine-side hook.
            let _ = (&name, node_id);
        }

        // Phase 2: re-insert where the new value is still a valid
        // Point AND the node's labels match the index definition.
        let Some(obj) = new_props.as_object() else {
            return;
        };
        for (idx_name, idx_label, prop_key) in registry.definitions() {
            let label_matches = match self.catalog().get_label_id(&idx_label) {
                Ok(id) => label_ids.contains(&id),
                Err(_) => false,
            };
            if !label_matches {
                continue;
            }
            let Some(val) = obj.get(&prop_key) else {
                continue;
            };
            let point = match crate::geospatial::Point::from_json_value(val) {
                Ok(p) => p,
                Err(_) => continue,
            };
            registry.insert_point(&idx_name, node_id, point.x, point.y);
            // Replay-WAL journaling lives on the engine-side hook
            // (`Engine::spatial_autopopulate_node`); the executor
            // hook only keeps the in-memory tree current. This
            // matches the FTS auto-populate layering — the
            // executor crate does not own the WAL handle.
            let _ = (node_id, point.x, point.y, &idx_name);
        }
    }

    /// §4 — evict `node_id` from every spatial index that currently
    /// lists it as a member. Called from the DELETE path before the
    /// storage record is marked deleted.
    pub(in crate::executor) fn spatial_evict_node(&self, node_id: u64) {
        let registry = self.shared.rtree_registry.clone();
        for name in registry.indexes_containing(node_id) {
            registry.delete_point(&name, node_id);
            // See spatial_autopopulate_node note: WAL journalling
            // lives on the engine-side hook.
            let _ = (&name, node_id);
        }
    }

    /// `CALL spatial.addPoint(label, property, nodeId, point)` —
    /// insert a point into the spatial index registered for
    /// `{label}.{property}`. Returns `{added: BOOLEAN}`.
    ///
    /// Provided as the Cypher-level bulk-loader until auto-populate
    /// on CREATE / SET lands (follow-up task
    /// `phase6_spatial-index-autopopulate`). Scripts that build a
    /// dataset up-front can drive this procedure once per row to
    /// initialise the index, then rely on `spatial.nearest` /
    /// `point.*` predicates for reads.
    pub(in crate::executor) fn execute_spatial_add_point(
        &self,
        context: &mut ExecutionContext,
        arguments: &[parser::Expression],
        yield_columns: Option<&Vec<String>>,
    ) -> Result<()> {
        // §7.1 — telemetry breadcrumb so deployments can spot stragglers
        // still calling the legacy bulk-loader. The procedure stays
        // idempotent with the auto-populate hook; log at info so a
        // routine production run surfaces it without flipping debug.
        tracing::info!(
            "spatial.addPoint called — superseded by Cypher CRUD auto-populate (phase6_spatial-index-autopopulate); scheduled for removal in v2.0.0"
        );
        let label = match arguments.first() {
            Some(expr) => match self.evaluate_expression_in_context(context, expr)? {
                Value::String(s) => s,
                other => {
                    return Err(Error::CypherExecution(format!(
                        "ERR_INVALID_ARG_TYPE: spatial.addPoint `label` must be STRING (got \
                         {other})"
                    )));
                }
            },
            None => {
                return Err(Error::CypherExecution(
                    "ERR_MISSING_ARG: spatial.addPoint requires `label` at position 0".to_string(),
                ));
            }
        };
        let property = match arguments.get(1) {
            Some(expr) => match self.evaluate_expression_in_context(context, expr)? {
                Value::String(s) => s,
                other => {
                    return Err(Error::CypherExecution(format!(
                        "ERR_INVALID_ARG_TYPE: spatial.addPoint `property` must be STRING (got \
                         {other})"
                    )));
                }
            },
            None => {
                return Err(Error::CypherExecution(
                    "ERR_MISSING_ARG: spatial.addPoint requires `property` at position 1"
                        .to_string(),
                ));
            }
        };
        let node_id = match arguments.get(2) {
            Some(expr) => match self.evaluate_expression_in_context(context, expr)? {
                Value::Number(n) => n.as_u64().ok_or_else(|| {
                    Error::CypherExecution(
                        "ERR_INVALID_ARG_VALUE: spatial.addPoint `nodeId` must be a \
                         non-negative INTEGER"
                            .to_string(),
                    )
                })?,
                other => {
                    return Err(Error::CypherExecution(format!(
                        "ERR_INVALID_ARG_TYPE: spatial.addPoint `nodeId` must be INTEGER (got \
                         {other})"
                    )));
                }
            },
            None => {
                return Err(Error::CypherExecution(
                    "ERR_MISSING_ARG: spatial.addPoint requires `nodeId` at position 2".to_string(),
                ));
            }
        };
        let point_val = match arguments.get(3) {
            Some(expr) => self.evaluate_expression_in_context(context, expr)?,
            None => {
                return Err(Error::CypherExecution(
                    "ERR_MISSING_ARG: spatial.addPoint requires `point` at position 3".to_string(),
                ));
            }
        };
        if !matches!(point_val, Value::Object(_)) {
            return Err(Error::CypherExecution(format!(
                "ERR_INVALID_ARG_TYPE: spatial.addPoint `point` must be a POINT (got \
                 {point_val})"
            )));
        }
        let point = crate::geospatial::Point::from_json_value(&point_val).map_err(|e| {
            Error::CypherExecution(format!(
                "ERR_INVALID_ARG_TYPE: spatial.addPoint `point` is not a valid POINT: {e}"
            ))
        })?;
        // phase6_spatial-index-autopopulate §7.1 — procedure kept for
        // backwards compat; auto-populate on CREATE/SET now covers the
        // same writes. Emit a deprecation notice so operators can
        // instrument stragglers.
        tracing::info!(
            label = %label,
            property = %property,
            node_id = node_id,
            "spatial.addPoint called — superseded by auto-populate; see CHANGELOG"
        );
        let key = format!("{label}.{property}");
        let registry = &self.shared.rtree_registry;
        if !registry.contains(&key) {
            return Err(Error::CypherExecution(format!(
                "ERR_SPATIAL_INDEX_NOT_FOUND: no spatial index for `{key}` — run `CREATE \
                 SPATIAL INDEX ON :{label}({property})` first"
            )));
        }
        registry.insert_point(&key, node_id, point.x, point.y);
        let columns = yield_columns
            .cloned()
            .unwrap_or_else(|| vec!["added".to_string()]);
        context.set_columns_and_rows(
            columns,
            vec![Row {
                values: vec![Value::Bool(true)],
            }],
        );
        Ok(())
    }

    /// `CALL spatial.nearest(point, label, k)` — engine-aware
    /// k-NN procedure (phase6_opencypher-geospatial-predicates §7.3).
    ///
    /// Finds the indexed point closest to the query point, scanning
    /// the per-`label.property` spatial index registered via
    /// `CREATE SPATIAL INDEX`. Streams rows `(node, dist)` ordered
    /// by distance ascending. Ties break on `node_id` ascending
    /// for deterministic output.
    ///
    /// When multiple spatial indexes exist for the same label
    /// (e.g. `Place.loc` + `Place.other`) the first one sorted
    /// alphabetically by key is used. An explicit future task will
    /// extend the signature with an optional `property` argument
    /// once the planner integrates spatial seeks end-to-end.
    pub(in crate::executor) fn execute_spatial_nearest(
        &self,
        context: &mut ExecutionContext,
        arguments: &[parser::Expression],
        yield_columns: Option<&Vec<String>>,
    ) -> Result<()> {
        let point_val = match arguments.first() {
            Some(expr) => self.evaluate_expression_in_context(context, expr)?,
            None => {
                return Err(Error::CypherExecution(
                    "ERR_MISSING_ARG: spatial.nearest requires `point` at position 0".to_string(),
                ));
            }
        };
        let label = match arguments.get(1) {
            Some(expr) => match self.evaluate_expression_in_context(context, expr)? {
                Value::String(s) => s,
                other => {
                    return Err(Error::CypherExecution(format!(
                        "ERR_INVALID_ARG_TYPE: spatial.nearest `label` must be STRING (got {other})"
                    )));
                }
            },
            None => {
                return Err(Error::CypherExecution(
                    "ERR_MISSING_ARG: spatial.nearest requires `label` at position 1".to_string(),
                ));
            }
        };
        let k = match arguments.get(2) {
            Some(expr) => match self.evaluate_expression_in_context(context, expr)? {
                Value::Number(n) => n
                    .as_i64()
                    .and_then(|v| usize::try_from(v).ok())
                    .ok_or_else(|| {
                        Error::CypherExecution(
                            "ERR_INVALID_ARG_VALUE: spatial.nearest `k` must be a positive \
                             INTEGER"
                                .to_string(),
                        )
                    })?,
                other => {
                    return Err(Error::CypherExecution(format!(
                        "ERR_INVALID_ARG_TYPE: spatial.nearest `k` must be INTEGER (got {other})"
                    )));
                }
            },
            None => {
                return Err(Error::CypherExecution(
                    "ERR_MISSING_ARG: spatial.nearest requires `k` at position 2".to_string(),
                ));
            }
        };
        if !matches!(point_val, Value::Object(_)) {
            return Err(Error::CypherExecution(format!(
                "ERR_INVALID_ARG_TYPE: spatial.nearest `point` must be a POINT (got {point_val})"
            )));
        }
        let point = crate::geospatial::Point::from_json_value(&point_val).map_err(|e| {
            Error::CypherExecution(format!(
                "ERR_INVALID_ARG_TYPE: spatial.nearest `point` is not a valid POINT: {e}"
            ))
        })?;

        // Locate the `{label}.<prop>` index in the R-tree registry.
        // Sort definitions so the pick is stable when more than one
        // property is indexed for the same label.
        let registry = &self.shared.rtree_registry;
        let prefix = format!("{label}.");
        let mut matching: Vec<String> = registry
            .definitions()
            .into_iter()
            .filter(|(name, _, _)| name.starts_with(&prefix))
            .map(|(name, _, _)| name)
            .collect();
        matching.sort();
        let Some(index_key) = matching.into_iter().next() else {
            return Err(Error::CypherExecution(format!(
                "ERR_SPATIAL_INDEX_NOT_FOUND: no spatial index exists for label `{label}` — \
                 run `CREATE SPATIAL INDEX ON :{label}(<property>)` first",
            )));
        };

        // Query the packed R-tree directly through the registry.
        use crate::index::rtree::Metric as RtreeMetric;
        let pairs: Vec<(u64, f64)> = registry
            .nearest_with_filter(
                &index_key,
                point.x,
                point.y,
                k,
                RtreeMetric::Cartesian,
                |_| true,
            )
            .map_err(|e| Error::CypherExecution(format!("ERR_SPATIAL_NEAREST_FAILED: {e}")))?
            .into_iter()
            .map(|h| (h.node_id, h.distance))
            .collect();

        let columns = yield_columns
            .cloned()
            .unwrap_or_else(|| vec!["node".to_string(), "dist".to_string()]);
        let rows: Vec<Row> = pairs
            .into_iter()
            .map(|(node_id, dist)| {
                let node = serde_json::json!({ "_nexus_id": node_id });
                let d = serde_json::Number::from_f64(dist)
                    .map(Value::Number)
                    .unwrap_or(Value::Null);
                Row {
                    values: vec![node, d],
                }
            })
            .collect();
        context.set_columns_and_rows(columns, rows);
        Ok(())
    }
}
