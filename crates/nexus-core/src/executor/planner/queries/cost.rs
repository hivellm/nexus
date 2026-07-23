//! Cost estimation, cardinality modelling, cache management, and operator-order
//! optimisation.

use super::*;

impl<'a> QueryPlanner<'a> {
    /// Estimate query cost for optimization
    pub fn estimate_cost(&self, operators: &[Operator]) -> Result<f64> {
        let mut total_cost = 0.0;

        for operator in operators {
            match operator {
                Operator::NodeByLabel { label_id, .. } => {
                    // Estimate cost based on label selectivity
                    let selectivity = self.estimate_label_selectivity(*label_id)?;
                    total_cost += 1000.0 * selectivity;
                }
                Operator::NodeIndexSeek { .. } => {
                    // Index seek is a point lookup over the property B-tree —
                    // far cheaper than a label scan; bias the planner toward it.
                    total_cost += 5.0;
                }
                Operator::AllNodesScan { .. } => {
                    // Scanning all nodes is more expensive than label scan
                    // Assume full scan of all nodes
                    total_cost += 2000.0;
                }
                Operator::Filter { .. } => {
                    // Filter operations are relatively cheap
                    total_cost += 10.0;
                }
                Operator::OptionalFilter { .. } => {
                    // OptionalFilter is similar cost to Filter
                    total_cost += 10.0;
                }
                Operator::Expand { .. } => {
                    // Relationship traversal is expensive
                    total_cost += 100.0;
                }
                Operator::Project { .. } => {
                    // Projection is cheap
                    total_cost += 1.0;
                }
                Operator::Limit { count } => {
                    // Limit reduces cost
                    total_cost *= (*count as f64) / 1000.0;
                }
                Operator::Skip { .. } => {
                    // Skip is cheap — a single pass drain, no re-sort.
                    total_cost += 1.0;
                }
                Operator::Sort { .. } => {
                    // Sorting is moderately expensive
                    total_cost += 50.0;
                }
                Operator::Aggregate { .. } => {
                    // Aggregation is moderately expensive
                    total_cost += 30.0;
                }
                Operator::Union { .. } => {
                    // Union is relatively cheap
                    total_cost += 20.0;
                }
                Operator::Join { .. } => {
                    // Join is expensive
                    total_cost += 200.0;
                }
                Operator::IndexScan { .. } => {
                    // Index scan is very cheap
                    total_cost += 5.0;
                }
                Operator::CompositeBtreeSeek { .. } => {
                    // Composite B-tree seek: point or short-range lookup.
                    total_cost += 5.0;
                }
                Operator::Distinct { .. } => {
                    // Distinct is moderately expensive
                    total_cost += 40.0;
                }
                Operator::HashJoin { .. } => {
                    // Hash join operations are moderately expensive
                    total_cost += 200.0;
                }
                Operator::Create { .. } => {
                    // CREATE operations are moderately expensive
                    total_cost += 50.0;
                }
                Operator::Delete { .. } => {
                    // DELETE operations are moderately expensive
                    total_cost += 40.0;
                }
                Operator::DetachDelete { .. } => {
                    // DETACH DELETE is more expensive (deletes relationships first)
                    total_cost += 60.0;
                }
                Operator::Unwind { .. } => {
                    // UNWIND expands list into rows - moderately cheap
                    total_cost += 15.0;
                }
                Operator::VariableLengthPath { .. } => {
                    // Variable-length paths are expensive (BFS traversal)
                    total_cost += 500.0;
                }
                Operator::QuantifiedExpand { .. } => {
                    // Quantified path patterns drive BFS plus list-promoted
                    // bookkeeping per frame — a touch more than the legacy
                    // var-length operator.
                    total_cost += 600.0;
                }
                Operator::CallProcedure { .. } => {
                    // Procedure calls are moderately expensive (depends on procedure)
                    total_cost += 200.0;
                }
                Operator::LoadCsv { .. } => {
                    // CSV loading is moderately expensive (file I/O)
                    total_cost += 50.0;
                }
                Operator::CreateIndex { .. } => {
                    // Index creation is cheap (metadata operation)
                    total_cost += 1.0;
                }
                Operator::ShowDatabases => {
                    // SHOW DATABASES is cheap (metadata operation)
                    total_cost += 1.0;
                }
                Operator::CreateDatabase { .. } => {
                    // CREATE DATABASE is moderately expensive
                    total_cost += 50.0;
                }
                Operator::DropDatabase { .. } => {
                    // DROP DATABASE is moderately expensive
                    total_cost += 50.0;
                }
                Operator::AlterDatabase { .. } => {
                    // ALTER DATABASE is cheap (metadata operation)
                    total_cost += 10.0;
                }
                Operator::UseDatabase { .. } => {
                    // USE DATABASE is cheap (session operation)
                    total_cost += 1.0;
                }
                Operator::With { items, .. } => {
                    // WITH clause is cheap (projection)
                    total_cost += items.len() as f64;
                }
                Operator::CallSubquery { inner_query, .. } => {
                    // Cost = clauses inside the inner subquery × a flat
                    // per-clause budget. The recursive descent is
                    // intentionally rough — we cannot re-plan the inner
                    // here without a full QueryPlanner, and the
                    // subquery operator never participates in
                    // join-ordering decisions, so a rough additive
                    // estimate suffices.
                    total_cost += 50.0 * (inner_query.clauses.len() as f64).max(1.0);
                }
                Operator::SpatialSeek { mode, .. } => {
                    // phase6_spatial-planner-seek §3 — cost the seek
                    // as `log_b(N) + matching_entries` with `b = 127`
                    // (RTREE_MAX_FANOUT). `matching_entries` is
                    // unknown at plan time; we use a conservative
                    // selectivity estimate (5% for bounded modes,
                    // `k` itself for k-NN). The label-scan + filter
                    // alternative is cost-modelled via the
                    // surrounding NodeByLabel arm; the planner
                    // picks the cheaper plan in the rewriter.
                    let n_default: f64 = 1000.0;
                    let log_b: f64 = 1.0_f64.max(n_default.log(127.0));
                    let matching: f64 = match mode {
                        crate::executor::types::SeekMode::Nearest { k, .. } => *k as f64,
                        _ => 0.05 * n_default,
                    };
                    total_cost += log_b + matching;
                }
                Operator::EnsureNullRowIfEmpty { .. } => {
                    // Trivial branch on row count — single-digit
                    // microseconds when it fires; bounded above by
                    // a no-op when the upstream produced rows.
                    total_cost += 0.5;
                }
            }
        }

        Ok(total_cost)
    }

