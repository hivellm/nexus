//! `plan_execution_strategy` — the pattern-to-operator lowering pass: MATCH
//! pattern NodeByLabel/AllNodesScan/index-seek emission, Expand emission for
//! relationship hops, WHERE-clause Filter/OptionalFilter lowering, RETURN
//! projection/aggregation build, and the trailing ORDER BY/SKIP/LIMIT
//! operators.

use super::*;

impl<'a> QueryPlanner<'a> {
    /// Plan execution strategy based on patterns and constraints
    #[allow(clippy::too_many_arguments)]
    pub(in crate::executor::planner::queries) fn plan_execution_strategy(
        &self,
        patterns: &[(Pattern, bool)], // (Pattern, is_optional)
        where_clauses: &[(Expression, Vec<String>)], // (expression, optional_vars)
        return_items: &[ReturnItem],
        limit_count: Option<usize>,
        skip_count: Option<usize>,
        distinct: bool,
        unwind_operators: &[Operator],
        unwind_before_match: bool,
        hints: &[QueryHint],
        order_by_clause: &Option<(Vec<String>, Vec<bool>)>,
        with_aggregation_where: &Option<Expression>, // WHERE from WITH with aggregation
        // Variables already bound by a prior query segment (the clauses
        // before a `WITH` in a segmented plan — see `plan_segmented`). Such
        // a variable is already materialised in the execution context, so
        // its node must NOT be re-scanned here (that would clobber the
        // carried binding); it is treated as an Expand anchor instead. Empty
        // for every single-segment (non-`WITH`-crossing) query, so existing
        // plans are unaffected.
        already_bound: &std::collections::HashSet<String>,
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

        // Process the first pattern (extract pattern from tuple)
        let patterns_only: Vec<Pattern> = patterns_local.iter().map(|(p, _)| p.clone()).collect();
        let start_pattern = self.select_start_pattern(&patterns_only)?;

        // Nodes this pattern's OWN relationship hops will populate, so it must
        // not also emit a driving scan for them (the `Expand` binds them).
        //
        // Scoped to one pattern, never across all of them. A variable a LATER
        // clause happens to use as a relationship target is still bound by the
        // clause it is written in, and suppressing that clause's scan left it
        // bound nowhere: `MATCH (a:A), (z:Z) OPTIONAL MATCH (a)-[r:T]->(z)`
        // emitted no scan for `z` at all, so the optional hop treated `z` as its
        // own output slot and NULL-padded it, instead of keeping `z` bound and
        // nulling only `r`.
        let start_target_nodes = Self::relationship_target_vars(start_pattern);

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
                    // A variable carried in from a prior `WITH` segment is
                    // already materialised in the context. Never re-scan it —
                    // that clobbers the carried binding. Skip even when it is
                    // the pattern's first node (the `is_first_node` forcing
                    // below must not override this); the Expand emitted by
                    // `add_relationship_operators` uses it as the source
                    // anchor and reads it from the live row instead.
                    if already_bound.contains(variable) {
                        continue;
                    }
                    // CRITICAL: Check if this is the first node in the pattern
                    let is_first_node = Some(variable.clone()) == first_node_var;

                    // Skip if this node is a pure target without labels (will be populated by Expand)
                    // EXCEPTION: Always create NodeByLabel for the first node, even in cyclic patterns
                    if !is_first_node && start_target_nodes.contains(variable) {
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

                        if first_label.starts_with('$') {
                            // Dynamic-label sentinel (`$param`): the planner
                            // has no params (they live on the execution
                            // context), so it cannot resolve the label id
                            // here. Drive the scan with AllNodesScan and
                            // lower the sentinel to a label-check Filter —
                            // the same lowering used for additional labels
                            // below — which resolves `$param` against the
                            // runtime params in operators/filter.rs (a
                            // non-empty STRING becomes the label;
                            // NULL/empty/non-STRING yields no rows, to
                            // mirror `WHERE n:$x`).
                            operators.push(Operator::AllNodesScan {
                                variable: variable.clone(),
                            });
                            operators.push(Operator::Filter {
                                predicate: format!("{}:{}", variable, first_label),
                                predicate_ast: None,
                            });
                        } else {
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
                                if let Some(seek) = self.composite_index_seek_for(
                                    node,
                                    label_id,
                                    first_label,
                                    variable,
                                ) {
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
                        }

                        // Add filters for additional labels (multiple label intersection)
                        if node.labels.len() > 1 {
                            for additional_label in &node.labels[1..] {
                                // Create a filter that checks if node has this label
                                let filter_expr = format!("{}:{}", variable, additional_label);
                                operators.push(Operator::Filter {
                                    predicate: filter_expr,
                                    predicate_ast: None,
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
                                predicate_ast: None,
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
            already_bound, // vars carried in from a prior WITH segment anchor the first Expand
            // `select_start_pattern` always returns `patterns[0]`, and the
            // additional-pattern loop below skips index 0, so scope 0 is this
            // clause's and only this clause's.
            0,
        )?;

        // Track variables bound by the first pattern (for OPTIONAL MATCH
        // handling). Seed it with the segment's carried-in bindings so
        // additional comma-separated patterns also treat them as anchors.
        let mut previously_bound_vars: std::collections::HashSet<String> = already_bound.clone();
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
            let pattern_target_nodes = Self::relationship_target_vars(pattern);
            for element in &pattern.elements {
                if let PatternElement::Node(node) = element {
                    if let Some(variable) = &node.variable {
                        if pattern_target_nodes.contains(variable) {
                            continue;
                        }

                        // Skip NodeByLabel for unbound vars in OPTIONAL MATCH if pattern has a bound var
                        if *is_optional
                            && pattern_has_bound_var
                            && !previously_bound_vars.contains(variable)
                        {
                            continue;
                        }

                        // An UNLABELLED node in an additional pattern still needs
                        // a driving scan. Without one nothing ever binds it: as an
                        // `Expand` source it makes `execute_expand` find no
                        // `source_var` in the row and drop every row (so
                        // `MATCH (x)-[r1]->(y) MATCH (p)-[r2]->(q)` returned no
                        // rows at all), and as a bare node it stays unbound and
                        // projects as NULL (so `MATCH (x)-[r1]->(y) MATCH (p)`
                        // returned one NULL-padded row instead of the cartesian).
                        // The first pattern's lowering already emits `AllNodesScan`
                        // here; only this loop was missing it.
                        //
                        // A variable an earlier pattern already bound must NOT be
                        // rescanned — that would discard its binding and re-drive
                        // the query from every node (`MATCH (a)-[r]->(b)
                        // MATCH (b)-[r2]->(c)` must keep `b`).
                        if previously_bound_vars.contains(variable) {
                            // Keep the binding, but still enforce this pattern's
                            // label predicate — as a Filter, which the planner's
                            // operator sort recombines AFTER every Expand, so it
                            // sees the variable bound. Skipping the node outright
                            // (which the old cross-pattern target set caused for
                            // this shape) dropped the predicate silently:
                            // `MATCH (a)-[:T]->(b) MATCH (b:B)` matched a `b`
                            // carrying no `:B` label at all.
                            //
                            // Only for a required pattern. In an OPTIONAL one a
                            // contradictory label should leave the row padded
                            // rather than drop it, and a Filter cannot express
                            // that — so the predicate stays dropped there, as
                            // before, rather than trading one wrong answer for
                            // another.
                            if !*is_optional {
                                for label in &node.labels {
                                    operators.push(Operator::Filter {
                                        predicate: format!("{}:{}", variable, label),
                                        predicate_ast: None,
                                    });
                                }
                            }
                        } else if node.labels.is_empty() {
                            operators.push(Operator::AllNodesScan {
                                variable: variable.clone(),
                            });
                        } else {
                            let first_label = &node.labels[0];

                            if first_label.starts_with('$') {
                                // Dynamic-label sentinel (`$param`): the
                                // planner has no params (they live on the
                                // execution context), so it cannot resolve
                                // the label id here. Drive the scan with
                                // AllNodesScan and lower the sentinel to a
                                // label-check Filter — the same lowering
                                // used for additional labels below — which
                                // resolves `$param` against the runtime
                                // params in operators/filter.rs (a
                                // non-empty STRING becomes the label;
                                // NULL/empty/non-STRING yields no rows, to
                                // mirror `WHERE n:$x`).
                                operators.push(Operator::AllNodesScan {
                                    variable: variable.clone(),
                                });
                                operators.push(Operator::Filter {
                                    predicate: format!("{}:{}", variable, first_label),
                                    predicate_ast: None,
                                });
                            } else {
                                let label_id = self.catalog.get_or_create_label(first_label)?;
                                if let Some(seek) = self.composite_index_seek_for(
                                    node,
                                    label_id,
                                    first_label,
                                    variable,
                                ) {
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

                            // Add filters for additional labels
                            if node.labels.len() > 1 {
                                for additional_label in &node.labels[1..] {
                                    let filter_expr = format!("{}:{}", variable, additional_label);
                                    operators.push(Operator::Filter {
                                        predicate: filter_expr,
                                        predicate_ast: None,
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
                                    predicate_ast: None,
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
                // One `Pattern` per MATCH clause (the parser flattens comma
                // parts into one), so the index IS the clause identity.
                pattern_idx as u32,
            )?;

            // This pattern's variables are bound from here on, so the NEXT
            // pattern's guards see them. The set was previously seeded from the
            // start pattern only and never grown, which made every guard that
            // asks "did an earlier pattern already bind this?" answer no from the
            // third pattern onward. That went unnoticed because the scan
            // suppression used to consult a target set pooled across all
            // patterns, which masked it: in
            // `MATCH (a) OPTIONAL MATCH (a)-->(b) OPTIONAL MATCH (b)-->(c)`,
            // `b` was suppressed in the third clause only because it was a
            // relationship target in the second. Scoping that set per pattern
            // removed the mask, so the real binding order has to be tracked.
            for element in &pattern.elements {
                if let PatternElement::Node(node) = element
                    && let Some(var) = &node.variable
                {
                    previously_bound_vars.insert(var.clone());
                }
            }
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
                operators.push(Operator::Filter {
                    predicate,
                    predicate_ast: Some(Box::new(where_clause.clone())),
                });
            } else {
                tracing::debug!(
                    "  WHERE clause #{}: {} (OptionalFilter, vars={:?})",
                    idx,
                    predicate,
                    optional_vars
                );
                operators.push(Operator::OptionalFilter {
                    predicate,
                    predicate_ast: Some(Box::new(where_clause.clone())),
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

            // Lift aggregates nested inside a larger expression (`count(*) > 0`,
            // `count(*) + 1`, `[count(*)]`, `head(collect(v))`) out into their
            // own synthetic bare-aggregate `ReturnItem`s, so the classification
            // loop below — which only recognises a *bare* aggregate call — sees
            // every aggregation the query actually asks for. The enclosing
            // expression (`> 0`, `+ 1`, `head(...)`, ...) is deferred to a
            // post-aggregation `Project` pushed after `Operator::Aggregate`
            // below, which evaluates it against the synthetic alias the
            // Aggregate operator produced. Items that are already a bare
            // aggregate call, or that carry no aggregation at all, pass through
            // unchanged; `post_agg_items` stays index-aligned with
            // `return_items` (`None` for a pass-through item) so the original
            // clause order/shape can be reproduced exactly. Mirrors the
            // planner_core `bound.rs` pre-pass.
            let mut effective_return_items: Vec<ReturnItem> = Vec::new();
            let mut post_agg_items: Vec<Option<ProjectionItem>> =
                Vec::with_capacity(return_items.len());
            let mut lift_index = 0usize;
            for item in return_items.iter() {
                let is_bare_aggregate = matches!(
                    &item.expression,
                    Expression::FunctionCall { name, .. }
                        if Self::is_aggregate_function_name(&name.to_lowercase())
                );
                if is_bare_aggregate || !self.contains_aggregation(&item.expression) {
                    effective_return_items.push(item.clone());
                    post_agg_items.push(None);
                    continue;
                }

                let alias = item.alias.clone().unwrap_or_else(|| {
                    self.expression_to_string(&item.expression)
                        .unwrap_or_default()
                });
                let (rewritten, lifted) = self.lift_aggregations(&item.expression, &mut lift_index);
                for (synthetic_alias, aggregate_call) in lifted {
                    effective_return_items.push(ReturnItem {
                        expression: aggregate_call,
                        alias: Some(synthetic_alias),
                    });
                }
                post_agg_items.push(Some(ProjectionItem {
                    alias,
                    expression: rewritten,
                }));
            }

            for item in &effective_return_items {
                // First, check if this expression contains any nested aggregations
                if self.contains_aggregation(&item.expression) {
                    has_aggregation = true;
                }

                match &item.expression {
                    Expression::FunctionCall { name, args } => {
                        let func_name = name.to_lowercase();
                        // Verbatim-style default column name for an unaliased
                        // aggregate (`count(*)`, `count(DISTINCT n)`,
                        // `sum(n.age)`) — matches the openCypher TCK and Neo4j,
                        // which name the column after the whole call rather
                        // than the bare function name. Mirrors the planner_core
                        // aggregate path; non-aggregate calls keep using
                        // expression_to_string in their own arms.
                        let agg_default_alias = self.aggregate_display_name(name, args);
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
                                        .unwrap_or_else(|| agg_default_alias.clone()),
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
                                            .unwrap_or_else(|| agg_default_alias.clone()),
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
                                            .unwrap_or_else(|| agg_default_alias.clone()),
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
                                            .unwrap_or_else(|| agg_default_alias.clone()),
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
                                            .unwrap_or_else(|| agg_default_alias.clone()),
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
                                            .unwrap_or_else(|| agg_default_alias.clone()),
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
                                            .unwrap_or_else(|| agg_default_alias.clone()),
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
                                            .unwrap_or_else(|| agg_default_alias.clone()),
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
                                            .unwrap_or_else(|| agg_default_alias.clone()),
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
                                            .unwrap_or_else(|| agg_default_alias.clone()),
                                        percentile,
                                    });
                                }
                            }
                            _ => {
                                // Not an aggregate function. The lifting
                                // pre-pass above has already pulled any nested
                                // aggregation out of this item into its own
                                // synthetic bare-aggregate `ReturnItem`, so an
                                // item that reaches this arm carries no
                                // aggregation — treat it as a regular column
                                // for GROUP BY.
                                let alias = item.alias.clone().unwrap_or_else(|| {
                                    self.expression_to_string(&item.expression)
                                        .unwrap_or_default()
                                });
                                non_aggregate_aliases.push(alias);
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

                for item in &effective_return_items {
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

                // Preserve the written RETURN order: the aggregate emits
                // `[group-by keys..., agg aliases...]`, which diverges from
                // the clause order whenever an aggregate precedes a grouping
                // key. Derived from the *effective* items so it names the
                // synthetic columns the aggregate actually emits; the
                // post-aggregation projection below restores the written
                // column list. Same alias derivation as the non-aggregate
                // Project branch below (G4).
                let output_order: Vec<String> = effective_return_items
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

                // Post-aggregation projection: evaluate any expression that
                // merely *wraps* an aggregate (`count(*) > 0`, `count(*) + 1`,
                // `head(collect(...))`, ...) against the synthetic column the
                // Aggregate produced, and pass every other item through by
                // name. Reproducing `return_items` position by position is
                // what keeps the grouping-key columns — and the written column
                // order — in the output. It precedes the Filter below because a
                // `WITH ... WHERE` may reference an alias only this projection
                // produces.
                if post_agg_items.iter().any(|item| item.is_some()) {
                    let items: Vec<ProjectionItem> = return_items
                        .iter()
                        .zip(post_agg_items)
                        .map(|(item, post_item)| {
                            post_item.unwrap_or_else(|| {
                                let alias = item.alias.clone().unwrap_or_else(|| {
                                    self.expression_to_string(&item.expression)
                                        .unwrap_or_default()
                                });
                                ProjectionItem {
                                    expression: Expression::Variable(alias.clone()),
                                    alias,
                                }
                            })
                        })
                        .collect();
                    operators.push(Operator::Project { items });
                }

                // If WITH had a WHERE clause with aggregation, add Filter after Aggregate
                if let Some(where_expression) = with_aggregation_where {
                    let filter_str = self.predicate_to_string(where_expression)?;
                    tracing::debug!(
                        "WITH aggregation WHERE (pattern branch): Adding Filter '{}' after Aggregate",
                        filter_str
                    );
                    operators.push(Operator::Filter {
                        predicate: filter_str,
                        predicate_ast: Some(Box::new(where_expression.clone())),
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
                // Resolve to projected aliases, projecting a hidden column for any
                // key the RETURN does not produce — see
                // `resolve_order_by_columns`.
                let resolved_columns =
                    self.resolve_order_by_columns(columns, return_items, operators, distinct);

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

        // Add SKIP after ORDER BY and before LIMIT — the standard openCypher
        // ORDER BY -> SKIP -> LIMIT pipeline order. Sort was appended above and
        // Limit is appended below, so pushing here keeps that ordering. Mirrors
        // the pattern-less branches in planner_core.rs.
        if let Some(count) = skip_count {
            operators.push(Operator::Skip { count });
        }

        // Add limit operator if specified
        if let Some(count) = limit_count {
            operators.push(Operator::Limit { count });
        }

        Ok(())
    }

    /// Resolve each `ORDER BY` key to a column the `Sort` operator can actually
    /// read, projecting a HIDDEN column for any key the `RETURN` does not already
    /// produce.
    ///
    /// `Sort` runs AFTER the projection and reads `result_set` columns, so a key
    /// the projection drops is simply not there at sort time —
    /// `RETURN n.name ORDER BY n.age` had no `n` left to read `age` from, and
    /// `execute_sort` skipped the unresolved key silently, returning rows in
    /// whatever order they arrived. Re-evaluating the expression at sort time
    /// cannot fix that: the data is already gone. The key has to be CARRIED
    /// through the projection, which is what the hidden column does; `Sort` drops
    /// it again once the rows are ordered (see `execute_sort`).
    ///
    /// The key's expression is recovered by re-parsing the string the planner
    /// produced with `expression_to_string` — the same round-trip
    /// `Operator::Filter` relies on when it carries no AST. A key that does not
    /// re-parse keeps today's behaviour (used as-is, and skipped at execution)
    /// rather than failing the query.
    ///
    /// Skipped entirely for `DISTINCT` and for aggregating projections: Cypher
    /// does not allow `ORDER BY` on a value the projection did not produce there
    /// (the pre-projection variables are out of scope), and a hidden column would
    /// silently change what `DISTINCT` dedups on or what the aggregation groups
    /// by.
    pub(in crate::executor::planner::queries) fn resolve_order_by_columns(
        &self,
        columns: &[String],
        return_items: &[ReturnItem],
        operators: &mut [Operator],
        distinct: bool,
    ) -> Vec<String> {
        let mut expression_to_alias = std::collections::HashMap::new();
        for item in return_items.iter() {
            let expr_str = self
                .expression_to_string(&item.expression)
                .unwrap_or_default();
            let alias = item.alias.clone().unwrap_or_else(|| expr_str.clone());
            expression_to_alias.insert(expr_str, alias);
        }
        // An alias may also be named directly (`RETURN n.age AS a ORDER BY a`).
        let projected_aliases: std::collections::HashSet<&String> =
            expression_to_alias.values().collect();

        let has_aggregate = operators
            .iter()
            .any(|op| matches!(op, Operator::Aggregate { .. }));
        let may_project_hidden = !distinct && !has_aggregate;

        let mut hidden: Vec<(String, ProjectionItem)> = Vec::new();
        let resolved: Vec<String> = columns
            .iter()
            .enumerate()
            .map(|(idx, col)| {
                if let Some(alias) = expression_to_alias.get(col) {
                    return alias.clone();
                }
                if projected_aliases.contains(col) {
                    return col.clone();
                }
                if may_project_hidden {
                    let mut parser = crate::executor::parser::CypherParser::new(col.clone());
                    if let Ok(expression) = parser.parse_expression() {
                        let alias = format!(
                            "{}{idx}",
                            crate::executor::operators::project::ORDER_BY_HIDDEN_KEY_PREFIX
                        );
                        hidden.push((
                            alias.clone(),
                            ProjectionItem {
                                expression,
                                alias: alias.clone(),
                            },
                        ));
                        return alias;
                    }
                }
                col.clone()
            })
            .collect();

        if !hidden.is_empty() {
            // Attach to the LAST projection in the pipeline — the one whose
            // columns the Sort will read.
            if let Some(Operator::Project { items }) = operators
                .iter_mut()
                .rev()
                .find(|op| matches!(op, Operator::Project { .. }))
            {
                items.extend(hidden.into_iter().map(|(_, item)| item));
            }
        }

        resolved
    }

    /// Node variables ONE pattern's own relationship hops will populate: the node
    /// written immediately after each relationship element. The `Expand` those
    /// hops lower to binds them, so the pattern must not also emit a driving scan
    /// for them — that would discard the traversal's result and re-drive the
    /// query from every node.
    ///
    /// Deliberately per-pattern. Computed across every pattern of the query, it
    /// suppressed the scan of a variable that a DIFFERENT clause binds
    /// independently — see the call site for the shape that broke.
    fn relationship_target_vars(pattern: &Pattern) -> std::collections::HashSet<String> {
        let mut targets = std::collections::HashSet::new();
        for (idx, element) in pattern.elements.iter().enumerate() {
            if matches!(element, PatternElement::Relationship(_))
                && let Some(PatternElement::Node(node)) = pattern.elements.get(idx + 1)
                && let Some(var) = &node.variable
            {
                targets.insert(var.clone());
            }
        }
        targets
    }
}
