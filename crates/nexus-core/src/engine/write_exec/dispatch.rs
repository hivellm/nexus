//! Write-query clause dispatch: the linear and UNWIND-write clause loops,
//! plus the MATCH/WHERE resolution they use to bind variables on the write
//! path. Extracted from `engine/write_exec.rs`.

use super::super::Engine;
use crate::storage::external_id::{ConflictPolicy, ExternalId};
use crate::{Error, Result, executor};
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};

/// Convert an AST-level conflict policy to the storage-level one used by
/// [`Engine::create_node_with_external_id`]. Mirrors
/// `executor::operators::create::ast_conflict_policy_to_storage`; duplicated
/// here (rather than reused across the module boundary) because the
/// executor's helper is `pub(in crate::executor)` and the write-query path
/// lives in `crate::engine`.
fn ast_conflict_policy_to_storage(p: executor::parser::AstConflictPolicy) -> ConflictPolicy {
    match p {
        executor::parser::AstConflictPolicy::Error => ConflictPolicy::Error,
        executor::parser::AstConflictPolicy::Match => ConflictPolicy::Match,
        executor::parser::AstConflictPolicy::Replace => ConflictPolicy::Replace,
    }
}

impl Engine {
    /// Resolve a parsed `_id` expression (string-literal or parameter) into
    /// an [`ExternalId`]. Mirrors `Executor::resolve_external_id`; anything
    /// other than a string literal or parameter is rejected at parse time,
    /// so this function only needs to handle those two cases.
    pub(super) fn resolve_external_id(
        &self,
        expr: &executor::parser::Expression,
    ) -> Result<ExternalId> {
        use std::str::FromStr;
        let raw: String = match expr {
            executor::parser::Expression::Literal(executor::parser::Literal::String(s)) => {
                s.clone()
            }
            executor::parser::Expression::Parameter(name) => match self.current_params.get(name) {
                Some(Value::String(s)) => s.clone(),
                Some(other) => {
                    return Err(Error::executor(format!(
                        "_id parameter `{}` must be a string, got {:?}",
                        name, other
                    )));
                }
                None => {
                    return Err(Error::executor(format!(
                        "_id parameter `{}` not provided",
                        name
                    )));
                }
            },
            _ => {
                return Err(Error::executor(
                    "_id expression must be a string literal or parameter (parser invariant)",
                ));
            }
        };
        ExternalId::from_str(&raw)
            .map_err(|e| Error::executor(format!("invalid _id `{}`: {}", raw, e)))
    }

