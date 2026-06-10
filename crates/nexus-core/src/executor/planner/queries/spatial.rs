//! Spatial-seek rewriter: recognise bbox/within-distance/k-NN patterns and
//! replace `NodeByLabel + Filter` with `SpatialSeek` when a registered R-tree
//! covers the predicate.

use super::qpp::{
    extract_bbox_literal, extract_f64_literal, extract_point_literal, extract_usize_literal,
    recognise_order_by_distance,
};
use super::*;

impl<'a> QueryPlanner<'a> {
    pub(super) fn try_rewrite_spatial_seek(
        &self,
        query: &CypherQuery,
        operators: Vec<Operator>,
    ) -> Vec<Operator> {
        let Some(registry) = self.rtree_registry.as_ref() else {
            return operators;
        };
        if registry.is_empty() {
            return operators;
        }

        // Index every MATCH node by variable -> label so the WHERE /
        // ORDER BY scan below can resolve `n.prop` to a registered
        // `{Label}.{prop}` index. Multi-label patterns: we record
        // the first label only — registering against multi-label
        // matches is reserved for the cluster-mode work.
        let mut var_to_label: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        for clause in &query.clauses {
            if let Clause::Match(mc) = clause {
                for elem in &mc.pattern.elements {
                    if let crate::executor::parser::PatternElement::Node(n) = elem {
                        if let (Some(var), Some(label)) = (n.variable.as_ref(), n.labels.first()) {
                            var_to_label
                                .entry(var.clone())
                                .or_insert_with(|| label.clone());
                        }
                    }
                }
            }
        }
        if var_to_label.is_empty() {
            return operators;
        }

        // Collect candidate seeks. WHERE-shape predicates land
        // directly. ORDER BY + LIMIT pairs need both clauses to
        // resolve before we can emit a Nearest seek.
        let mut candidates: Vec<(String, String, crate::executor::types::SeekMode)> = Vec::new();
        let mut order_by_distance: Option<(String, String, f64, f64)> = None;
        let mut limit_value: Option<usize> = None;

        for clause in &query.clauses {
            match clause {
                Clause::Where(wc) => {
                    if let Some(c) =
                        self.try_recognise_where_seek(&wc.expression, &var_to_label, registry)
                    {
                        candidates.push(c);
                    }
                }
                Clause::Match(mc) => {
                    if let Some(w) = mc.where_clause.as_ref() {
                        if let Some(c) =
                            self.try_recognise_where_seek(&w.expression, &var_to_label, registry)
                        {
                            candidates.push(c);
                        }
                    }
                }
                Clause::OrderBy(ob) => {
                    if order_by_distance.is_none() {
                        order_by_distance = recognise_order_by_distance(ob);
                    }
                }
                Clause::Limit(lc) => {
                    if limit_value.is_none() {
                        limit_value = extract_usize_literal(&lc.count);
                    }
                }
                _ => {}
            }
        }

        if let (Some((var, prop, cx, cy)), Some(k)) = (order_by_distance, limit_value) {
            if let Some(label) = var_to_label.get(&var) {
                let name = format!("{label}.{prop}");
                if registry.contains(&name) {
                    candidates.push((
                        var,
                        name,
                        crate::executor::types::SeekMode::Nearest {
                            center_x: cx,
                            center_y: cy,
                            k,
                        },
                    ));
                }
            }
        }

        if candidates.is_empty() {
            return operators;
        }

        // §3 cost-based picker: only swap when the seek is cheaper
        // than the legacy `NodeByLabel + Filter` alternative. For
        // the v1 cost model the comparison is selectivity-based
        // (5% for bounded modes, k for k-NN); future slices that
        // surface real index statistics tighten the estimate.
        for (variable, index_id, mode) in candidates {
            let n = self
                .estimate_label_cardinality(&variable, &var_to_label)
                .unwrap_or(1000.0);
            let seek_cost = self.spatial_seek_cost(&mode, n);
            let scan_cost = n + n; // label scan + per-row filter
            if seek_cost >= scan_cost {
                continue;
            }
            if let Some(rewritten) =
                self.swap_in_spatial_seek(&operators, &variable, &index_id, mode)
            {
                return rewritten;
            }
        }
        operators
    }

