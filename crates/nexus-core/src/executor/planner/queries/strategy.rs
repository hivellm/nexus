//! `plan_execution_strategy`, `synthesise_anonymous_source_anchors`,
//! `select_start_pattern`, and `node_index_seek_for` — the pattern-to-operator
//! lowering pass.

use super::*;

impl<'a> QueryPlanner<'a> {
    /// Plan execution strategy based on patterns and constraints
    #[allow(clippy::too_many_arguments)]
    pub(super) fn plan_execution_strategy(
        &self,
        patterns: &[(Pattern, bool)], // (Pattern, is_optional)
        where_clauses: &[(Expression, Vec<String>)], // (expression, optional_vars)
        return_items: &[ReturnItem],
        limit_count: Option<usize>,
        distinct: bool,
        unwind_operators: &[Operator],
        unwind_before_match: bool,
        hints: &[QueryHint],
        order_by_clause: &Option<(Vec<String>, Vec<bool>)>,
        with_aggregation_where: &Option<Expression>, // WHERE from WITH with aggregation
        operators: &mut Vec<Operator>,
    ) -> Result<()> {
        // CRITICAL: Insert UNWIND operators FIRST when they precede MATCH in the query
        // This handles queries like: UNWIND [...] AS x MATCH (p:Person {name: x})
        // UNWIND must run before NodeByLabel so the variable is bound for property filtering
        // Note: This differs from MATCH ... UNWIND ... where UNWIND expands rows from MATCH
        if unwind_before_match && !unwind_operators.is_empty() {
            for op in unwind_operators {
                operators.push(op.clone());
            }
        }

        // Synthesise variables for anonymous source anchors that carry
        // label or property filters (phase6 §1/§2). Without this, a pattern
        // like `MATCH (:P {id: 0})-[:KNOWS]->(b)` leaves the Expand's
        // source_var empty, so `execute_expand` takes its source-less
        // fallback and scans every KNOWS edge in the store — returning
        // every KNOWS edge instead of only the anchor's outgoing edges.
        // Synthesising a variable here lets the NodeByLabel + property
        // Filter path below constrain the source set correctly, and the
        // Expand uses that variable as its source.
        let mut patterns_local: Vec<(Pattern, bool)> = patterns.to_vec();
        let mut anchor_counter: usize = 0;
        for (pattern, _) in patterns_local.iter_mut() {
            Self::synthesise_anonymous_source_anchors(pattern, &mut anchor_counter);
        }

        // WHERE-form equality index-seek lift: a plan-time-constant
        // equality on a just-matched node variable (`MATCH (n:Person)
        // WHERE n.age = 30`) should seek the same way the inline-property
        // form does (`MATCH (n:Person {age: 30})`) instead of always
        // falling back to `NodeByLabel` + `Filter`. Each WHERE-clause
        // entry is decomposed into its top-level AND-conjuncts here so a
        // qualifying conjunct can be lifted out (by
        // `where_equality_index_seek_for`, called from the node loops
        // below, at the exact site that would otherwise emit
        // `NodeByLabel`) while any OTHER conjunct on the same WHERE
        // clause survives as a residual `Filter`. Rebuilt back into
        // `Expression` form (via `rebuild_and_conjunction`) after both
        // node loops have had a chance to consume conjuncts, then used
        // in place of `where_clauses` for the Filter/OptionalFilter
        // lowering pass below. Scope: EQUALITY ONLY on a single-property
        // index — range/IN/STARTS WITH/CONTAINS predicates are never
        // lifted here (see `where_equality_seek_operand`'s doc comment);
        // they stay full scans, made observable via the
        // `Nexus.Performance.UnindexedPropertyAccess` notification.
        let mut residual_where: Vec<(Vec<Expression>, Vec<String>)> = where_clauses
            .iter()
            .map(|(expr, opt_vars)| {
                let mut conjuncts = Vec::new();
                Self::flatten_and_conjuncts(expr, &mut conjuncts);
                (conjuncts, opt_vars.clone())
            })
            .collect();

        // Process ALL patterns, not just the first one
        // Multiple patterns need Cartesian product (Join)
        let mut all_target_nodes = std::collections::HashSet::new();

        // Identify target nodes across all patterns
        // CRITICAL FIX: Include ALL nodes that are targets of relationships (Expand),
        // not just nodes without labels. Nodes that are targets of Expand will be populated
        // by the Expand operator and don't need a separate NodeByLabel.
        for (pattern, _is_optional) in &patterns_local {
            for (idx, element) in pattern.elements.iter().enumerate() {
                if let PatternElement::Relationship(_) = element {
                    if idx + 1 < pattern.elements.len() {
                        if let PatternElement::Node(node) = &pattern.elements[idx + 1] {
                            if let Some(var) = &node.variable {
                                // CRITICAL: Add ALL target nodes, regardless of labels
                                // Nodes that are targets of Expand will be populated by Expand,
                                // so we shouldn't create NodeByLabel for them
                                all_target_nodes.insert(var.clone());
                            }
                        }
                    }
                }
            }
        }

        // Process the first pattern (extract pattern from tuple)
        let patterns_only: Vec<Pattern> = patterns_local.iter().map(|(p, _)| p.clone()).collect();
        let start_pattern = self.select_start_pattern(&patterns_only)?;

        // Add NodeByLabel operators for nodes in first pattern
        // CRITICAL FIX: For cyclic patterns (e.g., (a)->(b)->(c)->(a)),
        // the first node 'a' is BOTH a source AND a target. We need to identify
        // the first node and ALWAYS create NodeByLabel for it, even if it's a target.
        let first_node_var: Option<String> = start_pattern.elements.iter().find_map(|el| {
            if let PatternElement::Node(node) = el {
                node.variable.clone()
            } else {
                None
            }
        });

        for (idx, element) in start_pattern.elements.iter().enumerate() {
            if let PatternElement::Node(node) = element {
                if let Some(variable) = &node.variable {
                    // CRITICAL: Check if this is the first node in the pattern
                    let is_first_node = Some(variable.clone()) == first_node_var;

                    // Skip if this node is a pure target without labels (will be populated by Expand)
                    // EXCEPTION: Always create NodeByLabel for the first node, even in cyclic patterns
                    if !is_first_node && all_target_nodes.contains(variable) {
                        continue;
                    }

                    // Check for hints for this variable
                    let use_index_hint = hints.iter().find(|h| {
                        if let QueryHint::UsingIndex {
                            variable: hint_var, ..
                        } = h
                        {
                            hint_var == variable
                        } else {
                            false
                        }
                    });

                    let use_scan_hint = hints.iter().find(|h| {
                        if let QueryHint::UsingScan {
                            variable: hint_var, ..
                        } = h
                        {
                            hint_var == variable
                        } else {
                            false
                        }
                    });

                    if !node.labels.is_empty() {
                        // Use first label for initial scan
                        let first_label = &node.labels[0];
                        let label_id = self.catalog.get_or_create_label(first_label)?;

                        // Apply USING INDEX hint if present.
                        //
                        // phase7_planner-using-index-hints §1.5: when a
                        // `PropertyIndex` handle is installed
                        // (`with_property_index`), the planner verifies
                        // that the hinted `(label, property)` pair has a
                        // registered index and raises a structured
                        // `ERR_USING_INDEX_NOT_FOUND` when it doesn't.
                        // Without a handle the hint is accepted silently
                        // — that's the legacy behaviour of unit-test
                        // callers that don't construct an
                        // `IndexManager`.
                        if let Some(QueryHint::UsingIndex {
                            label: hint_label,
                            property: hint_property,
                            ..
                        }) = use_index_hint
                        {
                            if let Some(prop_idx) = self.property_index {
                                // Verify the (label, property) pair has
                                // a registered single-property index.
                                let label_id_for_check = self.catalog.get_label_id(hint_label).map_err(|_| {
                                    Error::CypherSyntax(format!(
                                        "ERR_USING_INDEX_NOT_FOUND: label `:{hint_label}` referenced by USING INDEX hint is not registered"
                                    ))
                                })?;
                                let key_id_for_check = self.catalog.get_key_id(hint_property).map_err(|_| {
                                    Error::CypherSyntax(format!(
                                        "ERR_USING_INDEX_NOT_FOUND: property `{hint_property}` referenced by USING INDEX hint on `:{hint_label}` is not registered"
                                    ))
                                })?;
                                if !prop_idx.has_index(label_id_for_check, key_id_for_check) {
                                    return Err(Error::CypherSyntax(format!(
                                        "ERR_USING_INDEX_NOT_FOUND: no property index registered for `:{hint_label}({hint_property})` (USING INDEX hint requires a matching CREATE INDEX)"
                                    )));
                                }
                            }
                            // Force index usage for this property
                            // The executor will use property index lookup instead of label scan
                            operators.push(Operator::NodeByLabel {
                                label_id,
                                variable: variable.clone(),
                            });
                            // Add filter to use index (executor will detect property filter and use index)
                        } else if use_scan_hint.is_some() {
                            // USING SCAN hint - force label scan (already using NodeByLabel)
                            operators.push(Operator::NodeByLabel {
                                label_id,
                                variable: variable.clone(),
                            });
                        } else {
                            // Normal planning — prefer an index seek when a
                            // covering property index exists, else label scan.
                            // A composite index covering the FULL inline
                            // property-map key set takes precedence over a
                            // single-property seek (one seek narrows to the
                            // exact tuple instead of leaving a residual
                            // Filter on the other predicate(s)); inline
                            // property equality (`{prop: value}`) on a
                            // single-property index is tried next; a
                            // WHERE-form equality conjunct (`WHERE n.prop =
                            // value`) on an indexed property is lifted into
                            // the same seek shape when neither inline seek
                            // applies.
                            if let Some(seek) =
                                self.composite_index_seek_for(node, label_id, first_label, variable)
                            {
                                operators.push(seek);
                            } else if let Some(seek) =
                                self.node_index_seek_for(node, label_id, variable)
                            {
                                operators.push(seek);
                            } else if let Some(seek) = self.where_equality_index_seek_for(
                                variable,
                                label_id,
                                &mut residual_where,
                            ) {
                                operators.push(seek);
                            } else {
                                operators.push(Operator::NodeByLabel {
                                    label_id,
                                    variable: variable.clone(),
                                });
                            }
                        }

                        // Add filters for additional labels (multiple label intersection)
                        if node.labels.len() > 1 {
                            for additional_label in &node.labels[1..] {
                                // Create a filter that checks if node has this label
                                let filter_expr = format!("{}:{}", variable, additional_label);
                                operators.push(Operator::Filter {
                                    predicate: filter_expr,
                                });
                            }
                        }
                    } else {
                        // No label specified - need to scan all nodes
                        // Use AllNodesScan operator to scan all nodes efficiently
                        operators.push(Operator::AllNodesScan {
                            variable: variable.clone(),
                        });
                    }

                    // Add filters for inline properties: MATCH (n {property: value})
                    if let Some(property_map) = &node.properties {
                        for (prop_name, prop_value_expr) in &property_map.properties {
                            // Convert property value expression to string for filter
                            // Use single quotes for strings to match Cypher parser expectations
                            let value_str = match prop_value_expr {
                                Expression::Literal(lit) => match lit {
                                    Literal::String(s) => format!("'{}'", s),
                                    Literal::Integer(i) => i.to_string(),
                                    Literal::Float(f) => f.to_string(),
                                    Literal::Boolean(b) => b.to_string(),
                                    Literal::Null => "null".to_string(),
                                    Literal::Point(p) => p.to_string(),
                                },
                                _ => self.expression_to_string(prop_value_expr)?,
                            };
                            let filter_expr = format!("{}.{} = {}", variable, prop_name, value_str);
                            operators.push(Operator::Filter {
                                predicate: filter_expr,
                            });
                        }
                    }
                }
            }
        }

        // Add relationship traversal operators for first pattern
        let first_is_optional = patterns_local.first().map(|(_, opt)| *opt).unwrap_or(false);

        // phase8_optional-match-empty-driver: when the very first
        // clause of the query is OPTIONAL MATCH and no prior driver
        // (UNWIND, prior MATCH, etc.) feeds the pipeline, the
        // OPTIONAL contract demands one row with the optional vars
        // bound to NULL even when the scan produces no matches.
        // Inject `EnsureNullRowIfEmpty` after the scan so the
        // executor's downstream `Project` / `OptionalFilter` /
        // aggregation operators see a non-empty row set.
        //
        // Trigger conditions:
        //   1. `first_is_optional == true`
        //   2. The pipeline going into `plan_execution_strategy`
        //      had no prior driver — `unwind_before_match` is
        //      false AND `operators` was empty before this fn ran.
        //      We approximate the latter by checking whether any
        //      operators have been pushed before this point: at
        //      this point `operators` contains only this pattern's
        //      `NodeByLabel`/`AllNodesScan`/`Filter` chain plus
        //      any optional `unwind_operators` from §1232. We
        //      capture the pre-pattern operator count by stashing
        //      it before the per-element loop above (see
        //      `pre_pattern_op_count` below) and gate on
        //      `unwind_before_match == false &&
        //      pre_pattern_op_count == 0`.
        let inject_optional_null_fallback = first_is_optional
            && !unwind_before_match
            && operators.iter().all(|op| {
                // The only operators allowed here are the ones we
                // just pushed for the first OPTIONAL pattern. A
                // prior driver (e.g. a previous WITH, UNWIND, or
                // CREATE) would have pushed something else; bail
                // out conservatively if we see anything that
                // isn't a NodeByLabel / AllNodesScan / Filter.
                matches!(
                    op,
                    Operator::NodeByLabel { .. }
                        | Operator::AllNodesScan { .. }
                        | Operator::Filter { .. }
                )
            });

        if inject_optional_null_fallback {
            // Collect the variables the first OPTIONAL pattern
            // introduced so the fallback knows which slots to
            // bind to NULL. Limit to node variables — relationship
            // variables on the first pattern require the Expand
            // operator, and a relationship-only first OPTIONAL
            // does not match the standalone "OPTIONAL MATCH (n)"
            // shape this fix targets.
            let mut vars: Vec<String> = Vec::new();
            for el in &start_pattern.elements {
                if let PatternElement::Node(node) = el
                    && let Some(v) = &node.variable
                    && !vars.contains(v)
                {
                    vars.push(v.clone());
                }
            }
            if !vars.is_empty() {
                operators.push(Operator::EnsureNullRowIfEmpty { vars });
            }
        }

        self.add_relationship_operators(
            std::slice::from_ref(start_pattern),
            first_is_optional,
            operators,
            &std::collections::HashSet::new(), // No previously bound vars for first pattern
        )?;

        // Track variables bound by the first pattern (for OPTIONAL MATCH handling)
        let mut previously_bound_vars: std::collections::HashSet<String> =
            std::collections::HashSet::new();
        for element in &start_pattern.elements {
            if let PatternElement::Node(node) = element {
                if let Some(var) = &node.variable {
                    previously_bound_vars.insert(var.clone());
                }
            }
        }

        // Process additional patterns (for comma-separated MATCH patterns like (p1:...), (p2:...))
        // Each additional pattern needs its own NodeByLabel + Filter operators
        for (pattern_idx, (pattern, is_optional)) in patterns_local.iter().enumerate() {
            if pattern_idx == 0 {
                continue; // Skip first pattern, already processed
            }

            // For OPTIONAL MATCH patterns (index > 0), we need LEFT OUTER JOIN semantics
            // This will be handled by wrapping the pattern operators in a way that preserves NULL values
            let _is_optional_pattern = *is_optional;

            // CRITICAL FIX: For OPTIONAL MATCH patterns, if ANY node variable is already bound
            // from a previous pattern, we should NOT add NodeByLabel for unbound nodes.
            // The Expand operator will handle finding the unbound nodes via relationship traversal.
            let pattern_has_bound_var = pattern.elements.iter().any(|el| {
                if let PatternElement::Node(node) = el {
                    node.variable
                        .as_ref()
                        .map_or(false, |v| previously_bound_vars.contains(v))
                } else {
                    false
                }
            });

            // Add NodeByLabel operators for nodes in this additional pattern
            for element in &pattern.elements {
                if let PatternElement::Node(node) = element {
                    if let Some(variable) = &node.variable {
                        if all_target_nodes.contains(variable) {
                            continue;
                        }

                        // Skip NodeByLabel for unbound vars in OPTIONAL MATCH if pattern has a bound var
                        if *is_optional
                            && pattern_has_bound_var
                            && !previously_bound_vars.contains(variable)
                        {
                            continue;
                        }

                        if !node.labels.is_empty() {
                            let first_label = &node.labels[0];
                            let label_id = self.catalog.get_or_create_label(first_label)?;
                            if let Some(seek) =
                                self.composite_index_seek_for(node, label_id, first_label, variable)
                            {
                                operators.push(seek);
                            } else if let Some(seek) =
                                self.node_index_seek_for(node, label_id, variable)
                            {
                                operators.push(seek);
                            } else if let Some(seek) = self.where_equality_index_seek_for(
                                variable,
                                label_id,
                                &mut residual_where,
                            ) {
                                operators.push(seek);
                            } else {
                                operators.push(Operator::NodeByLabel {
                                    label_id,
                                    variable: variable.clone(),
                                });
                            }

                            // Add filters for additional labels
                            if node.labels.len() > 1 {
                                for additional_label in &node.labels[1..] {
                                    let filter_expr = format!("{}:{}", variable, additional_label);
                                    operators.push(Operator::Filter {
                                        predicate: filter_expr,
                                    });
                                }
                            }
                        }

                        // Add filters for inline properties
                        if let Some(property_map) = &node.properties {
                            for (prop_name, prop_value_expr) in &property_map.properties {
                                // Use single quotes for strings to match Cypher parser expectations
                                let value_str = match prop_value_expr {
                                    Expression::Literal(lit) => match lit {
                                        Literal::String(s) => format!("'{}'", s),
                                        Literal::Integer(i) => i.to_string(),
                                        Literal::Float(f) => f.to_string(),
                                        Literal::Boolean(b) => b.to_string(),
                                        Literal::Null => "null".to_string(),
                                        Literal::Point(p) => p.to_string(),
                                    },
                                    _ => self.expression_to_string(prop_value_expr)?,
                                };
                                let filter_expr =
                                    format!("{}.{} = {}", variable, prop_name, value_str);
                                operators.push(Operator::Filter {
                                    predicate: filter_expr,
                                });
                            }
                        }
                    }
                }
            }

            // Add relationship operators for this pattern if any
            self.add_relationship_operators(
                std::slice::from_ref(pattern),
                *is_optional,
                operators,
                &previously_bound_vars,
            )?;
        }

        // Add filter operators for WHERE clauses. Rebuild each entry's
        // surviving conjuncts (some may have been lifted into a
        // `NodeIndexSeek` by the node loops above, via
        // `where_equality_index_seek_for`) back into `Expression` form;
        // an entry that lost every conjunct to a seek is dropped
        // entirely rather than emitting an empty/always-true Filter.
        let residual_where_clauses: Vec<(Expression, Vec<String>)> = residual_where
            .into_iter()
            .filter_map(|(conjuncts, opt_vars)| {
                Self::rebuild_and_conjunction(conjuncts).map(|expr| (expr, opt_vars))
            })
            .collect();
        tracing::debug!(
            "PLANNER: Adding {} WHERE clauses as Filter/OptionalFilter operators",
            residual_where_clauses.len()
        );
        for (idx, (where_clause, optional_vars)) in residual_where_clauses.iter().enumerate() {
            let predicate = self.predicate_to_string(where_clause)?;
            if optional_vars.is_empty() {
                tracing::debug!("  WHERE clause #{}: {} (regular Filter)", idx, predicate);
                operators.push(Operator::Filter { predicate });
            } else {
                tracing::debug!(
                    "  WHERE clause #{}: {} (OptionalFilter, vars={:?})",
                    idx,
                    predicate,
                    optional_vars
                );
                operators.push(Operator::OptionalFilter {
                    predicate,
                    optional_vars: optional_vars.clone(),
                });
            }
        }

        // Capture order_by_clause reference before entering nested blocks to ensure it's accessible
        let order_by_clause_ref = order_by_clause.as_ref();

        // Add projection or aggregation operator for RETURN clause
        if !return_items.is_empty() {
            // Check if any return items contain aggregate functions
            let mut has_aggregation = false;
            let mut aggregations = Vec::new();
            let mut group_by_columns = Vec::new();

            let mut non_aggregate_aliases: Vec<String> = Vec::new();
            // Initialize projection_items early so we can add literal projections for aggregations
            let mut projection_items: Vec<ProjectionItem> = Vec::new();

            for item in return_items.iter() {
                // First, check if this expression contains any nested aggregations
                if self.contains_aggregation(&item.expression) {
                    has_aggregation = true;
                }

                match &item.expression {
                    Expression::FunctionCall { name, args } => {
                        let func_name = name.to_lowercase();
                        match func_name.as_str() {
                            "count" => {
                                has_aggregation = true;

                                // Check for DISTINCT marker
                                let mut distinct = false;
                                let mut real_args = args.clone();
                                if let Some(Expression::Variable(var)) = args.first() {
                                    if var == "__DISTINCT__" {
                                        distinct = true;
                                        real_args = args[1..].to_vec();
                                    }
                                }

                                let column = if real_args.is_empty() {
                                    None // COUNT(*) or COUNT(DISTINCT *)
                                } else if let Some(Expression::Variable(var)) = real_args.first() {
                                    Some(var.clone())
                                } else if let Some(Expression::PropertyAccess {
                                    variable,
                                    property,
                                }) = real_args.first()
                                {
                                    Some(format!("{}.{}", variable, property))
                                } else {
                                    None
                                };

                                aggregations.push(Aggregation::Count {
                                    column,
                                    alias: item
                                        .alias
                                        .clone()
                                        .unwrap_or_else(|| "count".to_string()),
                                    distinct,
                                });
                            }
                            "sum" => {
                                has_aggregation = true;
                                if let Some(arg) = args.first() {
                                    // Handle literals by projecting them first
                                    let column = match arg {
                                        Expression::Variable(var) => var.clone(),
                                        Expression::PropertyAccess { variable, property } => {
                                            format!("{}.{}", variable, property)
                                        }
                                        Expression::Literal(_) => {
                                            // For literals, create a projection item first
                                            let alias = format!("__sum_arg_{}", aggregations.len());
                                            projection_items.push(ProjectionItem {
                                                alias: alias.clone(),
                                                expression: arg.clone(),
                                            });
                                            alias
                                        }
                                        _ => continue,
                                    };
                                    aggregations.push(Aggregation::Sum {
                                        column,
                                        alias: item
                                            .alias
                                            .clone()
                                            .unwrap_or_else(|| "sum".to_string()),
                                    });
                                }
                            }
                            "avg" => {
                                has_aggregation = true;
                                if let Some(arg) = args.first() {
                                    // Handle literals by projecting them first
                                    let column = match arg {
                                        Expression::Variable(var) => var.clone(),
                                        Expression::PropertyAccess { variable, property } => {
                                            format!("{}.{}", variable, property)
                                        }
                                        Expression::Literal(_) => {
                                            // For literals, create a projection item first
                                            let alias = format!("__avg_arg_{}", aggregations.len());
                                            projection_items.push(ProjectionItem {
                                                alias: alias.clone(),
                                                expression: arg.clone(),
                                            });
                                            alias
                                        }
                                        _ => continue,
                                    };
                                    aggregations.push(Aggregation::Avg {
                                        column,
                                        alias: item
                                            .alias
                                            .clone()
                                            .unwrap_or_else(|| "avg".to_string()),
                                    });
                                }
                            }
                            "min" => {
                                has_aggregation = true;
                                if let Some(arg) = args.first() {
                                    // Handle literals by projecting them first
                                    let column = match arg {
                                        Expression::Variable(var) => var.clone(),
                                        Expression::PropertyAccess { variable, property } => {
                                            format!("{}.{}", variable, property)
                                        }
                                        Expression::Literal(_) => {
                                            // For literals, create a projection item first
                                            let alias = format!("__min_arg_{}", aggregations.len());
                                            projection_items.push(ProjectionItem {
                                                alias: alias.clone(),
                                                expression: arg.clone(),
                                            });
                                            alias
                                        }
                                        _ => continue,
                                    };
                                    aggregations.push(Aggregation::Min {
                                        column,
                                        alias: item
                                            .alias
                                            .clone()
                                            .unwrap_or_else(|| "min".to_string()),
                                    });
                                }
                            }
                            "max" => {
                                has_aggregation = true;
                                if let Some(arg) = args.first() {
                                    // Handle literals by projecting them first
                                    let column = match arg {
                                        Expression::Variable(var) => var.clone(),
                                        Expression::PropertyAccess { variable, property } => {
                                            format!("{}.{}", variable, property)
                                        }
                                        Expression::Literal(_) => {
                                            // For literals, create a projection item first
                                            let alias = format!("__max_arg_{}", aggregations.len());
                                            projection_items.push(ProjectionItem {
                                                alias: alias.clone(),
                                                expression: arg.clone(),
                                            });
                                            alias
                                        }
                                        _ => continue,
                                    };
                                    aggregations.push(Aggregation::Max {
                                        column,
                                        alias: item
                                            .alias
                                            .clone()
                                            .unwrap_or_else(|| "max".to_string()),
                                    });
                                }
                            }
                            "collect" => {
                                has_aggregation = true;
                                let distinct = args.first().is_some_and(|arg| {
                                    if let Expression::Variable(v) = arg {
                                        v == "__DISTINCT__"
                                    } else {
                                        false
                                    }
                                });

                                // Get the actual argument (skip __DISTINCT__ if present)
                                let actual_arg = if distinct && args.len() > 1 {
                                    Some(&args[1])
                                } else if !distinct && !args.is_empty() {
                                    Some(&args[0])
                                } else {
                                    None
                                };

                                if let Some(arg) = actual_arg {
                                    // Handle expressions by projecting them first
                                    let column = match arg {
                                        Expression::Variable(var) => var.clone(),
                                        Expression::PropertyAccess { variable, property } => {
                                            format!("{}.{}", variable, property)
                                        }
                                        // For any other expression (including BinaryOp like x * 2),
                                        // create a projection item first so the expression is evaluated
                                        // before collect aggregates the results
                                        _ => {
                                            let alias =
                                                format!("__collect_arg_{}", aggregations.len());
                                            projection_items.push(ProjectionItem {
                                                alias: alias.clone(),
                                                expression: arg.clone(),
                                            });
                                            alias
                                        }
                                    };
                                    aggregations.push(Aggregation::Collect {
                                        column,
                                        alias: item
                                            .alias
                                            .clone()
                                            .unwrap_or_else(|| "collect".to_string()),
                                        distinct,
                                    });
                                }
                            }
                            // phase6 §9 — statistical aggregations on the MATCH+RETURN
                            // path. Same shape as min/max/avg above; without them the
                            // planner's `_ =>` arm dropped into the nested-aggregation
                            // probe and emitted a scalar projection that returned zero
                            // rows, so `MATCH (n:A) RETURN stdev(n.score)` yielded
                            // nothing instead of one aggregated row.
                            "stdev" => {
                                has_aggregation = true;
                                if let Some(arg) = args.first() {
                                    let column = match arg {
                                        Expression::Variable(var) => var.clone(),
                                        Expression::PropertyAccess { variable, property } => {
                                            format!("{}.{}", variable, property)
                                        }
                                        _ => continue,
                                    };
                                    aggregations.push(Aggregation::StDev {
                                        column,
                                        alias: item
                                            .alias
                                            .clone()
                                            .unwrap_or_else(|| "stdev".to_string()),
                                    });
                                }
                            }
                            "stdevp" => {
                                has_aggregation = true;
                                if let Some(arg) = args.first() {
                                    let column = match arg {
                                        Expression::Variable(var) => var.clone(),
                                        Expression::PropertyAccess { variable, property } => {
                                            format!("{}.{}", variable, property)
                                        }
                                        _ => continue,
                                    };
                                    aggregations.push(Aggregation::StDevP {
                                        column,
                                        alias: item
                                            .alias
                                            .clone()
                                            .unwrap_or_else(|| "stdevp".to_string()),
                                    });
                                }
                            }
                            "percentilecont" => {
                                has_aggregation = true;
                                if args.len() >= 2 {
                                    let column = match &args[0] {
                                        Expression::Variable(var) => var.clone(),
                                        Expression::PropertyAccess { variable, property } => {
                                            format!("{}.{}", variable, property)
                                        }
                                        _ => continue,
                                    };
                                    let percentile = match &args[1] {
                                        Expression::Literal(Literal::Float(f)) => *f,
                                        Expression::Literal(Literal::Integer(i)) => *i as f64,
                                        _ => continue,
                                    };
                                    aggregations.push(Aggregation::PercentileCont {
                                        column,
                                        alias: item
                                            .alias
                                            .clone()
                                            .unwrap_or_else(|| "percentileCont".to_string()),
                                        percentile,
                                    });
                                }
                            }
                            "percentiledisc" => {
                                has_aggregation = true;
                                if args.len() >= 2 {
                                    let column = match &args[0] {
                                        Expression::Variable(var) => var.clone(),
                                        Expression::PropertyAccess { variable, property } => {
                                            format!("{}.{}", variable, property)
                                        }
                                        _ => continue,
                                    };
                                    let percentile = match &args[1] {
                                        Expression::Literal(Literal::Float(f)) => *f,
                                        Expression::Literal(Literal::Integer(i)) => *i as f64,
                                        _ => continue,
                                    };
                                    aggregations.push(Aggregation::PercentileDisc {
                                        column,
                                        alias: item
                                            .alias
                                            .clone()
                                            .unwrap_or_else(|| "percentileDisc".to_string()),
                                        percentile,
                                    });
                                }
                            }
                            _ => {
                                // Not an aggregate function, but might contain nested aggregations
                                // Check if any argument contains an aggregation
                                let mut has_nested_agg = false;
                                let mut temp_agg_alias: Option<String> = None;

                                for arg in args {
                                    if self.contains_aggregation(&arg) {
                                        has_nested_agg = true;
                                        // Extract nested aggregation (e.g., collect() inside head())
                                        if let Expression::FunctionCall {
                                            name: nested_name,
                                            args: nested_args,
                                        } = arg
                                        {
                                            let nested_func = nested_name.to_lowercase();
                                            if nested_func == "collect" {
                                                let distinct =
                                                    nested_args.first().is_some_and(|arg| {
                                                        if let Expression::Variable(v) = arg {
                                                            v == "__DISTINCT__"
                                                        } else {
                                                            false
                                                        }
                                                    });

                                                let actual_arg =
                                                    if distinct && nested_args.len() > 1 {
                                                        Some(&nested_args[1])
                                                    } else if !distinct && !nested_args.is_empty() {
                                                        Some(&nested_args[0])
                                                    } else {
                                                        None
                                                    };

                                                if let Some(arg) = actual_arg {
                                                    let column = match arg {
                                                        Expression::Variable(var) => var.clone(),
                                                        Expression::PropertyAccess {
                                                            variable,
                                                            property,
                                                        } => {
                                                            format!("{}.{}", variable, property)
                                                        }
                                                        Expression::Literal(_) => {
                                                            let alias = format!(
                                                                "__collect_arg_{}",
                                                                aggregations.len()
                                                            );
                                                            projection_items.push(ProjectionItem {
                                                                alias: alias.clone(),
                                                                expression: arg.clone(),
                                                            });
                                                            alias
                                                        }
                                                        _ => continue,
                                                    };
                                                    // Create temporary alias for the aggregation result
                                                    let temp_alias =
                                                        format!("__agg_{}", aggregations.len());
                                                    temp_agg_alias = Some(temp_alias.clone());
                                                    aggregations.push(Aggregation::Collect {
                                                        column,
                                                        alias: temp_alias,
                                                        distinct,
                                                    });
                                                }
                                            }
                                        }
                                    }
                                }

                                if has_nested_agg {
                                    // Don't add to non_aggregate_aliases - we'll handle this in post-aggregation projection
                                    // The nested aggregation will be extracted and handled separately
                                } else {
                                    // Not an aggregate function and no nested aggregations, treat as regular column for GROUP BY
                                    let alias = item.alias.clone().unwrap_or_else(|| {
                                        self.expression_to_string(&item.expression)
                                            .unwrap_or_default()
                                    });
                                    non_aggregate_aliases.push(alias);
                                }
                            }
                        }
                    }
                    _ => {
                        // Non-aggregate expression, add to GROUP BY if there are aggregations
                        let alias = item.alias.clone().unwrap_or_else(|| {
                            self.expression_to_string(&item.expression)
                                .unwrap_or_default()
                        });
                        non_aggregate_aliases.push(alias);
                    }
                }
            }

            if has_aggregation {
                let mut required_columns: HashSet<String> = HashSet::new();

                if group_by_columns.is_empty() {
                    group_by_columns = non_aggregate_aliases.clone();
                } else {
                    for alias in &non_aggregate_aliases {
                        if !group_by_columns.contains(alias) {
                            group_by_columns.push(alias.clone());
                        }
                    }
                }

                // CRITICAL FIX: Add projection items for all GROUP BY columns
                // This ensures that Project operator creates columns with correct aliases
                // before Aggregate tries to group by them
                for col in &group_by_columns {
                    // Check if this column is already in projection_items
                    if !projection_items.iter().any(|item| item.alias == *col) {
                        // Try to find the corresponding return item to get the expression
                        let mut found = false;
                        for item in return_items {
                            let alias = item.alias.clone().unwrap_or_else(|| {
                                self.expression_to_string(&item.expression)
                                    .unwrap_or_default()
                            });
                            if alias == *col {
                                // Found the matching return item, add it to projection_items
                                projection_items.push(ProjectionItem {
                                    alias: col.clone(),
                                    expression: item.expression.clone(),
                                });
                                found = true;
                                break;
                            }
                        }
                        // If not found in return_items, create a projection item from the column name
                        if !found {
                            let expression = if col.contains('.') {
                                let parts: Vec<&str> = col.split('.').collect();
                                if parts.len() == 2 {
                                    Expression::PropertyAccess {
                                        variable: parts[0].to_string(),
                                        property: parts[1].to_string(),
                                    }
                                } else {
                                    Expression::Variable(col.clone())
                                }
                            } else {
                                Expression::Variable(col.clone())
                            };
                            projection_items.push(ProjectionItem {
                                alias: col.clone(),
                                expression,
                            });
                        }
                    }
                }

                for item in return_items {
                    match &item.expression {
                        Expression::FunctionCall { name, args } => {
                            let func_name = name.to_lowercase();
                            match func_name.as_str() {
                                // phase6 §9 — statistical aggregations belong in the
                                // same required_columns tracking as count/sum/avg so
                                // Project retains the referenced column for Aggregate
                                // to consume.
                                "count" | "sum" | "avg" | "min" | "max" | "collect" | "stdev"
                                | "stdevp" | "percentilecont" | "percentiledisc" => {
                                    // Skip DISTINCT marker if present
                                    let real_args =
                                        if let Some(Expression::Variable(var)) = args.first() {
                                            if var == "__DISTINCT__" {
                                                &args[1..]
                                            } else {
                                                args.as_slice()
                                            }
                                        } else {
                                            args.as_slice()
                                        };

                                    if let Some(arg) = real_args.first() {
                                        match arg {
                                            Expression::Variable(var) => {
                                                required_columns.insert(var.clone());
                                            }
                                            Expression::PropertyAccess { variable, property } => {
                                                required_columns
                                                    .insert(format!("{}.{}", variable, property));
                                            }
                                            _ => {}
                                        }
                                    }
                                }
                                _ => {
                                    // Check if this function contains nested aggregations
                                    // If so, don't add to projection_items here - it will be handled in post-aggregation projection
                                    if !self.contains_aggregation(&item.expression) {
                                        let alias = item.alias.clone().unwrap_or_else(|| {
                                            self.expression_to_string(&item.expression)
                                                .unwrap_or_default()
                                        });
                                        projection_items.push(ProjectionItem {
                                            alias,
                                            expression: item.expression.clone(),
                                        });
                                    }
                                }
                            }
                        }
                        _ => {
                            let alias = item.alias.clone().unwrap_or_else(|| {
                                self.expression_to_string(&item.expression)
                                    .unwrap_or_default()
                            });
                            projection_items.push(ProjectionItem {
                                alias,
                                expression: item.expression.clone(),
                            });
                        }
                    }
                }

                for column in required_columns {
                    if !projection_items.iter().any(|item| item.alias == column) {
                        let expression = if column.contains('.') {
                            let parts: Vec<&str> = column.split('.').collect();
                            if parts.len() == 2 {
                                Expression::PropertyAccess {
                                    variable: parts[0].to_string(),
                                    property: parts[1].to_string(),
                                }
                            } else {
                                Expression::Variable(column.clone())
                            }
                        } else {
                            Expression::Variable(column.clone())
                        };

                        projection_items.push(ProjectionItem {
                            alias: column.clone(),
                            expression,
                        });
                    }
                }

                if !projection_items.is_empty() {
                    operators.push(Operator::Project {
                        items: projection_items.clone(),
                    });
                }

                // Insert UNWIND operators before aggregation
                // Only if UNWIND comes AFTER MATCH (not already inserted at start)
                if !unwind_before_match {
                    for op in unwind_operators {
                        operators.push(op.clone());
                    }
                }

                let aggregations_clone = aggregations.clone();
                // Preserve the written RETURN order: the aggregate emits
                // `[group-by keys..., agg aliases...]`, which diverges from
                // the clause order whenever an aggregate precedes a grouping
                // key. Same alias derivation as the non-aggregate Project
                // branch below (G4).
                let output_order: Vec<String> = return_items
                    .iter()
                    .map(|item| {
                        item.alias.clone().unwrap_or_else(|| {
                            self.expression_to_string(&item.expression)
                                .unwrap_or_default()
                        })
                    })
                    .collect();
                operators.push(Operator::Aggregate {
                    group_by: group_by_columns,
                    aggregations,
                    projection_items: if projection_items.is_empty() {
                        None
                    } else {
                        Some(projection_items)
                    },
                    output_order: Some(output_order),
                    source: None,
                    streaming_optimized: false,
                    push_down_optimized: false,
                });

                // If WITH had a WHERE clause with aggregation, add Filter after Aggregate
                if let Some(where_expression) = with_aggregation_where {
                    let filter_str = self.predicate_to_string(where_expression)?;
                    tracing::debug!(
                        "WITH aggregation WHERE (pattern branch): Adding Filter '{}' after Aggregate",
                        filter_str
                    );
                    operators.push(Operator::Filter {
                        predicate: filter_str,
                    });
                }

                // After aggregation, apply any non-aggregate functions that wrap aggregations
                // (e.g., head(collect(...)), tail(collect(...)), reverse(collect(...)))
                let mut post_agg_projection_items = Vec::new();
                for item in return_items {
                    if let Expression::FunctionCall { name, .. } = &item.expression {
                        let func_name = name.to_lowercase();
                        // Check if this is a non-aggregate function that contains nested aggregations
                        // phase6 §9 — statistical aggregations must be recognised here too,
                        // otherwise the planner mistakes stdev/percentileCont for a
                        // wrapper around an aggregate and emits a redundant
                        // post-aggregation Project, which silently drops rows.
                        if !matches!(
                            func_name.as_str(),
                            "count"
                                | "sum"
                                | "avg"
                                | "min"
                                | "max"
                                | "collect"
                                | "stdev"
                                | "stdevp"
                                | "percentilecont"
                                | "percentiledisc"
                        ) && self.contains_aggregation(&item.expression)
                        {
                            // Replace nested aggregations with variable references
                            let modified_expr = self
                                .replace_nested_aggregations(&item.expression, &aggregations_clone);
                            post_agg_projection_items.push(ProjectionItem {
                                alias: item.alias.clone().unwrap_or_else(|| {
                                    self.expression_to_string(&item.expression)
                                        .unwrap_or_default()
                                }),
                                expression: modified_expr,
                            });
                        }
                    }
                }

                if !post_agg_projection_items.is_empty() {
                    operators.push(Operator::Project {
                        items: post_agg_projection_items,
                    });
                }
            } else {
                // Insert UNWIND operators before final projection
                // Only if UNWIND comes AFTER MATCH (not already inserted at start)
                if !unwind_before_match {
                    for op in unwind_operators {
                        operators.push(op.clone());
                    }
                }

                // Regular projection
                let projection_items: Vec<ProjectionItem> = return_items
                    .iter()
                    .map(|item| ProjectionItem {
                        alias: item.alias.clone().unwrap_or_else(|| {
                            self.expression_to_string(&item.expression)
                                .unwrap_or_default()
                        }),
                        expression: item.expression.clone(),
                    })
                    .collect();

                operators.push(Operator::Project {
                    items: projection_items,
                });

                // Add DISTINCT operator if specified
                if distinct {
                    let distinct_columns: Vec<String> = return_items
                        .iter()
                        .map(|item| {
                            item.alias.clone().unwrap_or_else(|| {
                                self.expression_to_string(&item.expression)
                                    .unwrap_or_default()
                            })
                        })
                        .collect();
                    operators.push(Operator::Distinct {
                        columns: distinct_columns,
                    });
                }

                // Add ORDER BY after DISTINCT if UNWIND is present (ORDER BY must come after DISTINCT)
                // This ensures correct order: UNWIND → Project → DISTINCT → ORDER BY → LIMIT
                if !unwind_operators.is_empty() {
                    if let Some((columns, ascending)) = order_by_clause_ref {
                        // Build a map of expression -> alias from return_items for resolution
                        let mut expression_to_alias = std::collections::HashMap::new();
                        for item in return_items.iter() {
                            let expr_str = self
                                .expression_to_string(&item.expression)
                                .unwrap_or_default();
                            let alias = item.alias.clone().unwrap_or_else(|| expr_str.clone());
                            expression_to_alias.insert(expr_str, alias);
                        }

                        // Resolve ORDER BY column names to aliases
                        let resolved_columns: Vec<String> = columns
                            .iter()
                            .map(|col| {
                                // Try to resolve to alias, otherwise use as-is
                                expression_to_alias
                                    .get(col)
                                    .cloned()
                                    .unwrap_or_else(|| col.clone())
                            })
                            .collect();

                        // Add ORDER BY right after DISTINCT (which was just added above)
                        operators.push(Operator::Sort {
                            columns: resolved_columns,
                            ascending: ascending.clone(),
                        });
                    }
                }
            }
        }

        // Add ORDER BY operator (Sort) AFTER projection/aggregation but BEFORE limit
        // This handles ORDER BY for queries WITHOUT UNWIND + DISTINCT
        // (UNWIND + DISTINCT case was already handled above)
        // Check if ORDER BY was already added (for UNWIND queries)
        let order_by_added =
            !unwind_operators.is_empty() && distinct && order_by_clause_ref.is_some();

        if !order_by_added {
            if let Some((columns, ascending)) = order_by_clause_ref {
                // Build a map of expression -> alias from return_items for resolution
                let mut expression_to_alias = std::collections::HashMap::new();
                for item in return_items.iter() {
                    let expr_str = self
                        .expression_to_string(&item.expression)
                        .unwrap_or_default();
                    let alias = item.alias.clone().unwrap_or_else(|| expr_str.clone());
                    expression_to_alias.insert(expr_str, alias);
                }

                // Resolve ORDER BY column names to aliases
                let resolved_columns: Vec<String> = columns
                    .iter()
                    .map(|col| {
                        // Try to resolve to alias, otherwise use as-is
                        expression_to_alias
                            .get(col)
                            .cloned()
                            .unwrap_or_else(|| col.clone())
                    })
                    .collect();

                // Find where to insert Sort (before Limit if exists)
                let limit_pos = operators
                    .iter()
                    .position(|op| matches!(op, Operator::Limit { .. }));

                let sort_op = Operator::Sort {
                    columns: resolved_columns,
                    ascending: ascending.clone(),
                };

                if let Some(pos) = limit_pos {
                    // Insert before Limit
                    operators.insert(pos, sort_op);
                } else {
                    // Add at the end
                    operators.push(sort_op);
                }
            }
        }

        // Add limit operator if specified
        if let Some(count) = limit_count {
            operators.push(Operator::Limit { count });
        }

        Ok(())
    }