    pub(in crate::engine) fn execute_write_query(
        &mut self,
        ast: &executor::parser::CypherQuery,
    ) -> Result<executor::ResultSet> {
        // Drain any stale planner notifications that a prior query may have
        // deposited into the thread-local sink on this OS thread.  The read
        // path clears the sink inside `Executor::execute` before planning, but
        // `execute_write_query` bypasses the executor entirely — without this
        // drain a notification produced by a preceding query leaks into the
        // `ResultSet` we return here (the flaky
        // `engine_does_not_leak_notifications_across_consecutive_queries` test).
        // The discard is intentional: notifications for *this* query are
        // computed later by `compute_unindexed_property_access_notifications`
        // and appended fresh.
        let _ = crate::executor::planner::queries::drain_pending_planner_notifications();

        let mut context: HashMap<String, Vec<u64>> = HashMap::new();
        // Track relationship bindings: variable -> upserted (rel_id, rel_type)
        // entries (one per MERGE application — see #14).
        let mut rel_context: HashMap<String, Vec<(u64, String)>> = HashMap::new();
        let mut result: Option<executor::ResultSet> = None;

        // UNWIND-driven write (issue #13): `UNWIND list AS row <writes> RETURN`
        // runs the downstream write clauses once per row. Handled by a
        // dedicated path; the linear loop below stays the non-UNWIND fast path.
        if let Some(unwind_idx) = ast
            .clauses
            .iter()
            .position(|c| matches!(c, executor::parser::Clause::Unwind(_)))
        {
            return self.execute_unwind_write_query(ast, unwind_idx);
        }

        // Accurate "did this query mutate anything" signal for
        // `finalize_write_result`'s refresh-skip guard. CREATE/MERGE only
        // ever grow the record store (ids are never reused), so a node- or
        // relationship-count delta across the whole clause loop reliably
        // catches every node/relationship creation, including a MERGE that
        // fell through to its create branch — no per-clause bookkeeping
        // needed for that half. SET/REMOVE/FOREACH mutate properties or
        // labels in place (no id-count change), so those clauses set
        // `other_mutation` explicitly.
        let pre_node_count = self.storage.node_count();
        let pre_rel_count = self.storage.relationship_count();
        let mut other_mutation = false;

        for clause in &ast.clauses {
            match clause {
                executor::parser::Clause::Match(match_clause) => {
                    // Process all node patterns in the match clause
                    self.process_match_clause_multi(match_clause, &mut context, &mut rel_context)?;
                }
                // G2 — a CREATE clause in the SAME statement as a
                // following SET/REMOVE/RETURN (e.g. `CREATE (n:X {p:1})
                // REMOVE n.p`) must bind its variables into `context` /
                // `rel_context` just like MATCH/MERGE do, so the
                // downstream clause can resolve them. Previously this
                // arm fell through to the catch-all `_ => {}` below and
                // silently dropped the CREATE, leaving `n` unbound and
                // REMOVE erroring with "Unknown variable 'n'". Mirrors
                // the CREATE handling already present in the
                // UNWIND-write loop (`execute_unwind_write_query`),
                // extended here to also support relationship elements.
                executor::parser::Clause::Create(create_clause) => {
                    // `_id` (issue #29): resolved once per CREATE clause and
                    // consumed by only the FIRST node the pattern creates —
                    // the parser hoists `_id` out of that node's property
                    // map into `external_id_expr`, so any other node in the
                    // pattern (e.g. a relationship's target) must never
                    // receive it.
                    let ext_id = create_clause
                        .external_id_expr
                        .as_ref()
                        .map(|expr| self.resolve_external_id(expr))
                        .transpose()?;
                    let ext_policy = ast_conflict_policy_to_storage(create_clause.conflict_policy);
                    let mut ext_id_consumed = false;
                    let mut last_node_id: Option<u64> = None;
                    // Indices of pattern elements already materialised as a
                    // node by the `Relationship` arm below (it peeks
                    // `i + 1` and creates that target node itself to wire
                    // the edge). Without this, the loop's own `Node` arm
                    // fires again on the very same index right after,
                    // creating a second, unconnected copy and clobbering
                    // the variable binding / `last_node_id` with the
                    // orphan (the phantom-duplicate-target-node bug).
                    let mut consumed_node_indices: HashSet<usize> = HashSet::new();
                    for (i, element) in create_clause.pattern.elements.iter().enumerate() {
                        if consumed_node_indices.contains(&i) {
                            continue;
                        }
                        match element {
                            executor::parser::PatternElement::Node(node) => {
                                // Cross-clause / cross-element reuse: a bare
                                // reference to a variable already bound to
                                // EXACTLY ONE node — by an earlier MATCH or
                                // CREATE clause in this same statement, or by
                                // an earlier element of this very pattern —
                                // resolves to that node instead of minting a
                                // duplicate. Semantic validation already
                                // rejects re-declaring a bound variable with
                                // labels/properties/`_id`
                                // (`check_create_element_rebind`), so a bound
                                // `node.variable` reaching here is always the
                                // bare `(a)` shape with no structure to
                                // apply. A variable bound to MULTIPLE ids
                                // (e.g. a preceding comma-joined MATCH cross
                                // product) is NOT handled by this loop: the
                                // arm falls through, mints a fresh node from
                                // the bare `(a)` shape and overwrites the
                                // binding below — a pre-existing duplicate-
                                // node gap. Proper per-row fan-out belongs to
                                // a multi-row write path this single-pass
                                // clause loop does not implement, so this
                                // guard only short-circuits the unambiguous
                                // single-id case.
                                if let Some(var) = &node.variable {
                                    if let Some([existing_id]) = context.get(var).map(Vec::as_slice)
                                    {
                                        last_node_id = Some(*existing_id);
                                        continue;
                                    }
                                }
                                let mut props = Map::new();
                                if let Some(pm) = &node.properties {
                                    for (k, expr) in &pm.properties {
                                        props.insert(k.clone(), self.eval_write_value(expr)?);
                                    }
                                }
                                let node_ext_id = if ext_id_consumed {
                                    None
                                } else {
                                    ext_id_consumed = true;
                                    ext_id.clone()
                                };
                                let id = self.create_node_with_external_id(
                                    node.labels.clone(),
                                    Value::Object(props),
                                    node_ext_id,
                                    ext_policy,
                                )?;
                                if let Some(var) = &node.variable {
                                    context.insert(var.clone(), vec![id]);
                                }
                                last_node_id = Some(id);
                            }
                            executor::parser::PatternElement::Relationship(rel) => {
                                let source_id = last_node_id.ok_or_else(|| {
                                    Error::CypherExecution(
                                        "Relationship must follow a node".to_string(),
                                    )
                                })?;
                                let target_id = match create_clause.pattern.elements.get(i + 1) {
                                    Some(executor::parser::PatternElement::Node(target_node)) => {
                                        // Same cross-clause/cross-element
                                        // single-id reuse as the `Node` arm
                                        // above, applied to a relationship's
                                        // target endpoint — `(a)-[:R]->(b)`
                                        // must wire onto the ORIGINAL `b`
                                        // when it is already bound, not mint
                                        // an unconnected duplicate.
                                        let reused_id =
                                            target_node.variable.as_ref().and_then(|var| {
                                                match context.get(var).map(Vec::as_slice) {
                                                    Some([existing_id]) => Some(*existing_id),
                                                    _ => None,
                                                }
                                            });
                                        let tid = if let Some(existing_id) = reused_id {
                                            existing_id
                                        } else {
                                            let mut props = Map::new();
                                            if let Some(pm) = &target_node.properties {
                                                for (k, expr) in &pm.properties {
                                                    props.insert(
                                                        k.clone(),
                                                        self.eval_write_value(expr)?,
                                                    );
                                                }
                                            }
                                            let new_id = self.create_node(
                                                target_node.labels.clone(),
                                                Value::Object(props),
                                            )?;
                                            if let Some(var) = &target_node.variable {
                                                context.insert(var.clone(), vec![new_id]);
                                            }
                                            new_id
                                        };
                                        last_node_id = Some(tid);
                                        // Mark `i + 1` consumed so the loop's
                                        // own `Node` arm does not re-create
                                        // this same element on its next
                                        // iteration. For a chained pattern
                                        // like `(a)-[:R]->(b)-[:S]->(c)`, `b`
                                        // (index `i + 1` here) is also the
                                        // SOURCE of the next relationship —
                                        // `last_node_id` already points at
                                        // this connected node, so the next
                                        // `Relationship` arm picks it up
                                        // correctly without re-deriving it.
                                        consumed_node_indices.insert(i + 1);
                                        tid
                                    }
                                    _ => {
                                        return Err(Error::CypherExecution(
                                            "Relationship must be followed by a node".to_string(),
                                        ));
                                    }
                                };
                                let rel_type = rel.types.first().ok_or_else(|| {
                                    Error::CypherExecution(
                                        "Relationship must have a type".to_string(),
                                    )
                                })?;
                                let mut rel_props = Map::new();
                                if let Some(pm) = &rel.properties {
                                    for (k, expr) in &pm.properties {
                                        rel_props.insert(k.clone(), self.eval_write_value(expr)?);
                                    }
                                }
                                let rel_id = self.create_relationship(
                                    source_id,
                                    target_id,
                                    rel_type.clone(),
                                    Value::Object(rel_props),
                                )?;
                                if let Some(var) = &rel.variable {
                                    rel_context
                                        .entry(var.clone())
                                        .or_default()
                                        .push((rel_id, rel_type.clone()));
                                }
                            }
                            executor::parser::PatternElement::QuantifiedGroup(_) => {
                                return Err(Error::CypherExecution(
                                    "ERR_QPP_NOT_IN_CREATE: quantified path patterns \
                                     are read-only; use a MATCH clause instead"
                                        .to_string(),
                                ));
                            }
                        }
                    }
                }
                executor::parser::Clause::Merge(merge_clause) => {
                    // Check if this is a relationship MERGE with bound variables.
                    // A comma-joined `MATCH (a), (b) MERGE (a)-[:T]->(b)` yields
                    // one entry per (a, b) driving pair — see 4.10.
                    if let Some(rels) =
                        self.process_merge_relationship(&merge_clause, &mut context)?
                    {
                        for (rel_var, rel_id, rel_type) in rels {
                            // Empty `rel_var` is the anonymous-relationship
                            // sentinel (see `process_merge_relationship`) — it
                            // must never be bound into `rel_context`.
                            if !rel_var.is_empty() {
                                rel_context
                                    .entry(rel_var)
                                    .or_default()
                                    .push((rel_id, rel_type));
                            }
                        }
                    } else {
                        // Fall back to node MERGE
                        let (variable, node_ids) = self.process_merge_clause(merge_clause)?;
                        context.insert(variable, node_ids);
                    }
                }
                executor::parser::Clause::Set(set_clause) => {
                    self.apply_set_clause(&context, &rel_context, set_clause)?;
                    other_mutation = true;
                }
                executor::parser::Clause::Remove(remove_clause) => {
                    self.apply_remove_clause(&context, remove_clause)?;
                    other_mutation = true;
                }
                executor::parser::Clause::Foreach(foreach_clause) => {
                    self.execute_foreach_clause(&context, foreach_clause)?;
                    other_mutation = true;
                }
                executor::parser::Clause::Return(return_clause) => {
                    result = Some(self.build_return_result_with_rels(
                        &context,
                        &rel_context,
                        return_clause,
                    )?);
                }
                executor::parser::Clause::Where(where_clause) => {
                    // A free-standing `WHERE` after a MATCH — e.g.
                    // `MATCH ()-[r]->() WHERE id(r) = $x SET r.w = 1`.
                    // Narrows/binds the matched variables by id before the
                    // following SET/DELETE. Previously this errored outright.
                    self.apply_write_id_filter(
                        &where_clause.expression,
                        &mut context,
                        &mut rel_context,
                    )?;
                }
                executor::parser::Clause::With(with_clause) => {
                    // A `WITH` between write clauses is a SCOPE CUT: keep the
                    // variables it projects, drop the rest, optionally under a new
                    // name. For the write path's `variable -> ids` model that is
                    // directly representable — but only for a BARE variable
                    // projection. A projected property, expression or aggregation
                    // has no id list to carry forward, so it keeps erroring rather
                    // than silently dropping the projection.
                    //
                    // Before this, `With` sat in the catch-all below and made the
                    // whole query unexecutable: `MERGE (n…) WITH n MERGE (n2…)
                    // RETURN …` and `MATCH … WITH n, duration(…) AS d SET n.d = d`
                    // both died on "Unsupported clause in write query".
                    let mut kept: HashMap<String, Vec<u64>> = HashMap::new();
                    let mut kept_rels: HashMap<String, Vec<(u64, String)>> = HashMap::new();
                    for item in &with_clause.items {
                        let executor::parser::Expression::Variable(name) = &item.expression else {
                            return Err(Error::CypherExecution(format!(
                                "WITH in a write query supports bare variable projections only                                  (`WITH n`, `WITH n AS m`); `{}` projects an expression, which                                  has no binding to carry forward",
                                item.alias.clone().unwrap_or_else(|| "<expr>".to_string())
                            )));
                        };
                        let out_name = item.alias.clone().unwrap_or_else(|| name.clone());
                        if let Some(ids) = context.get(name) {
                            kept.insert(out_name.clone(), ids.clone());
                        }
                        if let Some(rels) = rel_context.get(name) {
                            kept_rels.insert(out_name, rels.clone());
                        }
                    }
                    context = kept;
                    rel_context = kept_rels;
                }
                executor::parser::Clause::Unwind(_)
                | executor::parser::Clause::Union(_)
                | executor::parser::Clause::OrderBy(_)
                | executor::parser::Clause::Limit(_)
                | executor::parser::Clause::Skip(_) => {
                    return Err(Error::CypherExecution(
                        "Unsupported clause in write query".to_string(),
                    ));
                }
                _ => {}
            }
        }

        let mutated = other_mutation
            || self.storage.node_count() != pre_node_count
            || self.storage.relationship_count() != pre_rel_count;
        self.finalize_write_result(result, ast, mutated)
    }