    /// Estimate selectivity of a label
    pub(super) fn estimate_label_selectivity(&self, label_id: u32) -> Result<f64> {
        // Try to get real statistics from the label index
        let stats = self.label_index.get_stats();
        if stats.total_nodes > 0 {
            let label_count = self.label_index.get_nodes_with_labels(&[label_id])?.len();
            return Ok(label_count as f64 / stats.total_nodes as f64);
        }

        // Fallback to a conservative estimate if statistics unavailable
        Ok(0.1) // 10% selectivity
    }

    /// Estimate the total cost of an operator plan
    /// Advanced cost estimation with cardinality and I/O modeling
    pub fn estimate_plan_cost(&self, operators: &[Operator]) -> Result<f64> {
        let mut total_cost = 0.0;
        let mut current_cardinality = 1.0; // Estimated number of rows at current point

        for operator in operators {
            let (operator_cost, output_cardinality) =
                self.estimate_operator_cost(operator, current_cardinality)?;
            total_cost += operator_cost;
            current_cardinality = output_cardinality;
        }

        Ok(total_cost)
    }

    /// Estimate cost for a single operator with cardinality modeling
    pub(super) fn estimate_operator_cost(
        &self,
        operator: &Operator,
        input_cardinality: f64,
    ) -> Result<(f64, f64)> {
        match operator {
            Operator::NodeByLabel { label_id, .. } => {
                // Get label statistics from catalog
                let label_stats = self.label_index.get_stats();
                let total_nodes = label_stats.total_nodes as f64;

                // phase6_traversal-aggregation-perf §4: the old formula used
                // `avg_nodes_per_label` — the average across *every* label —
                // for every `NodeByLabel`, so two operators scanning
                // different labels always priced identically and
                // `optimize_operator_order`'s scan-cost sort could never
                // actually tell a 10-node label apart from a 10,000-node
                // one. Use this label's own live cardinality from the
                // label bitmap instead (same source `estimate_label_selectivity`
                // already uses below). Falls back to the previous
                // average-based estimate if the bitmap lookup errs, and
                // preserves the exact prior cold-catalog behaviour
                // (0 cardinality, `total_nodes == 0`) so a cold catalog
                // sees no change at all.
                let output_cardinality = if total_nodes > 0.0 {
                    self.label_index
                        .get_nodes_with_labels(&[*label_id])
                        .map(|bitmap| bitmap.len() as f64)
                        .unwrap_or(label_stats.avg_nodes_per_label)
                } else {
                    0.0
                };

                let io_cost = output_cardinality * 10.0; // I/O cost per node read
                let cpu_cost = output_cardinality * 2.0; // CPU cost per node processing

                Ok((io_cost + cpu_cost, output_cardinality))
            }

            Operator::AllNodesScan { .. } => {
                let label_stats = self.label_index.get_stats();
                let total_nodes = label_stats.total_nodes as f64;
                let io_cost = total_nodes * 15.0; // More expensive than indexed scan
                let cpu_cost = total_nodes * 1.0;

                Ok((io_cost + cpu_cost, total_nodes))
            }

            Operator::Filter { predicate, .. } => {
                // Estimate filter selectivity based on predicate type
                // For now, use a simple heuristic since predicate is a String
                let selectivity = 0.5; // Default 50% selectivity for filters
                let output_cardinality = input_cardinality * selectivity;

                // Filter is mostly CPU-bound
                let cpu_cost = input_cardinality * 5.0; // CPU cost per row filtered

                Ok((cpu_cost, output_cardinality))
            }

            Operator::Expand {
                type_ids,
                direction,
                ..
            } => {
                // Estimate relationship expansion cost
                let rel_stats = self.estimate_relationship_stats(&Some(type_ids.clone()))?;
                let avg_relationships_per_node = rel_stats.avg_relationships_per_node;

                let output_cardinality = input_cardinality * avg_relationships_per_node;

                // Relationship traversal involves both I/O and CPU
                let io_cost = input_cardinality * 20.0; // Reading relationship data
                let cpu_cost = output_cardinality * 3.0; // Processing each relationship

                Ok((io_cost + cpu_cost, output_cardinality))
            }

            Operator::Join {
                left,
                right,
                join_type,
                ..
            } => {
                // Cardinality (expected ROW COUNT) of each side, not the
                // cost of producing it — `estimate_plan_cost` returns a
                // cost figure in abstract cost units, which is the wrong
                // category to feed into a cartesian-product / output-row
                // estimate below. Mirrors the `Union` arm's use of
                // `estimate_operator_cardinality` for the same purpose.
                let left_cardinality =
                    self.estimate_operator_cardinality(std::slice::from_ref(left.as_ref()))?;
                let right_cardinality =
                    self.estimate_operator_cardinality(std::slice::from_ref(right.as_ref()))?;

                let (join_cost, output_cardinality) = match join_type {
                    JoinType::Inner => {
                        // Estimate join selectivity (simplified)
                        let selectivity = 0.1; // Assume 10% of cartesian product
                        let cartesian = left_cardinality * right_cardinality;
                        let output_card = cartesian * selectivity;

                        // Hash join cost model
                        let build_cost = left_cardinality * 5.0; // Building hash table
                        let probe_cost = right_cardinality * 3.0; // Probing hash table

                        (build_cost + probe_cost, output_card)
                    }
                    JoinType::LeftOuter => {
                        // Left outer join preserves left side
                        let output_card = left_cardinality;
                        let cost = left_cardinality * 10.0 + right_cardinality * 5.0;
                        (cost, output_card)
                    }
                    _ => {
                        // Default to cartesian product cost
                        let output_card = left_cardinality * right_cardinality;
                        let cost = output_card * 2.0;
                        (cost, output_card)
                    }
                };

                Ok((join_cost, output_cardinality))
            }

            Operator::Union { left, right, .. } => {
                let left_cost = self.estimate_plan_cost(left)?;
                let right_cost = self.estimate_plan_cost(right)?;
                let left_card = self.estimate_operator_cardinality(left)?;
                let right_card = self.estimate_operator_cardinality(right)?;

                // Union cost is sum of both sides
                let total_cost = left_cost + right_cost;
                let output_cardinality = left_card + right_card; // Union removes duplicates conceptually

                Ok((total_cost, output_cardinality))
            }

            Operator::Project { .. } => {
                // Projection is mostly CPU-bound
                let cpu_cost = input_cardinality * 1.0;
                Ok((cpu_cost, input_cardinality)) // Cardinality unchanged
            }

            Operator::Sort { .. } => {
                // Sort cost using n*log(n) model
                let sort_cost = input_cardinality * (input_cardinality.log2()).max(1.0) * 2.0;
                Ok((sort_cost, input_cardinality))
            }

            Operator::Limit { count, .. } => {
                // Limit reduces both cost and cardinality
                let limit_cost = 1.0;
                let output_cardinality = (*count as f64).min(input_cardinality);
                Ok((limit_cost, output_cardinality))
            }

            Operator::Aggregate {
                source,
                aggregations,
                group_by,
                ..
            } => {
                let group_count = if group_by.is_empty() {
                    1.0 // No grouping
                } else {
                    // Estimate number of groups (simplified)
                    (input_cardinality * 0.1).max(1.0)
                };

                let agg_cost = input_cardinality * 3.0 + group_count * 5.0; // Processing + grouping
                Ok((agg_cost, group_count))
            }

            // Default case for unhandled operators
            _ => {
                let default_cost = input_cardinality * 10.0; // Conservative estimate
                Ok((default_cost, input_cardinality))
            }
        }
    }