    /// Select the most selective pattern to start execution
    /// Give a synthetic variable to anonymous anchor nodes that would
    /// otherwise leave an Expand / VariableLengthPath with an empty
    /// `source_var`. Without this, `execute_expand` takes the source-less
    /// fallback and scans every relationship of the matching type —
    /// returning every edge in the store instead of only the anchor's
    /// outgoing edges (phase6 bench §1, §2).
    ///
    /// Only synthesises for nodes that
    /// - have no variable,
    /// - carry at least one label or property (so the synthesis is worth
    ///   the NodeByLabel + Filter pair the planner emits), and
    /// - are the immediate predecessor of a Relationship element (i.e.
    ///   they are the source of a hop, not a dangling tail).
    fn synthesise_anonymous_source_anchors(pattern: &mut Pattern, counter: &mut usize) {
        let len = pattern.elements.len();
        for idx in 0..len {
            // The anchor must be a source of a relationship: next element is a Rel.
            if idx + 1 >= len {
                continue;
            }
            if !matches!(pattern.elements[idx + 1], PatternElement::Relationship(_)) {
                continue;
            }
            // And it must not itself be the target of a prior relationship —
            // that case is handled by the Expand operator's target_var path.
            if idx > 0 && matches!(pattern.elements[idx - 1], PatternElement::Relationship(_)) {
                continue;
            }
            if let PatternElement::Node(node) = &mut pattern.elements[idx] {
                if node.variable.is_some() {
                    continue;
                }
                let has_filterable = !node.labels.is_empty()
                    || node
                        .properties
                        .as_ref()
                        .map(|m| !m.properties.is_empty())
                        .unwrap_or(false);
                if !has_filterable {
                    continue;
                }
                let name = format!("__anchor_{}", *counter);
                *counter += 1;
                node.variable = Some(name);
            }
        }
    }