    /// Shared tail for the write-query paths: async-flush, refresh the
    /// executor against the new storage state, and attach the write-path
    /// `Nexus.Performance.UnindexedPropertyAccess` diagnostic. Used by both
    /// the linear `execute_write_query` loop and the UNWIND-write path.
    ///
    /// `mutated` is the caller's accurately-computed "did this write
    /// actually change anything" signal — see
    /// [`Engine::refresh_executor_if_mutated`] for why it is a plain
    /// `bool` and not a [`executor::types::SideEffects`]. Passing `true`
    /// unconditionally reproduces the previous always-refresh behaviour.
    pub(super) fn finalize_write_result(
        &mut self,
        result: Option<executor::ResultSet>,
        ast: &executor::parser::CypherQuery,
        mutated: bool,
    ) -> Result<executor::ResultSet> {
        // Async flush — matches the CREATE / executor-side write paths,
        // which use `flush_async` as well. The SYNC `flush()` here used
        // to dominate write-query latency (5-10ms per call on spinning
        // media; 2-3ms even on NVMe) because mmap page syncs are
        // OS-level operations. With the WAL already providing
        // durability on commit, this full sync is redundant on the hot
        // path. Callers that genuinely need on-disk durability can issue
        // an explicit `flush()` after the write.
        self.storage.flush_async()?;
        self.refresh_executor_if_mutated(mutated)?;

        // Diagnostic pre-pass for the write path: MERGE/SET/REMOVE
        // bypass the planner entirely, so the planner-side
        // `Nexus.Performance.UnindexedPropertyAccess` notification
        // never fires here. Run the same scan against the engine's
        // catalog + property-index registry and attach any
        // notifications to the returned `ResultSet`.
        let mut rs = result.unwrap_or_else(|| executor::ResultSet::new(vec![], vec![]));
        let notes =
            crate::executor::planner::queries::compute_unindexed_property_access_notifications(
                &self.catalog,
                &self.indexes.property_index,
                ast,
            );
        if !notes.is_empty() {
            rs.notifications.extend(notes);
        }
        Ok(rs)
    }