    /// Estimate cardinality (number of output rows) for an operator
    pub(super) fn estimate_operator_cardinality(&self, operators: &[Operator]) -> Result<f64> {
        let mut cardinality = 1.0;
        for operator in operators {
            let (_, output_card) = self.estimate_operator_cost(operator, cardinality)?;
            cardinality = output_card;
        }
        Ok(cardinality)
    }

    /// Estimate filter selectivity based on predicate type
    pub(super) fn estimate_filter_selectivity(&self, predicate: &str) -> Result<f64> {
        // Simple heuristic based on predicate content
        if predicate.contains('=') && !predicate.contains('!') {
            // Equality filters are selective
            Ok(0.1) // 10% selectivity for equality
        } else if predicate.contains("CONTAINS") || predicate.contains("STARTS WITH") {
            // String matching is moderately selective
            Ok(0.3) // 30% selectivity
        } else if predicate.contains('>') || predicate.contains('<') {
            // Range filters have medium selectivity
            Ok(0.4) // 40% selectivity for ranges
        } else {
            // Default selectivity for complex predicates
            Ok(0.5) // 50% selectivity
        }
    }

    /// Estimate relationship traversal statistics
    ///
    /// phase6_traversal-aggregation-perf §4: when `type_filter` names one or
    /// more relationship types, scale `avg_relationships_per_node` by the
    /// catalog's real per-type relationship counters (`Catalog::get_rel_count`
    /// — increment-only, so treated as an upper bound, matching the same
    /// caveat `Catalog::get_node_count` carries) instead of the flat
    /// default. This is conservative by construction: an unfiltered Expand
    /// (`type_filter` is `None` or empty), a cold catalog (all counters
    /// still 0), or a cold label index (`total_nodes == 0`) all fall back
    /// to the exact previous hardcoded stats — no behaviour change in any
    /// of those cases.
    pub(super) fn estimate_relationship_stats(
        &self,
        type_filter: &Option<Vec<u32>>,
    ) -> Result<RelationshipTraversalStats> {
        let default_stats = RelationshipTraversalStats {
            total_relationships: 1000,
            total_nodes: 500,
            high_degree_nodes: 10,
            avg_relationships_per_node: 2.0,
            path_cache_hit_rate: 0.8,
            index_hit_rate: 0.9,
        };

        let Some(type_ids) = type_filter else {
            return Ok(default_stats);
        };
        if type_ids.is_empty() {
            return Ok(default_stats);
        }

        let mut total_matching_rels: u64 = 0;
        for type_id in type_ids {
            match self.catalog.get_rel_count(*type_id) {
                Ok(count) => total_matching_rels = total_matching_rels.saturating_add(count),
                Err(_) => return Ok(default_stats),
            }
        }
        if total_matching_rels == 0 {
            return Ok(default_stats);
        }

        let total_nodes = self.label_index.get_stats().total_nodes;
        if total_nodes == 0 {
            return Ok(default_stats);
        }

        Ok(RelationshipTraversalStats {
            total_relationships: total_matching_rels,
            avg_relationships_per_node: total_matching_rels as f64 / total_nodes as f64,
            total_nodes,
            ..default_stats
        })
    }

