//! `WITH → MATCH` segmented planning: detects when a query needs
//! per-segment planning, splits it at `WITH` boundaries, threads
//! carried bindings across segments via [`super::bound::plan_query_bound`],
//! and the pattern-variable collection helper shared with the
//! single-segment path.

use super::super::*;

impl<'a> QueryPlanner<'a> {
    /// Collect every variable a pattern introduces — node variables,
    /// relationship variables, and the variables of any quantified-group
    /// members — with no positional skip. Used to compute an OPTIONAL
    /// MATCH's nullable-variable set as a set difference against
    /// variables already bound by prior clauses, instead of assuming the
    /// pattern's first node is always the already-bound anchor (that
    /// assumption breaks for reverse-direction patterns like
    /// `(b)-[:KNOWS]->(a)` where `a` is the bound anchor, and for
    /// standalone patterns with no bound anchor at all).
    /// True when a `MATCH` clause textually follows a `WITH` clause. Such a
    /// query must be planned segment-by-segment (phase7 §4.11): the bucket
    /// planner would otherwise fold the post-`WITH` `MATCH` into the
    /// pre-`WITH` pattern phase, and the `WITH` projection would then drop
    /// every variable that `MATCH` introduced. Deliberately scoped to
    /// `MATCH` only — `WITH … CREATE/MERGE/…` are separate concerns and keep
    /// the single-segment path.
    pub(super) fn has_match_after_with(query: &CypherQuery) -> bool {
        let mut seen_with = false;
        for clause in &query.clauses {
            match clause {
                Clause::With(_) => seen_with = true,
                Clause::Match(_) if seen_with => return true,
                _ => {}
            }
        }
        false
    }

    /// Plan a query that crosses one or more `WITH → MATCH` boundaries by
    /// splitting it into `WITH`-delimited segments and planning each in
    /// order. Every non-final segment ends with its `WITH` clause (the
    /// projection that closes it); the trailing clauses form the final
    /// segment. Each segment is planned by [`Self::plan_query_bound`] with
    /// the set of variables the previous segment's `WITH` carried forward,
    /// so a post-`WITH` `MATCH` expands from those bindings (or
    /// Cartesian-joins a fresh scan against them) instead of re-scanning and
    /// clobbering them. The per-segment operator lists are concatenated; the
    /// executor runs them against one shared context, where each segment's
    /// terminal projection leaves exactly the carried scope for the next.
    pub(super) fn plan_segmented(&mut self, query: &CypherQuery) -> Result<Vec<Operator>> {
        // Split clauses into segments. A `WITH` closes the segment it ends.
        let mut segments: Vec<Vec<Clause>> = Vec::new();
        let mut current: Vec<Clause> = Vec::new();
        for clause in &query.clauses {
            current.push(clause.clone());
            if matches!(clause, Clause::With(_)) {
                segments.push(std::mem::take(&mut current));
            }
        }
        if !current.is_empty() {
            segments.push(current);
        }

        let mut all_ops: Vec<Operator> = Vec::new();
        let mut bound: std::collections::HashSet<String> = std::collections::HashSet::new();
        let seg_count = segments.len();
        for (idx, seg_clauses) in segments.into_iter().enumerate() {
            // A segment's terminal `WITH` restricts the scope to exactly the
            // variables it projects — those become the next segment's
            // carried bindings (replace, not accumulate, since `WITH` drops
            // everything it does not re-export).
            let seg_output = Self::segment_output_vars(&seg_clauses);
            let seg_query = CypherQuery {
                clauses: seg_clauses,
                params: query.params.clone(),
                graph_scope: query.graph_scope.clone(),
            };
            let seg_ops = self.plan_query_bound(&seg_query, &bound)?;
            all_ops.extend(seg_ops);
            if idx + 1 < seg_count {
                bound = seg_output;
            }
        }
        Ok(all_ops)
    }

    /// The variables a segment exports to the next one: the aliases (or bare
    /// variable names) projected by the segment's terminal `WITH`. Returns
    /// empty for the final segment (which ends in `RETURN` and has no
    /// successor) or any segment not ending in `WITH`.
    pub(super) fn segment_output_vars(clauses: &[Clause]) -> std::collections::HashSet<String> {
        let mut out = std::collections::HashSet::new();
        if let Some(Clause::With(with)) = clauses.last() {
            for item in &with.items {
                if let Some(alias) = &item.alias {
                    out.insert(alias.clone());
                } else if let Expression::Variable(v) = &item.expression {
                    out.insert(v.clone());
                }
                // A non-aliased non-variable projection (e.g. `WITH a.x`)
                // is not a legal downstream identifier, so it contributes
                // no carried binding.
            }
        }
        out
    }

    pub(super) fn collect_pattern_variables(pattern: &Pattern) -> Vec<String> {
        let mut vars = Vec::new();
        for element in &pattern.elements {
            match element {
                PatternElement::Node(node) => {
                    if let Some(var) = &node.variable {
                        vars.push(var.clone());
                    }
                }
                PatternElement::Relationship(rel) => {
                    if let Some(var) = &rel.variable {
                        vars.push(var.clone());
                    }
                }
                PatternElement::QuantifiedGroup(group) => {
                    for inner in &group.inner {
                        match inner {
                            PatternElement::Node(n) => {
                                if let Some(var) = &n.variable {
                                    vars.push(var.clone());
                                }
                            }
                            PatternElement::Relationship(r) => {
                                if let Some(var) = &r.variable {
                                    vars.push(var.clone());
                                }
                            }
                            PatternElement::QuantifiedGroup(_) => {}
                        }
                    }
                }
            }
        }
        vars
    }
}