    /// Evaluate an expression to a `serde_json::Value` for the write path,
    /// supporting list/map literals and (via `expression_to_json_value`)
    /// scalar literals plus UNWIND row bindings (`row` / `row.id`). Used to
    /// materialise the UNWIND list and per-row map values (issue #13).
    pub(super) fn eval_write_value(
        &self,
        expr: &executor::parser::Expression,
    ) -> Result<serde_json::Value> {
        match expr {
            executor::parser::Expression::Map(entries) => {
                let mut m = serde_json::Map::with_capacity(entries.len());
                for (k, v) in entries.iter() {
                    m.insert(k.clone(), self.eval_write_value(v)?);
                }
                Ok(serde_json::Value::Object(m))
            }
            executor::parser::Expression::List(items) => {
                let mut a = Vec::with_capacity(items.len());
                for it in items {
                    a.push(self.eval_write_value(it)?);
                }
                Ok(serde_json::Value::Array(a))
            }
            // B6 — `UNWIND $rows AS row` (and any other write-path position
            // that materialises a full value via `eval_write_value`, e.g. a
            // MERGE-relationship inline property) resolves the parameter
            // against `self.current_params`. A missing parameter is a clear
            // client error rather than silently degrading to an empty
            // UNWIND list.
            executor::parser::Expression::Parameter(name) => {
                self.current_params.get(name).cloned().ok_or_else(|| {
                    Error::CypherExecution(format!("Parameter `${name}` was not provided"))
                })
            }
            // Scalars + UNWIND row bindings (Variable / PropertyAccess) are
            // handled here.
            _ => self.expression_to_json_value(expr),
        }
    }