    /// Recognise a `WHERE` expression that fits the spatial-seek
    /// shape. Returns `Some((variable, index_id, mode))` only when
    /// the expression is a top-level `point.withinBBox` or
    /// `point.withinDistance` call against a `<var>.<prop>` whose
    /// `(label, property)` registers in the R-tree.
    pub(super) fn try_recognise_where_seek(
        &self,
        expr: &Expression,
        var_to_label: &std::collections::HashMap<String, String>,
        registry: &std::sync::Arc<crate::index::rtree::RTreeRegistry>,
    ) -> Option<(String, String, crate::executor::types::SeekMode)> {
        let (name, args) = match expr {
            Expression::FunctionCall { name, args } => (name.as_str(), args),
            _ => return None,
        };
        // First arg must be `<var>.<prop>`.
        let (var, prop) = match args.first()? {
            Expression::PropertyAccess { variable, property } => {
                (variable.clone(), property.clone())
            }
            _ => return None,
        };
        let label = var_to_label.get(&var)?;
        let index_id = format!("{label}.{prop}");
        if !registry.contains(&index_id) {
            return None;
        }
        let mode = match name {
            "point.withinBBox" | "withinBBox" => {
                let (min_x, min_y, max_x, max_y) = extract_bbox_literal(args.get(1)?)?;
                crate::executor::types::SeekMode::Bbox {
                    min_x,
                    min_y,
                    max_x,
                    max_y,
                }
            }
            "point.withinDistance" | "withinDistance" => {
                let (cx, cy) = extract_point_literal(args.get(1)?)?;
                let d = extract_f64_literal(args.get(2)?)?;
                crate::executor::types::SeekMode::WithinDistance {
                    center_x: cx,
                    center_y: cy,
                    meters: d,
                }
            }
            _ => return None,
        };
        Some((var, index_id, mode))
    }

    /// Cost the spatial seek per §3: `log_b(N) + matching` with
    /// `b = 127`. `matching` is `k` for k-NN, otherwise `0.05 * N`.
    pub(super) fn spatial_seek_cost(&self, mode: &crate::executor::types::SeekMode, n: f64) -> f64 {
        let b = 127.0_f64;
        let log_b = if n > 1.0 { n.log(b).max(1.0) } else { 1.0 };
        let matching = match mode {
            crate::executor::types::SeekMode::Nearest { k, .. } => *k as f64,
            _ => 0.05 * n,
        };
        log_b + matching
    }

    /// Conservative label-cardinality estimate for the scan-vs-seek
    /// cost comparison. Reads `LabelIndex` stats when available;
    /// falls back to 1 000 (planner-default) when the catalog
    /// hasn't been queried yet.
    pub(super) fn estimate_label_cardinality(
        &self,
        variable: &str,
        var_to_label: &std::collections::HashMap<String, String>,
    ) -> Option<f64> {
        let label_name = var_to_label.get(variable)?;
        let label_id = self.catalog.get_label_id(label_name).ok()?;
        let count = self
            .label_index
            .get_nodes_with_labels(&[label_id])
            .ok()?
            .len();
        if count == 0 {
            Some(1000.0)
        } else {
            Some(count as f64)
        }
    }

    /// Replace the `NodeByLabel { variable, .. }` operator with a
    /// `SpatialSeek` carrying the same variable. Returns `None`
    /// when the pipeline has no matching `NodeByLabel` to swap.
    pub(super) fn swap_in_spatial_seek(
        &self,
        operators: &[Operator],
        variable: &str,
        index_id: &str,
        mode: crate::executor::types::SeekMode,
    ) -> Option<Vec<Operator>> {
        let mut out = Vec::with_capacity(operators.len());
        let mut swapped = false;
        for op in operators {
            if !swapped {
                if let Operator::NodeByLabel { variable: v, .. } = op {
                    if v == variable {
                        out.push(Operator::SpatialSeek {
                            index_id: index_id.to_string(),
                            variable: variable.to_string(),
                            mode: mode.clone(),
                        });
                        swapped = true;
                        continue;
                    }
                }
            }
            out.push(op.clone());
        }
        if swapped { Some(out) } else { None }
    }

    /// Diagnostic pre-pass: walk every MATCH/MERGE clause looking for
    /// node selectors of the form `(var:Label { prop: <expr> })` or
    /// WHERE equality predicates of the form `var.prop = <expr>` where
    /// `var` is bound to a node with a known label, and emit one
    /// `Nexus.Performance.UnindexedPropertyAccess` notification per
    /// distinct `(label, property)` pair that lacks a covering
    /// property index.
    ///
    /// No-op when:
    ///   - `self.property_index` is `None` (caller has no catalog
    ///     handle — typical for the standalone `Executor::parse_and_plan`
    ///     path and for planner unit tests).
    ///   - The label is not registered in the catalog (the planner
    ///     would either auto-create it later or fail; either way no
    ///     scan-vs-seek decision exists yet).
    ///   - The property name is not registered (same reasoning).
    pub(super) fn scan_unindexed_property_access(&mut self, query: &CypherQuery) {
        let Some(prop_idx) = self.property_index else {
            return;
        };
        // Delegate to the free function so MATCH/READ paths share one
        // implementation with the engine's MERGE/write path.
        let notes = compute_unindexed_property_access_notifications(self.catalog, prop_idx, query);
        self.notifications.extend(notes);
    }
}