    /// Get query plan cache statistics
    pub fn plan_cache_stats(&self) -> &QueryPlanCacheStats {
        self.plan_cache.stats()
    }

    /// Get detailed plan reuse statistics
    pub fn plan_reuse_stats(&self) -> PlanReuseStats {
        self.plan_cache.plan_reuse_stats()
    }

    /// Get aggregation cache statistics
    pub fn aggregation_cache_stats(&self) -> AggregationCacheStats {
        self.aggregation_cache.stats()
    }

    /// Clean expired entries from both caches
    pub fn clean_expired_caches(&mut self) {
        self.plan_cache.clean_expired();
        self.aggregation_cache.clean_expired();
    }

    /// Update performance metrics with query plan cache statistics
    pub async fn update_performance_metrics(
        &self,
        metrics: &crate::performance::PerformanceMetrics,
    ) {
        let stats = self.plan_cache.stats();

        // Update counters
        metrics
            .increment_counter("query_plan_cache_lookups", stats.lookups)
            .await;
        metrics
            .increment_counter("query_plan_cache_hits", stats.hits)
            .await;
        metrics
            .increment_counter("query_plan_cache_misses", stats.misses)
            .await;
        metrics
            .increment_counter("query_plan_cache_evictions", stats.evictions)
            .await;
        metrics
            .increment_counter("query_plan_cache_expirations", stats.expirations)
            .await;

        // Update gauges
        metrics
            .set_gauge(
                "query_plan_cache_hit_rate",
                if stats.lookups > 0 {
                    stats.hits as f64 / stats.lookups as f64
                } else {
                    0.0
                },
            )
            .await;
        metrics
            .set_gauge("query_plan_cache_size", stats.cached_plans as f64)
            .await;
    }