    /// Execute an `UNWIND list AS var <write clauses> [RETURN ...]` write
    /// query by running the post-UNWIND write clauses once per list item,
    /// binding `var` to the item for the iteration (issue #13). Only `MATCH`
    /// may precede the `UNWIND`; the post-UNWIND clauses may be
    /// MERGE / SET / REMOVE / FOREACH (+ a trailing RETURN).
    pub(super) fn execute_unwind_write_query(
        &mut self,
        ast: &executor::parser::CypherQuery,
        unwind_idx: usize,
    ) -> Result<executor::ResultSet> {
        use executor::parser::Clause;

        // `base_context` holds bindings from any leading MATCH (shared by
        // every row). `accumulated` collects the node ids written across all
        // rows for the trailing RETURN/count. Each row runs its write clauses
        // against a *fresh per-row context* so a `SET` only touches that row's
        // node, not every node merged so far.
        let mut base_context: HashMap<String, Vec<u64>> = HashMap::new();
        let mut accumulated: HashMap<String, Vec<u64>> = HashMap::new();
        // #14: accumulates ONE entry per row so a trailing `RETURN count(r)`
        // reflects every row's upserted edge, not just the last one.
        let mut rel_context: HashMap<String, Vec<(u64, String)>> = HashMap::new();

        // Clauses before UNWIND run once (e.g. a leading MATCH).
        for clause in &ast.clauses[..unwind_idx] {
            match clause {
                Clause::Match(mc) => {
                    self.process_match_clause_multi(mc, &mut base_context, &mut rel_context)?
                }
                _ => {
                    return Err(Error::CypherExecution(
                        "Only MATCH may precede UNWIND in a write query".to_string(),
                    ));
                }
            }
        }

        let unwind = match &ast.clauses[unwind_idx] {
            Clause::Unwind(u) => u,
            _ => unreachable!("unwind_idx points at a non-UNWIND clause"),
        };
        let items = match self.eval_write_value(&unwind.expression)? {
            serde_json::Value::Array(a) => a,
            serde_json::Value::Null => Vec::new(),
            // Neo4j unwinds a non-list scalar as a single row.
            other => vec![other],
        };

        let post = &ast.clauses[unwind_idx + 1..];

        // `_id` (issue #29): resolved ONCE, before the per-row loop below.
        // `create_clause.external_id_expr` cannot vary per row — a per-row
        // `_id` (e.g. `_id: row.id`) is a parse error today (out of scope
        // here) — so resolving it here is equivalent to resolving it
        // inside the loop, but avoids a `?`-propagating early return from
        // inside the loop body that would skip the manual
        // `self.unwind_bindings.clear()` cleanup every other early-return
        // arm below performs.
        let create_ext_id = post
            .iter()
            .find_map(|c| match c {
                Clause::Create(cc) => cc.external_id_expr.as_ref(),
                _ => None,
            })
            .map(|expr| self.resolve_external_id(expr))
            .transpose()?;
        let create_ext_policy = post
            .iter()
            .find_map(|c| match c {
                Clause::Create(cc) => Some(cc.conflict_policy),
                _ => None,
            })
            .map(ast_conflict_policy_to_storage)
            .unwrap_or(ConflictPolicy::Error);

        // Same accurate mutation signal as the linear
        // `execute_write_query` loop (see its comment): a node/relationship
        // count delta across every row catches every CREATE/MERGE-created
        // entity, while SET/REMOVE/FOREACH set `other_mutation` explicitly.
        let pre_node_count = self.storage.node_count();
        let pre_rel_count = self.storage.relationship_count();
        let mut other_mutation = false;

        for item in items {
            self.unwind_bindings.insert(unwind.variable.clone(), item);
            // Fresh per-row context seeded from the shared MATCH bindings, so
            // SET/REMOVE only touch the node(s) this row merged/matched.
            let mut row_context = base_context.clone();
            for clause in post {
                match clause {
                    Clause::Merge(merge_clause) => {
                        if let Some(rels) =
                            self.process_merge_relationship(merge_clause, &mut row_context)?
                        {
                            for (rel_var, rel_id, rel_type) in rels {
                                // Empty `rel_var` is the anonymous-relationship
                                // sentinel (see `process_merge_relationship`) —
                                // it must never be bound into `rel_context`.
                                if !rel_var.is_empty() {
                                    rel_context
                                        .entry(rel_var)
                                        .or_default()
                                        .push((rel_id, rel_type));
                                }
                            }
                        } else {
                            let (variable, node_ids) = self.process_merge_clause(merge_clause)?;
                            row_context.insert(variable.clone(), node_ids.clone());
                            accumulated.entry(variable).or_default().extend(node_ids);
                        }
                    }
                    Clause::Create(create_clause) => {
                        // Consumed by only the FIRST node this pattern
                        // creates — same "one _id, first node only"
                        // contract as the linear CREATE arm above.
                        let mut ext_id_consumed = false;
                        for element in &create_clause.pattern.elements {
                            match element {
                                executor::parser::PatternElement::Node(node) => {
                                    let mut props = serde_json::Map::new();
                                    if let Some(pm) = &node.properties {
                                        for (k, expr) in &pm.properties {
                                            props.insert(k.clone(), self.eval_write_value(expr)?);
                                        }
                                    }
                                    let node_ext_id = if ext_id_consumed {
                                        None
                                    } else {
                                        ext_id_consumed = true;
                                        create_ext_id.clone()
                                    };
                                    let id = self.create_node_with_external_id(
                                        node.labels.clone(),
                                        serde_json::Value::Object(props),
                                        node_ext_id,
                                        create_ext_policy,
                                    )?;
                                    if let Some(var) = &node.variable {
                                        row_context.insert(var.clone(), vec![id]);
                                        accumulated.entry(var.clone()).or_default().push(id);
                                    }
                                }
                                _ => {
                                    self.unwind_bindings.clear();
                                    return Err(Error::CypherExecution(
                                        "Relationship CREATE inside UNWIND is not supported; \
                                         use separate MERGE clauses for the endpoints and edge"
                                            .to_string(),
                                    ));
                                }
                            }
                        }
                    }
                    Clause::Set(set_clause) => {
                        self.apply_set_clause(&row_context, &rel_context, set_clause)?;
                        other_mutation = true;
                    }
                    Clause::Remove(remove_clause) => {
                        self.apply_remove_clause(&row_context, remove_clause)?;
                        other_mutation = true;
                    }
                    Clause::Foreach(foreach_clause) => {
                        self.execute_foreach_clause(&row_context, foreach_clause)?;
                        other_mutation = true;
                    }
                    // #14: per-row MATCH — resolves endpoints like
                    // `MATCH (a {id: row.fk}), (b {id: row.tk})` into the row
                    // context so a following relationship MERGE upserts the
                    // edge for every row (the edge analogue of the #13 node fix).
                    // `find_nodes_by_node_pattern` resolves `row.*` via the
                    // active unwind binding.
                    Clause::Match(match_clause) => {
                        self.process_match_clause_multi(
                            match_clause,
                            &mut row_context,
                            &mut rel_context,
                        )?;
                    }
                    // RETURN is computed once after the loop.
                    Clause::Return(_) => {}
                    Clause::Where(_)
                    | Clause::With(_)
                    | Clause::Unwind(_)
                    | Clause::Union(_)
                    | Clause::OrderBy(_)
                    | Clause::Limit(_)
                    | Clause::Skip(_) => {
                        self.unwind_bindings.clear();
                        return Err(Error::CypherExecution(
                            "Unsupported clause after UNWIND in write query".to_string(),
                        ));
                    }
                    _ => {}
                }
            }
        }
        self.unwind_bindings.clear();

        // Merge the leading-MATCH bindings with the accumulated per-row writes
        // into the RETURN context, with stable de-duplicated id lists so a
        // trailing `RETURN count(n)` reflects every distinct row written.
        let mut return_context = base_context;
        for (variable, ids) in accumulated {
            return_context.entry(variable).or_default().extend(ids);
        }
        for ids in return_context.values_mut() {
            ids.sort_unstable();
            ids.dedup();
        }

        let mutated = other_mutation
            || self.storage.node_count() != pre_node_count
            || self.storage.relationship_count() != pre_rel_count;

        // Build the trailing RETURN (if any) after flush+refresh so the
        // executor-backed projection sees the freshly written rows.
        self.storage.flush_async()?;
        self.refresh_executor_if_mutated(mutated)?;
        let result = post
            .iter()
            .find_map(|c| match c {
                Clause::Return(r) => Some(r),
                _ => None,
            })
            .map(|return_clause| {
                self.build_return_result_with_rels(&return_context, &rel_context, return_clause)
            })
            .transpose()?;

        // Reuse the shared notification tail (flush/refresh are idempotent).
        self.finalize_write_result(result, ast, mutated)
    }