    pub(super) fn select_start_pattern<'b>(&self, patterns: &'b [Pattern]) -> Result<&'b Pattern> {
        if patterns.is_empty() {
            return Err(Error::CypherSyntax(
                "No patterns found in query".to_string(),
            ));
        }

        // For MVP, just return the first pattern
        // In a full implementation, we would analyze selectivity
        Ok(&patterns[0])
    }

    /// Build a `CompositeBtreeSeek` when a registered composite index
    /// (or NODE KEY constraint, which registers a UNIQUE composite
    /// index under the hood) on `label_id` has its FULL declared key
    /// set covered by `node`'s inline property-map equalities
    /// (`MATCH (n:L {a: 1, b: 2})` against an index on `(a, b)`).
    ///
    /// Called BEFORE [`Self::node_index_seek_for`] at both node-loop
    /// call sites so a composite index wins over a single-property
    /// index when both could apply: a composite seek narrows straight
    /// to the exact tuple, where a single-property seek would still
    /// need a residual `Filter` for the other predicate(s).
    ///
    /// Returns `None` (caller falls through to `node_index_seek_for` /
    /// `where_equality_index_seek_for` / `NodeByLabel`) when no
    /// composite-index handle is installed, the property map is empty
    /// or all-non-literal, or — critically — only a SUBSET of a
    /// registered index's key set is present. A partial match is
    /// deliberately never turned into a prefix seek here: the planner
    /// has no residual-filter wiring for the un-seeked trailing
    /// columns on this path, so seeking on an incomplete key would
    /// silently return every node sharing the seeked prefix instead of
    /// the caller's intended (narrower) match.
    fn composite_index_seek_for(
        &self,
        node: &NodePattern,
        label_id: u32,
        label_name: &str,
        variable: &str,
    ) -> Option<Operator> {
        let registry = self.composite_index?;
        let property_map = node.properties.as_ref()?;

        let mut literal_values: HashMap<&str, serde_json::Value> = HashMap::new();
        for (prop_name, expr) in &property_map.properties {
            let value = match expr {
                Expression::Literal(Literal::String(s)) => serde_json::Value::String(s.clone()),
                Expression::Literal(Literal::Integer(i)) => serde_json::Value::from(*i),
                Expression::Literal(Literal::Float(f)) => match serde_json::Number::from_f64(*f) {
                    Some(n) => serde_json::Value::Number(n),
                    None => continue,
                },
                Expression::Literal(Literal::Boolean(b)) => serde_json::Value::Bool(*b),
                // null / point / parameter / non-literal / correlated:
                // not indexable at plan time for a composite seek.
                _ => continue,
            };
            literal_values.insert(prop_name.as_str(), value);
        }
        if literal_values.is_empty() {
            return None;
        }

        for (lbl, keys, _unique, _name) in registry.list() {
            if lbl != label_id || keys.is_empty() {
                continue;
            }
            if !keys.iter().all(|k| literal_values.contains_key(k.as_str())) {
                continue;
            }
            let prefix: Vec<(String, serde_json::Value)> = keys
                .iter()
                .filter_map(|k| {
                    literal_values
                        .get(k.as_str())
                        .cloned()
                        .map(|v| (k.clone(), v))
                })
                .collect();
            return Some(Operator::CompositeBtreeSeek {
                label: label_name.to_string(),
                variable: variable.to_string(),
                prefix,
            });
        }
        None
    }

    /// Build a `NodeIndexSeek` for the first inline equality property of
    /// `node` whose `(label_id, key_id)` has a registered property index
    /// and whose value is either an indexable literal (constant seek) or a
    /// row-local expression (`a.prop` / bare variable — per-row correlated
    /// seek, evaluated at execution time by `execute_correlated_index_seek`).
    /// Returns `None` (caller falls back to `NodeByLabel`) when no
    /// PropertyIndex handle is installed, no property qualifies, or the
    /// value is null/point/parameter/non-literal and non-correlated.
    fn node_index_seek_for(
        &self,
        node: &NodePattern,
        label_id: u32,
        variable: &str,
    ) -> Option<Operator> {
        let prop_idx = self.property_index?;
        let property_map = node.properties.as_ref()?;
        for (prop_name, expr) in &property_map.properties {
            let Ok(key_id) = self.catalog.get_key_id(prop_name) else {
                continue;
            };
            if !prop_idx.has_index(label_id, key_id) {
                continue;
            }
            match expr {
                // Constant: value baked into the plan at plan time.
                Expression::Literal(Literal::String(s)) => {
                    return Some(Operator::NodeIndexSeek {
                        label_id,
                        key_id,
                        value: crate::index::PropertyValue::String(s.clone()),
                        key_expression: None,
                        variable: variable.to_string(),
                    });
                }
                Expression::Literal(Literal::Integer(i)) => {
                    return Some(Operator::NodeIndexSeek {
                        label_id,
                        key_id,
                        value: crate::index::PropertyValue::Integer(*i),
                        key_expression: None,
                        variable: variable.to_string(),
                    });
                }
                Expression::Literal(Literal::Float(f)) => {
                    return Some(Operator::NodeIndexSeek {
                        label_id,
                        key_id,
                        value: crate::index::PropertyValue::Float(*f),
                        key_expression: None,
                        variable: variable.to_string(),
                    });
                }
                Expression::Literal(Literal::Boolean(b)) => {
                    return Some(Operator::NodeIndexSeek {
                        label_id,
                        key_id,
                        value: crate::index::PropertyValue::Boolean(*b),
                        key_expression: None,
                        variable: variable.to_string(),
                    });
                }
                // Row-local / correlated: e.g. `r.s` from
                // `UNWIND $rows AS r MATCH (a:P {id: r.s})`. The key is
                // evaluated per driving row at execution time, so the
                // plan-time `value` is a documented no-op placeholder —
                // `execute_correlated_index_seek` ignores it whenever
                // `key_expression` is `Some(_)`.
                Expression::PropertyAccess { .. } | Expression::Variable(_) => {
                    return Some(Operator::NodeIndexSeek {
                        label_id,
                        key_id,
                        value: crate::index::PropertyValue::Null,
                        key_expression: Some(expr.clone()),
                        variable: variable.to_string(),
                    });
                }
                // null / point / param / other non-literal: not indexable.
                _ => continue,
            }
        }
        None
    }

    /// Flatten a WHERE-clause expression into its top-level AND-conjuncts,
    /// recursing through nested `AND`s (`a AND b AND c` yields `[a, b,
    /// c]`). A non-`AND` expression (including one rooted in `OR`, since
    /// splitting an `OR`'s branches would change its meaning) yields
    /// itself as the sole conjunct.
    fn flatten_and_conjuncts(expr: &Expression, out: &mut Vec<Expression>) {
        if let Expression::BinaryOp {
            left,
            op: BinaryOperator::And,
            right,
        } = expr
        {
            Self::flatten_and_conjuncts(left, out);
            Self::flatten_and_conjuncts(right, out);
        } else {
            out.push(expr.clone());
        }
    }

    /// Rebuild an AND-conjunction `Expression` from its conjuncts — the
    /// inverse of [`Self::flatten_and_conjuncts`]. Returns `None` for an
    /// empty list (every conjunct on that WHERE-clause entry was lifted
    /// into a seek by [`Self::where_equality_index_seek_for`], so the
    /// caller should drop the entry rather than emit an empty/always-true
    /// Filter).
    fn rebuild_and_conjunction(conjuncts: Vec<Expression>) -> Option<Expression> {
        let mut iter = conjuncts.into_iter();
        let first = iter.next()?;
        Some(iter.fold(first, |acc, next| Expression::BinaryOp {
            left: Box::new(acc),
            op: BinaryOperator::And,
            right: Box::new(next),
        }))
    }

    /// WHERE-form counterpart to [`Self::node_index_seek_for`]: looks for
    /// a top-level `variable.prop = <constant>` conjunct across
    /// `residual` — the AND-conjunct-decomposed working copy of the
    /// query's WHERE clauses, built once in [`Self::plan_execution_strategy`]
    /// — whose `(label_id, prop)` pair has a registered single-property
    /// index. On a match, removes the consumed conjunct from `residual`
    /// (dropping the whole entry once it has no conjuncts left) and
    /// returns the `NodeIndexSeek` operator to emit in place of
    /// `NodeByLabel`. Returns `None` when no such conjunct exists, mirroring
    /// `node_index_seek_for`'s "caller falls back to `NodeByLabel`" contract.
    ///
    /// Only inspects entries with an EMPTY `optional_vars` — lifting a
    /// conjunct out of what would become an `OptionalFilter` would change
    /// OPTIONAL MATCH semantics (a failed predicate there nulls the
    /// optional variables rather than dropping the row), so OPTIONAL
    /// MATCH WHERE clauses are left untouched and keep going through the
    /// existing `OptionalFilter` path.
    fn where_equality_index_seek_for(
        &self,
        variable: &str,
        label_id: u32,
        residual: &mut [(Vec<Expression>, Vec<String>)],
    ) -> Option<Operator> {
        self.property_index?;
        for (conjuncts, optional_vars) in residual.iter_mut() {
            if !optional_vars.is_empty() {
                continue;
            }
            for i in 0..conjuncts.len() {
                if let Some(seek) =
                    self.where_equality_seek_operand(&conjuncts[i], variable, label_id)
                {
                    conjuncts.remove(i);
                    return Some(seek);
                }
            }
        }
        None
    }

    /// If `conjunct` is a top-level equality `variable.prop = <constant>`
    /// (or the mirrored `<constant> = variable.prop`), and `(label_id,
    /// prop)` has a registered single-property index, return the
    /// `NodeIndexSeek` operator to emit in its place.
    ///
    /// "Constant" here means a plan-time LITERAL only — `$parameter`
    /// values are deliberately excluded. The planner has no access to
    /// bound parameter values (they are supplied at execution time), and
    /// `NodeIndexSeek`'s `key_expression`-driven correlated-seek path
    /// (`execute_correlated_index_seek`) requires driving rows to
    /// already exist in the pipeline — the common case a bare `WHERE
    /// n.prop = $x` lowers to is the FIRST scan of the query, where no
    /// driving rows exist yet, so routing a parameter through that path
    /// would silently return zero rows instead of seeking. Lifting
    /// `$parameter` equality is left to a follow-up.
    ///
    /// SCOPE: EQUALITY ONLY. Range (`>`, `<`, `>=`, `<=`), `IN`, `STARTS
    /// WITH`, and `CONTAINS` predicates are never lifted here — they
    /// remain full scans, made observable via the
    /// `Nexus.Performance.UnindexedPropertyAccess` notification
    /// (`unindexed.rs`).
    fn where_equality_seek_operand(
        &self,
        conjunct: &Expression,
        variable: &str,
        label_id: u32,
    ) -> Option<Operator> {
        let prop_idx = self.property_index?;
        let Expression::BinaryOp {
            left,
            op: BinaryOperator::Equal,
            right,
        } = conjunct
        else {
            return None;
        };
        // Resolve which side is the `var.prop` operand and which is the
        // candidate constant. Left is preferred when both sides are
        // property accesses, matching `unindexed.rs`'s resolution order.
        let (property, value_expr) = match (left.as_ref(), right.as_ref()) {
            (
                Expression::PropertyAccess {
                    variable: v,
                    property,
                },
                other,
            ) if v == variable => (property, other),
            (
                other,
                Expression::PropertyAccess {
                    variable: v,
                    property,
                },
            ) if v == variable => (property, other),
            _ => return None,
        };
        let key_id = self.catalog.get_key_id(property).ok()?;
        if !prop_idx.has_index(label_id, key_id) {
            return None;
        }
        let value = match value_expr {
            Expression::Literal(Literal::String(s)) => {
                crate::index::PropertyValue::String(s.clone())
            }
            Expression::Literal(Literal::Integer(i)) => crate::index::PropertyValue::Integer(*i),
            Expression::Literal(Literal::Float(f)) => crate::index::PropertyValue::Float(*f),
            Expression::Literal(Literal::Boolean(b)) => crate::index::PropertyValue::Boolean(*b),
            // null / point / parameter / non-literal: not indexable at
            // plan time — see the doc comment above.
            _ => return None,
        };
        Some(Operator::NodeIndexSeek {
            label_id,
            key_id,
            value,
            key_expression: None,
            variable: variable.to_string(),
        })
    }
}