    /// Clear query plan cache
    pub fn clear_plan_cache(&mut self) {
        self.plan_cache.clear();
    }

    /// Clean expired plans from cache
    pub fn clean_expired_plans(&mut self) {
        self.plan_cache.clean_expired();
    }

    /// Optimize operator order based on cost estimates
    pub fn optimize_operator_order(&self, operators: Vec<Operator>) -> Result<Vec<Operator>> {
        if operators.len() <= 1 {
            return Ok(operators);
        }

        // Check if there's a WITH operator followed immediately by a Filter
        // If so, keep them together (WITH WHERE pattern) and skip optimization
        for i in 0..operators.len() - 1 {
            if matches!(&operators[i], Operator::With { .. }) {
                if matches!(&operators[i + 1], Operator::Filter { .. }) {
                    tracing::debug!(
                        "Skipping operator optimization - WITH followed by Filter (WITH WHERE pattern)"
                    );
                    return Ok(operators);
                }
            }
        }

        // Check if a per-row binder (UNWIND or LOAD CSV) comes before any scan
        // in the original operator order. This happens in queries like
        // `UNWIND [...] AS x MATCH (n:Label {prop: x})` or
        // `LOAD CSV FROM '...' AS row MATCH (n:Label {id: row.id})`. In that
        // case the binder must run first to create the variable bindings the
        // scan/seek (and any residual filter) depend on.
        let mut unwind_before_scan = false;
        let mut seen_unwind = false;
        for operator in &operators {
            match operator {
                // UNWIND and LOAD CSV both bind a fresh per-row variable
                // independent of any upstream row stream, so either one
                // preceding a scan triggers the binder-before-scan order.
                Operator::Unwind { .. } | Operator::LoadCsv { .. } => {
                    seen_unwind = true;
                }
                // Index seeks bind a node variable exactly like a label scan
                // does (they generate fresh rows independent of any upstream
                // row stream), so they must be detected here too — otherwise
                // `UNWIND ... MATCH (a:P {id: r.s})` is missed and the seek
                // ends up reordered after a `Filter` that references the
                // variable it binds. See
                // phase0_fix-correlated-predicate-index-seek.
                //
                // A spatial R-tree seek (`SpatialSeek`) belongs here for the
                // same reason: it generates fresh node rows independent of
                // upstream input, exactly like a label scan.
                Operator::NodeByLabel { .. }
                | Operator::AllNodesScan { .. }
                | Operator::IndexScan { .. }
                | Operator::NodeIndexSeek { .. }
                | Operator::CompositeBtreeSeek { .. }
                | Operator::SpatialSeek { .. } => {
                    if seen_unwind {
                        unwind_before_scan = true;
                        break;
                    }
                }
                _ => {}
            }
        }

        // Separate operators into different categories
        let mut scans = Vec::new();
        let mut filters = Vec::new();
        let mut expansions = Vec::new();
        let mut joins = Vec::new();
        let mut unwinds = Vec::new();
        let mut others = Vec::new();

        // INVARIANT: every variable-BINDING operator (any operator that
        // introduces a NEW row-scoped binding for a pattern variable —
        // a node from a scan/seek, or a relationship-traversal target)
        // must land in a bucket that is recombined BEFORE `filters`
        // (`scans` or `expansions` below). A `Filter` referencing a
        // variable that has not been bound yet evaluates that variable
        // as `Null`, which is always falsy, so the predicate silently
        // drops every row instead of raising an error. The `_ => others`
        // catch-all is recombined AFTER `filters` in both branches, so it
        // must never receive a binding operator — any new binding
        // operator variant added to `Operator` must be added to `scans`,
        // `expansions`, or (for a per-row binder like UNWIND / LOAD CSV)
        // `unwinds` here, not left to fall through to `others`.
        for operator in operators {
            match &operator {
                // Index seeks and spatial seeks bind a node variable
                // exactly like a label scan — they must be ordered with
                // the scans, before any `Filter`/`Expand` that references
                // the variable they bind, or the residual predicate runs
                // against unbound input and is silently dropped. See
                // phase0_fix-correlated-predicate-index-seek.
                Operator::NodeByLabel { .. }
                | Operator::AllNodesScan { .. }
                | Operator::IndexScan { .. }
                | Operator::NodeIndexSeek { .. }
                | Operator::CompositeBtreeSeek { .. }
                | Operator::SpatialSeek { .. } => {
                    scans.push(operator);
                }
                Operator::Filter { .. } => {
                    filters.push(operator);
                }
                // `VariableLengthPath` and `QuantifiedExpand` are
                // relationship-traversal operators that consume a source
                // variable and bind a target variable, exactly like
                // `Expand`, so they must recombine before any `Filter`
                // that references the variable they bind.
                Operator::Expand { .. }
                | Operator::VariableLengthPath { .. }
                | Operator::QuantifiedExpand { .. } => {
                    expansions.push(operator);
                }
                Operator::Join { .. } => {
                    joins.push(operator);
                }
                // UNWIND and LOAD CSV both bind a fresh per-row variable and
                // must recombine before `filters` — a `Filter` referencing that
                // variable before it is bound evaluates it as `Null` (always
                // false) and silently drops every row. `LoadCsv` shares the
                // `unwinds` bucket so a `LOAD CSV`/`UNWIND` pair keeps its
                // original relative order.
                Operator::Unwind { .. } | Operator::LoadCsv { .. } => {
                    unwinds.push(operator);
                }
                _ => {
                    others.push(operator);
                }
            }
        }

        // Optimize scan order: put lowest cost scans first
        let mut scan_costs = Vec::new();
        for (i, scan) in scans.iter().enumerate() {
            let cost = self
                .estimate_plan_cost(std::slice::from_ref(scan))
                .unwrap_or(1000.0);
            scan_costs.push((cost, i));
        }
        scan_costs.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

        let mut optimized_scans = Vec::new();
        for (_, idx) in scan_costs {
            optimized_scans.push(scans[idx].clone());
        }

        // Optimize join order: put smaller joins first (simple heuristic)
        let mut optimized_joins = Vec::new();
        for join in joins {
            optimized_joins.push(join);
        }

        // Combine in optimal order based on whether UNWIND precedes scans
        let mut result = Vec::new();
        if unwind_before_scan {
            // UNWIND before scan pattern (e.g., UNWIND [...] AS x MATCH (n {prop: x}))
            // Order: unwinds -> scans -> expansions -> filters -> joins -> others
            // UNWIND must run first to create variable bindings for MATCH
            result.extend(unwinds);
            result.extend(optimized_scans);
            result.extend(expansions);
            result.extend(filters);
            result.extend(optimized_joins);
            result.extend(others);
        } else {
            // Normal order: scans -> expansions -> unwinds -> filters -> joins -> others
            // Expansions must come before filters because filters may depend on relationship variables
            // created by expansions (e.g., WHERE r.role = 'Developer')
            // UNWIND must come before filters because UNWIND creates rows that filters operate on
            result.extend(optimized_scans);
            result.extend(expansions);
            result.extend(unwinds);
            result.extend(filters);
            result.extend(optimized_joins);
            result.extend(others);
        }

        Ok(result)
    }
}