    pub(super) fn process_match_clause(
        &mut self,
        match_clause: &executor::parser::MatchClause,
    ) -> Result<(String, Vec<u64>)> {
        if match_clause.optional {
            return Err(Error::CypherExecution(
                "OPTIONAL MATCH not supported in write queries".to_string(),
            ));
        }

        if match_clause.where_clause.is_some() {
            return Err(Error::CypherExecution(
                "MATCH with WHERE is not supported in write queries".to_string(),
            ));
        }

        let node_pattern = match_clause
            .pattern
            .elements
            .iter()
            .find_map(|element| {
                if let executor::parser::PatternElement::Node(node) = element {
                    Some(node.clone())
                } else {
                    None
                }
            })
            .ok_or_else(|| Error::CypherExecution("MATCH requires a node pattern".to_string()))?;

        let variable = node_pattern
            .variable
            .clone()
            .ok_or_else(|| Error::CypherExecution("MATCH requires a variable alias".to_string()))?;

        let mut node_ids = self.find_nodes_by_node_pattern(&node_pattern)?;
        node_ids.sort_unstable();
        node_ids.dedup();

        Ok((variable, node_ids))
    }

    /// Process all node patterns in a MATCH clause (for multi-node patterns like (a), (b))
    /// Apply a write-query `WHERE` predicate to the already-matched
    /// bindings. Supports the `id(var) = <value>` shape and `AND`-chains of
    /// it — how a client targets one relationship (or node) by id for a
    /// following `SET` / `DELETE` (e.g. `MATCH ()-[r]->() WHERE id(r) = $x
    /// SET r.w = 1`). Binds an as-yet-unbound `var` (the untyped
    /// `()-[r]->()` case, which the pattern binder skips) to the entity
    /// with that id, and narrows an already-bound `var` to the matching id.
    /// Any other predicate shape errors — the write path rejected every
    /// `WHERE` before this, so it can only ever widen what succeeds.
    fn apply_write_id_filter(
        &mut self,
        predicate: &executor::parser::Expression,
        context: &mut HashMap<String, Vec<u64>>,
        rel_context: &mut HashMap<String, Vec<(u64, String)>>,
    ) -> Result<()> {
        use executor::parser::{BinaryOperator, Expression};
        match predicate {
            Expression::BinaryOp {
                left,
                op: BinaryOperator::And,
                right,
            } => {
                self.apply_write_id_filter(left, context, rel_context)?;
                self.apply_write_id_filter(right, context, rel_context)
            }
            Expression::BinaryOp {
                left,
                op: BinaryOperator::Equal,
                right,
            } => {
                // Accept `id(var) = expr` or `expr = id(var)`.
                let (var, value_expr) = match (Self::as_id_var(left), Self::as_id_var(right)) {
                    (Some(v), _) => (v, right.as_ref()),
                    (_, Some(v)) => (v, left.as_ref()),
                    _ => {
                        return Err(Error::CypherExecution(
                            "WHERE in a write query only supports `id(var) = <value>`".to_string(),
                        ));
                    }
                };
                let id = self.eval_write_value(value_expr)?.as_u64().ok_or_else(|| {
                    Error::CypherExecution(
                        "id(...) comparison value must be an integer".to_string(),
                    )
                })?;
                self.bind_or_filter_by_id(var, id, context, rel_context)
            }
            _ => Err(Error::CypherExecution(
                "WHERE in a write query only supports `id(var) = <value>` predicates".to_string(),
            )),
        }
    }

    /// Extract `v` from an `id(v)` function-call expression.
    fn as_id_var(expr: &executor::parser::Expression) -> Option<String> {
        use executor::parser::Expression;
        if let Expression::FunctionCall { name, args } = expr {
            if name.eq_ignore_ascii_case("id") && args.len() == 1 {
                if let Expression::Variable(v) = &args[0] {
                    return Some(v.clone());
                }
            }
        }
        None
    }

    /// Bind `var` to the entity with `id` — resolving an unbound
    /// relationship by id — or filter an already-bound `var` to that id.
    fn bind_or_filter_by_id(
        &mut self,
        var: String,
        id: u64,
        context: &mut HashMap<String, Vec<u64>>,
        rel_context: &mut HashMap<String, Vec<(u64, String)>>,
    ) -> Result<()> {
        if let Some(rels) = rel_context.get_mut(&var) {
            rels.retain(|(rid, _)| *rid == id);
            return Ok(());
        }
        if let Some(nodes) = context.get_mut(&var) {
            nodes.retain(|n| *n == id);
            return Ok(());
        }
        // Unbound — e.g. an untyped `()-[r]->()` whose relationship the
        // pattern binder skipped (it needs a type). Resolve it by id.
        if let Some(record) = self.get_relationship(id)? {
            let type_name = self
                .catalog
                .get_type_name(record.type_id)?
                .unwrap_or_default();
            rel_context.insert(var, vec![(id, type_name)]);
        } else {
            // No such relationship: leave the binding empty so the
            // downstream SET/DELETE/RETURN is a no-op rather than an error.
            rel_context.insert(var, Vec::new());
        }
        Ok(())
    }

    pub(super) fn process_match_clause_multi(
        &mut self,
        match_clause: &executor::parser::MatchClause,
        context: &mut HashMap<String, Vec<u64>>,
        rel_context: &mut HashMap<String, Vec<(u64, String)>>,
    ) -> Result<()> {
        if match_clause.optional {
            return Err(Error::CypherExecution(
                "OPTIONAL MATCH not supported in write queries".to_string(),
            ));
        }

        // Process all node patterns in the pattern
        for element in &match_clause.pattern.elements {
            if let executor::parser::PatternElement::Node(node_pattern) = element {
                if let Some(variable) = &node_pattern.variable {
                    let mut node_ids = self.find_nodes_by_node_pattern(node_pattern)?;
                    node_ids.sort_unstable();
                    node_ids.dedup();
                    context.insert(variable.clone(), node_ids);
                }
            }
        }

        // Bind matched relationship variables (#25) so a following
        // `SET r.k = v` can resolve `r`. For each `(left)-[r:T]->(right)`
        // triple whose endpoints are bound, resolve the relationship(s) of
        // type T between them (honouring direction) and bind `r`.
        use executor::parser::{PatternElement, RelationshipDirection};
        let elements = &match_clause.pattern.elements;
        for i in 0..elements.len() {
            let PatternElement::Relationship(rel) = &elements[i] else {
                continue;
            };
            let (Some(rel_var), Some(rel_type)) = (&rel.variable, rel.types.first()) else {
                continue;
            };
            if i == 0 || i + 1 >= elements.len() {
                continue;
            }
            let (PatternElement::Node(left), PatternElement::Node(right)) =
                (&elements[i - 1], &elements[i + 1])
            else {
                continue;
            };
            // Resolve each endpoint's node ids: from the bound context when it
            // has a variable, otherwise by matching the node pattern directly
            // so anonymous endpoints with label/property filters still work
            // (e.g. `MATCH (:P {id:'e'})-[r:T]->(:P {id:'f'}) SET r.k = v`).
            let left_ids = match &left.variable {
                Some(v) if context.contains_key(v) => context.get(v).cloned().unwrap_or_default(),
                _ => self.find_nodes_by_node_pattern(left)?,
            };
            let right_ids = match &right.variable {
                Some(v) if context.contains_key(v) => context.get(v).cloned().unwrap_or_default(),
                _ => self.find_nodes_by_node_pattern(right)?,
            };
            // Resolve (src, dst) endpoints by direction. `Both` is treated as
            // outgoing-then-reverse below.
            let (src_ids, dst_ids) = match rel.direction {
                RelationshipDirection::Incoming => (&right_ids, &left_ids),
                _ => (&left_ids, &right_ids),
            };
            let mut found: Vec<(u64, String)> = Vec::new();
            for &s in src_ids {
                for &d in dst_ids {
                    if let Some(rid) = self.find_relationship_between(s, d, rel_type)? {
                        found.push((rid, rel_type.clone()));
                    }
                    if matches!(rel.direction, RelationshipDirection::Both) {
                        if let Some(rid) = self.find_relationship_between(d, s, rel_type)? {
                            found.push((rid, rel_type.clone()));
                        }
                    }
                }
            }
            found.sort_unstable_by_key(|(id, _)| *id);
            found.dedup_by_key(|(id, _)| *id);
            if !found.is_empty() {
                rel_context
                    .entry(rel_var.clone())
                    .or_default()
                    .extend(found);
            }
        }

        // Apply a WHERE attached to this MATCH (e.g. `MATCH (n) WHERE
        // id(n) = $x`) after the pattern bound its variables. The
        // free-standing `WHERE` form (a separate clause) is handled by the
        // clause loop; both funnel through the same id-filter.
        if let Some(where_clause) = &match_clause.where_clause {
            self.apply_write_id_filter(&where_clause.expression, context, rel_context)?;
        }

        Ok(())
    }
}
