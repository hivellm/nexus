//! Write-query execution: MERGE / SET / REMOVE / FOREACH / UNWIND-write.
//! Extracted from `engine/mod.rs`.

use super::Engine;
use super::crud::NodeWriteState;
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
    fn resolve_external_id(&self, expr: &executor::parser::Expression) -> Result<ExternalId> {
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

    pub(super) fn execute_write_query(
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
                    for (i, element) in create_clause.pattern.elements.iter().enumerate() {
                        match element {
                            executor::parser::PatternElement::Node(node) => {
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
                                        let mut props = Map::new();
                                        if let Some(pm) = &target_node.properties {
                                            for (k, expr) in &pm.properties {
                                                props.insert(
                                                    k.clone(),
                                                    self.eval_write_value(expr)?,
                                                );
                                            }
                                        }
                                        let tid = self.create_node(
                                            target_node.labels.clone(),
                                            Value::Object(props),
                                        )?;
                                        if let Some(var) = &target_node.variable {
                                            context.insert(var.clone(), vec![tid]);
                                        }
                                        last_node_id = Some(tid);
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
                    // Check if this is a relationship MERGE with bound variables
                    if let Some((rel_var, rel_id, rel_type)) =
                        self.process_merge_relationship(&merge_clause, &mut context)?
                    {
                        rel_context
                            .entry(rel_var)
                            .or_default()
                            .push((rel_id, rel_type));
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
                executor::parser::Clause::Where(_)
                | executor::parser::Clause::With(_)
                | executor::parser::Clause::Unwind(_)
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
                        if let Some((rel_var, rel_id, rel_type)) =
                            self.process_merge_relationship(merge_clause, &mut row_context)?
                        {
                            rel_context
                                .entry(rel_var)
                                .or_default()
                                .push((rel_id, rel_type));
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

    pub(super) fn process_merge_clause(
        &mut self,
        merge_clause: &executor::parser::MergeClause,
    ) -> Result<(String, Vec<u64>)> {
        let node_pattern = merge_clause
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
            .ok_or_else(|| Error::CypherExecution("MERGE requires a node pattern".to_string()))?;

        let variable = node_pattern
            .variable
            .clone()
            .ok_or_else(|| Error::CypherExecution("MERGE requires a variable alias".to_string()))?;

        // Null-key contract (Neo4j parity): MERGE cannot use a null property
        // value. Reject before match-or-create so behaviour is identical
        // whether or not an existing node would match. Mirrors Neo4j's
        // "Cannot merge node using null property value for <key>".
        if let Some(prop_map) = &node_pattern.properties {
            for (key, expr) in &prop_map.properties {
                if matches!(
                    self.expression_to_json_value(expr)?,
                    serde_json::Value::Null
                ) {
                    return Err(Error::CypherExecution(format!(
                        "Cannot merge node using null property value for {key}"
                    )));
                }
            }
        }

        // `_id` (issue #29): resolve the magic `_id` property the parser
        // hoisted out of `node_pattern.properties` into `external_id_expr`.
        // The external id is a stronger key than the property-based search
        // below — the search is now `_id`-blind, since the parser already
        // stripped `_id` out of `node_pattern.properties` — so a hit here
        // short-circuits that search entirely.
        let ext_id = merge_clause
            .external_id_expr
            .as_ref()
            .map(|expr| self.resolve_external_id(expr))
            .transpose()?;

        let existing_by_ext_id = if let Some(ext) = &ext_id {
            let txn = self.catalog.read_txn()?;
            let found = self.catalog.external_id_index().get_internal(&txn, ext)?;
            drop(txn);
            found
        } else {
            None
        };

        let mut node_ids = if let Some(id) = existing_by_ext_id {
            vec![id]
        } else {
            let mut ids = self.find_nodes_by_node_pattern(&node_pattern)?;
            ids.sort_unstable();
            ids.dedup();
            ids
        };

        if node_ids.is_empty() {
            let labels = node_pattern.labels.clone();
            let mut props = Map::new();
            if let Some(prop_map) = &node_pattern.properties {
                for (key, expr) in &prop_map.properties {
                    let value = self.expression_to_json_value(expr)?;
                    props.insert(key.clone(), value);
                }
            }
            // create_node_with_external_id already checks constraints, so
            // we can call it directly. `ConflictPolicy::Match` closes the
            // TOCTOU window between the `existing_by_ext_id` lookup above
            // and this create: if a concurrent MERGE raced in and won,
            // this falls back to the now-existing internal id instead of
            // erroring (`MergeClause` has no `conflict_policy` of its own —
            // find-or-create is always the semantics here).
            let node_id = self.create_node_with_external_id(
                labels,
                Value::Object(props),
                ext_id,
                ConflictPolicy::Match,
            )?;
            node_ids.push(node_id);

            if let Some(on_create) = &merge_clause.on_create {
                let mut ctx = HashMap::new();
                ctx.insert(variable.clone(), vec![node_id]);
                self.apply_set_clause(&ctx, &HashMap::new(), on_create)?;
            }
        } else if let Some(on_match) = &merge_clause.on_match {
            let mut ctx = HashMap::new();
            ctx.insert(variable.clone(), node_ids.clone());
            self.apply_set_clause(&ctx, &HashMap::new(), on_match)?;
        }

        Ok((variable, node_ids))
    }

    /// MERGE (find-or-create) a single node pattern with no `ON
    /// CREATE`/`ON MATCH` handling of its own (#25/G3). Used by
    /// [`Self::process_merge_relationship`] to resolve the endpoint nodes
    /// of a standalone relationship-MERGE pattern (`MERGE (a:L1 {..})-[r:T]->(b:L2
    /// {..})`) that has no preceding MATCH to bind them — `ON CREATE` /
    /// `ON MATCH` on the enclosing `MergeClause` still targets the
    /// relationship only, per the existing `process_merge_relationship`
    /// contract; mirrors the match-or-create logic in
    /// [`Self::process_merge_clause`] (including its `_id` external-id
    /// fast path) minus that per-clause SET handling.
    pub(super) fn merge_single_node(
        &mut self,
        node_pattern: &executor::parser::NodePattern,
    ) -> Result<u64> {
        // Null-key contract (Neo4j parity): MERGE cannot use a null
        // property value.
        if let Some(prop_map) = &node_pattern.properties {
            for (key, expr) in &prop_map.properties {
                if matches!(
                    self.expression_to_json_value(expr)?,
                    serde_json::Value::Null
                ) {
                    return Err(Error::CypherExecution(format!(
                        "Cannot merge node using null property value for {key}"
                    )));
                }
            }
        }

        // `_id` (relationship-MERGE endpoints): resolve the per-node `_id`
        // the parser hoisted out of `node_pattern.properties` into
        // `external_id_expr` (populated per-node for MERGE patterns — see
        // `extract_underscore_id_from_pattern`). A hit in the external-id
        // index short-circuits the property-based search below, mirroring
        // `Self::process_merge_clause`.
        let ext_id = node_pattern
            .external_id_expr
            .as_ref()
            .map(|expr| self.resolve_external_id(expr))
            .transpose()?;

        let existing_by_ext_id = if let Some(ext) = &ext_id {
            let txn = self.catalog.read_txn()?;
            let found = self.catalog.external_id_index().get_internal(&txn, ext)?;
            drop(txn);
            found
        } else {
            None
        };

        if let Some(id) = existing_by_ext_id {
            return Ok(id);
        }

        let mut node_ids = self.find_nodes_by_node_pattern(node_pattern)?;
        node_ids.sort_unstable();
        node_ids.dedup();

        if let Some(&id) = node_ids.first() {
            return Ok(id);
        }

        let labels = node_pattern.labels.clone();
        let mut props = Map::new();
        if let Some(prop_map) = &node_pattern.properties {
            for (key, expr) in &prop_map.properties {
                let value = self.expression_to_json_value(expr)?;
                props.insert(key.clone(), value);
            }
        }
        // `ConflictPolicy::Match` closes the TOCTOU window between the
        // `existing_by_ext_id` lookup above and this create — mirrors
        // `Self::process_merge_clause`. When `ext_id` is `None` this
        // behaves identically to a plain `create_node`: `create_node_inner`
        // only takes the external-id path when an id is actually supplied.
        self.create_node_with_external_id(
            labels,
            Value::Object(props),
            ext_id,
            ConflictPolicy::Match,
        )
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

        if match_clause.where_clause.is_some() {
            return Err(Error::CypherExecution(
                "MATCH with WHERE is not supported in write queries".to_string(),
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

        Ok(())
    }

    /// Process MERGE with relationship pattern when nodes are already bound
    /// Returns Some((rel_variable, rel_id, rel_type)) if this is a relationship MERGE
    pub(super) fn process_merge_relationship(
        &mut self,
        merge_clause: &executor::parser::MergeClause,
        context: &mut HashMap<String, Vec<u64>>,
    ) -> Result<Option<(String, u64, String)>> {
        // Check if pattern has: Node, Relationship, Node structure
        let elements = &merge_clause.pattern.elements;
        if elements.len() != 3 {
            return Ok(None);
        }

        // Extract source node, relationship, and target node
        let src_node = match &elements[0] {
            executor::parser::PatternElement::Node(n) => n,
            _ => return Ok(None),
        };
        let rel_pattern = match &elements[1] {
            executor::parser::PatternElement::Relationship(r) => r,
            _ => return Ok(None),
        };
        let dst_node = match &elements[2] {
            executor::parser::PatternElement::Node(n) => n,
            _ => return Ok(None),
        };

        // Get source and destination variable names
        let src_var = match &src_node.variable {
            Some(v) => v.clone(),
            None => return Ok(None),
        };
        let dst_var = match &dst_node.variable {
            Some(v) => v.clone(),
            None => return Ok(None),
        };

        // Get relationship variable and type
        let rel_var = match &rel_pattern.variable {
            Some(v) => v.clone(),
            None => return Ok(None),
        };
        let rel_type = match rel_pattern.types.first() {
            Some(t) => t.clone(),
            None => return Ok(None),
        };

        // G3 — resolve source/destination node ids. When an enclosing
        // MATCH/UNWIND already bound the variable (existing contract:
        // present in `context` with a non-empty id list), reuse it
        // as-is. When the variable is bound but resolved to ZERO nodes
        // (e.g. a MATCH that found nothing), preserve the prior
        // behaviour of bailing out to the node-only MERGE fallback.
        // When the variable is not in `context` at all, this is a
        // STANDALONE relationship-MERGE pattern with no preceding MATCH
        // (`MERGE (a:L1 {..})-[r:T]->(b:L2 {..})`, harness cases 10/11)
        // — MERGE (find-or-create) the endpoint node inline and bind it,
        // so the path pattern resolves its own endpoints instead of
        // silently requiring an upstream MATCH.
        let src_id = match context.get(&src_var) {
            Some(ids) if !ids.is_empty() => ids[0],
            Some(_) => return Ok(None),
            None => {
                let id = self.merge_single_node(src_node)?;
                context.insert(src_var.clone(), vec![id]);
                id
            }
        };
        let dst_id = match context.get(&dst_var) {
            Some(ids) if !ids.is_empty() => ids[0],
            Some(_) => return Ok(None),
            None => {
                let id = self.merge_single_node(dst_node)?;
                context.insert(dst_var.clone(), vec![id]);
                id
            }
        };

        // Check if relationship already exists
        let existing_rel = self.find_relationship_between(src_id, dst_id, &rel_type)?;

        let rel_id = if let Some(rid) = existing_rel {
            // Relationship exists — apply ON MATCH SET to its properties (#14).
            if let Some(on_match) = &merge_clause.on_match {
                self.apply_merge_rel_set(&rel_var, rid, on_match)?;
            }
            rid
        } else {
            // Create the relationship with the pattern's inline properties
            // (#25 — previously dropped: a hardcoded empty map was used, so
            // `MERGE (a)-[r:T {k:v}]->(b)` created a propless edge), then
            // layer ON CREATE SET on top (which may override them). Uses
            // `eval_write_value` so inline props resolve UNWIND `row.*`
            // bindings on the per-row MERGE path.
            let mut props_map = Map::new();
            if let Some(prop_map) = &rel_pattern.properties {
                for (key, expr) in &prop_map.properties {
                    props_map.insert(key.clone(), self.eval_write_value(expr)?);
                }
            }
            let new_rel_id = self.create_relationship(
                src_id,
                dst_id,
                rel_type.clone(),
                Value::Object(props_map),
            )?;
            if let Some(on_create) = &merge_clause.on_create {
                self.apply_merge_rel_set(&rel_var, new_rel_id, on_create)?;
            }
            new_rel_id
        };

        Ok(Some((rel_var, rel_id, rel_type)))
    }

    /// Apply a MERGE `ON CREATE` / `ON MATCH SET` clause to a relationship's
    /// properties (#14). Only `SetItem::Property` assignments whose target is
    /// the relationship variable are applied; the RHS is evaluated with
    /// `evaluate_set_expression`, which resolves UNWIND row bindings (e.g.
    /// `SET r.w = row.w`) and `r.<prop>` self-references against the rel's
    /// current properties. Other SET item kinds are ignored for relationships.
    pub(super) fn apply_merge_rel_set(
        &mut self,
        rel_var: &str,
        rel_id: u64,
        set_clause: &executor::parser::SetClause,
    ) -> Result<()> {
        let mut props: Map<String, Value> = self
            .storage
            .load_relationship_properties(rel_id)?
            .and_then(|v| match v {
                Value::Object(m) => Some(m),
                _ => None,
            })
            .unwrap_or_default();

        let mut changed = false;
        for item in &set_clause.items {
            if let executor::parser::SetItem::Property {
                target,
                property,
                value,
            } = item
            {
                if target != rel_var {
                    continue;
                }
                let v = self.evaluate_set_expression(value, rel_var, &props)?;
                props.insert(property.clone(), v);
                changed = true;
            }
        }

        if changed {
            let props_value = Value::Object(props);
            // Register every property key with the catalog so
            // `db.propertyKeys()` sees keys written via MERGE ON CREATE/ON
            // MATCH SET on a relationship. See `Catalog::register_property_keys`.
            self.catalog.register_property_keys(&props_value);
            self.storage
                .update_relationship_properties(rel_id, props_value)?;
        }
        Ok(())
    }

    /// Apply a single `SET <rel>.<property> = <value>` to one relationship
    /// (#25). Loads the rel's current props, evaluates the RHS (resolving
    /// `r.<prop>` self-refs and UNWIND `row.*` bindings via
    /// `evaluate_set_expression`), writes the property, and persists.
    pub(super) fn set_relationship_property(
        &mut self,
        rel_var: &str,
        rel_id: u64,
        property: &str,
        value: &executor::parser::Expression,
    ) -> Result<()> {
        let mut props: Map<String, Value> = self
            .storage
            .load_relationship_properties(rel_id)?
            .and_then(|v| match v {
                Value::Object(m) => Some(m),
                _ => None,
            })
            .unwrap_or_default();
        let v = self.evaluate_set_expression(value, rel_var, &props)?;
        // Null means "remove the property" (openCypher SET-to-null semantics).
        if matches!(v, Value::Null) {
            props.remove(property);
        } else {
            props.insert(property.to_string(), v);
        }
        let props_value = Value::Object(props);
        // Register every property key with the catalog so `db.propertyKeys()`
        // sees keys written via `SET <rel>.<property> = <value>`. See
        // `Catalog::register_property_keys`.
        self.catalog.register_property_keys(&props_value);
        self.storage
            .update_relationship_properties(rel_id, props_value)?;
        Ok(())
    }

    /// Apply `SET <rel> += <mapExpr>` to one relationship (#25): merge the
    /// evaluated map into the rel's props (null map = no-op; a null value in
    /// the map removes that key), mirroring the node `MapMerge` semantics.
    pub(super) fn merge_relationship_map(
        &mut self,
        rel_var: &str,
        rel_id: u64,
        map: &executor::parser::Expression,
    ) -> Result<()> {
        let mut props: Map<String, Value> = self
            .storage
            .load_relationship_properties(rel_id)?
            .and_then(|v| match v {
                Value::Object(m) => Some(m),
                _ => None,
            })
            .unwrap_or_default();
        match self.evaluate_set_expression(map, rel_var, &props)? {
            Value::Null => return Ok(()),
            Value::Object(rhs) => {
                for (k, v) in rhs.into_iter() {
                    if matches!(v, Value::Null) {
                        props.remove(&k);
                    } else {
                        props.insert(k, v);
                    }
                }
            }
            _ => {
                return Err(Error::CypherExecution(format!(
                    "ERR_SET_NON_MAP: SET {rel_var} += <rhs> requires a MAP or NULL"
                )));
            }
        }
        let props_value = Value::Object(props);
        // Register every property key with the catalog so `db.propertyKeys()`
        // sees keys written via `SET <rel> += <mapExpr>`. See
        // `Catalog::register_property_keys`.
        self.catalog.register_property_keys(&props_value);
        self.storage
            .update_relationship_properties(rel_id, props_value)?;
        Ok(())
    }

    /// Find a relationship of a specific type between two nodes
    pub(super) fn find_relationship_between(
        &self,
        src_id: u64,
        dst_id: u64,
        rel_type: &str,
    ) -> Result<Option<u64>> {
        // #18: if a prior incremental relationship-index update failed, rebuild
        // the index from storage once before trusting the exact-edge fast path.
        self.heal_relationship_index_if_dirty();

        // Get the type ID
        let type_id = match self.catalog.get_type_id(rel_type)? {
            Some(id) => id,
            None => return Ok(None),
        };

        // Fast path: the exact-edge existence index gives an O(1) hint for
        // `(src, type, dst)`. It is only a hint — verify against storage (the
        // record may be deleted, or the index may not have been rebuilt yet
        // after a restart). On any mismatch fall through to the authoritative
        // chain walk so correctness never depends on the index being complete.
        if let Some(rid) = self
            .cache
            .relationship_index()
            .find_edge(src_id, type_id, dst_id)
        {
            if let Ok(rel) = self.storage.read_rel(rid) {
                if !rel.is_deleted()
                    && rel.src_id == src_id
                    && rel.dst_id == dst_id
                    && rel.type_id == type_id
                {
                    return Ok(Some(rid));
                }
            }
        }

        // Read source node to get its relationship chain
        let src_node = self.storage.read_node(src_id)?;
        let mut rel_ptr = src_node.first_rel_ptr;

        // #20: make the fast-path miss itself observable (debug level — entry
        // is common on small graphs; the warn below covers the pathology).
        tracing::debug!(
            src_id,
            rel_type,
            "exact-edge index miss — falling back to O(degree) chain walk"
        );

        // Telemetry (issue #12): the chain walk is O(degree). For a hub node
        // accumulating thousands of same-type edges, each edge-MERGE existence
        // check that misses the exact-edge index degrades to a full-chain
        // scan, which under a sustained edge-write burst manifests as a
        // no-query-running CPU climb. Count hops and warn past a threshold so
        // the pathology is observable (RUST_LOG=nexus_core=warn) instead of an
        // opaque stall.
        let mut hops: u64 = 0;
        while rel_ptr != 0 {
            // Chain pointers are stored as `rel_id + 1` (0 is the
            // end-of-chain sentinel — see record_store_ops
            // `create_relationship` and the matching decode in
            // executor/operators/path.rs). Reading `rel_ptr` directly was
            // an off-by-one that silently broke this authoritative
            // fallback: it walked the wrong records and returned None (or
            // a wrong id) whenever the exact-edge index missed.
            let rel_id = rel_ptr - 1;
            let rel_record = self.storage.read_rel(rel_id)?;
            hops += 1;

            // #20: warn DURING the walk, the moment it crosses the threshold,
            // so a hub-degree pathology is surfaced in real time — not only
            // after a (possibly enormous) scan completes, and even when the
            // edge is eventually found below (the early return would otherwise
            // skip a post-loop warning).
            if hops == 1000 {
                tracing::warn!(
                    src_id,
                    rel_type,
                    "find_relationship_between is walking a long O(degree) \
                     relationship chain (>= 1000 hops) — exact-edge index miss \
                     on a high-degree hub; sustained edge-MERGE here can pin CPU \
                     (issue #12)"
                );
            }

            // Check if this is an outgoing relationship to dst_id with the
            // right type. Skip deleted records — the fast path above
            // verifies deletion too, and MERGE must not treat a deleted
            // edge as existing.
            if !rel_record.is_deleted()
                && rel_record.src_id == src_id
                && rel_record.dst_id == dst_id
                && rel_record.type_id == type_id
            {
                return Ok(Some(rel_id));
            }

            // Move to next relationship in chain
            if rel_record.src_id == src_id {
                rel_ptr = rel_record.next_src_ptr;
            } else if rel_record.dst_id == src_id {
                rel_ptr = rel_record.next_dst_ptr;
            } else {
                break;
            }
        }

        Ok(None)
    }

    /// Build return result with support for relationship variables
    pub(super) fn build_return_result_with_rels(
        &mut self,
        context: &HashMap<String, Vec<u64>>,
        rel_context: &HashMap<String, Vec<(u64, String)>>,
        return_clause: &executor::parser::ReturnClause,
    ) -> Result<executor::ResultSet> {
        if return_clause.items.is_empty() {
            return Ok(executor::ResultSet::new(vec![], vec![]));
        }

        // Check if any return item references a relationship variable
        let has_rel_refs = return_clause
            .items
            .iter()
            .any(|item| self.expression_references_rel(&item.expression, rel_context));

        if !has_rel_refs || rel_context.is_empty() {
            // No relationship references, use regular handling
            return self.build_return_result(context, return_clause);
        }

        // Build result with relationship variable support
        let mut columns = Vec::new();
        let mut row_values = Vec::new();

        for item in &return_clause.items {
            let col_name = item
                .alias
                .clone()
                .unwrap_or_else(|| self.expression_to_string(&item.expression));
            columns.push(col_name);

            let value =
                self.evaluate_return_expression_with_rels(&item.expression, context, rel_context)?;
            row_values.push(value);
        }

        Ok(executor::ResultSet::new(
            columns,
            vec![executor::Row { values: row_values }],
        ))
    }

    /// Check if an expression references a relationship variable
    pub(super) fn expression_references_rel(
        &self,
        expr: &executor::parser::Expression,
        rel_context: &HashMap<String, Vec<(u64, String)>>,
    ) -> bool {
        match expr {
            executor::parser::Expression::Variable(v) => rel_context.contains_key(v),
            executor::parser::Expression::FunctionCall { args, .. } => args
                .iter()
                .any(|arg| self.expression_references_rel(arg, rel_context)),
            executor::parser::Expression::PropertyAccess { variable, .. } => {
                rel_context.contains_key(variable)
            }
            _ => false,
        }
    }

    /// Evaluate a return expression with relationship variable support
    pub(super) fn evaluate_return_expression_with_rels(
        &self,
        expr: &executor::parser::Expression,
        _context: &HashMap<String, Vec<u64>>,
        rel_context: &HashMap<String, Vec<(u64, String)>>,
    ) -> Result<Value> {
        match expr {
            executor::parser::Expression::FunctionCall { name, args } => {
                let func_name = name.to_lowercase();
                if func_name == "type" && args.len() == 1 {
                    // type(r) - return relationship type
                    if let executor::parser::Expression::Variable(var) = &args[0] {
                        if let Some(entries) = rel_context.get(var) {
                            if let Some((_rel_id, rel_type)) = entries.last() {
                                return Ok(Value::String(rel_type.clone()));
                            }
                        }
                    }
                }
                // #14: `count(r)` over an upserted relationship variable —
                // number of distinct edges merged across all UNWIND rows.
                if func_name == "count" && args.len() == 1 {
                    if let executor::parser::Expression::Variable(var) = &args[0] {
                        if let Some(entries) = rel_context.get(var) {
                            let mut ids: Vec<u64> = entries.iter().map(|(id, _)| *id).collect();
                            ids.sort_unstable();
                            ids.dedup();
                            return Ok(Value::Number((ids.len() as u64).into()));
                        }
                    }
                }
                // For other functions, return null for now
                Ok(Value::Null)
            }
            executor::parser::Expression::Variable(var) => {
                if let Some(entries) = rel_context.get(var) {
                    if let Some((rel_id, rel_type)) = entries.last() {
                        // Return relationship as object
                        let mut obj = Map::new();
                        obj.insert("_id".to_string(), Value::Number((*rel_id).into()));
                        obj.insert("_type".to_string(), Value::String(rel_type.clone()));
                        return Ok(Value::Object(obj));
                    }
                }
                Ok(Value::Null)
            }
            // `RETURN r.prop` in the same statement as the MERGE/CREATE
            // that bound `r`. This arm was missing — PropertyAccess on a
            // relationship variable fell through to the catch-all Null
            // below, so `MERGE (a)-[r:T {v:9}]->(b) RETURN r.v` projected
            // null even though the property was persisted correctly
            // (harness case 11b; found live on the 2.5.0-dev image).
            executor::parser::Expression::PropertyAccess { variable, property } => {
                if let Some(entries) = rel_context.get(variable) {
                    if let Some((rel_id, _)) = entries.last() {
                        let props = self.storage.load_relationship_properties(*rel_id)?;
                        if let Some(Value::Object(map)) = props {
                            return Ok(map.get(property).cloned().unwrap_or(Value::Null));
                        }
                    }
                }
                Ok(Value::Null)
            }
            _ => Ok(Value::Null),
        }
    }

    pub(super) fn apply_set_clause(
        &mut self,
        context: &HashMap<String, Vec<u64>>,
        rel_context: &HashMap<String, Vec<(u64, String)>>,
        set_clause: &executor::parser::SetClause,
    ) -> Result<()> {
        tracing::info!(
            "[apply_set_clause] START: context={:?}, items={}",
            context,
            set_clause.items.len()
        );
        if set_clause.items.is_empty() {
            tracing::info!("[apply_set_clause] No items, returning early");
            return Ok(());
        }

        let mut state_map: HashMap<u64, NodeWriteState> = HashMap::new();

        for item in &set_clause.items {
            match item {
                executor::parser::SetItem::Property {
                    target,
                    property,
                    value,
                } => {
                    // #25 — `SET r.k = v` on a matched/merged relationship
                    // variable. Resolve `r` from the relationship context
                    // (the write-path MATCH now binds rel vars); apply to
                    // every bound relationship.
                    if let Some(rels) = rel_context.get(target) {
                        for (rel_id, _ty) in rels.clone() {
                            self.set_relationship_property(target, rel_id, property, value)?;
                        }
                        continue;
                    }
                    let node_ids = context.get(target).ok_or_else(|| {
                        Error::CypherExecution(format!(
                            "Unknown variable '{}' in SET clause",
                            target
                        ))
                    })?;

                    // Evaluate expression per-node to support expressions like n.value * 2
                    tracing::info!(
                        "[apply_set_clause] Property SET: target={}, property={}, node_ids={:?}",
                        target,
                        property,
                        node_ids
                    );
                    for node_id in node_ids.clone() {
                        let state = self.ensure_node_state(node_id, &mut state_map)?;
                        let json_value =
                            self.evaluate_set_expression(value, target, &state.properties)?;
                        tracing::info!(
                            "[apply_set_clause] node_id={}, property={}, new_value={:?}",
                            node_id,
                            property,
                            json_value
                        );
                        // phase6_opencypher-constraint-enforcement —
                        // run NOT NULL guard for this node's labels
                        // (existing + staged), and the property-type
                        // check against the new value.
                        let label_ids = self.label_ids_for_state(state)?;
                        self.enforce_not_null_on_prop_change(
                            &label_ids,
                            property,
                            Some(&json_value),
                        )?;
                        // Check property-type constraint against the
                        // specific value being written.
                        if !matches!(json_value, serde_json::Value::Null) {
                            for c in &self.property_type_constraints {
                                if c.property_key != *property {
                                    continue;
                                }
                                let Some(label_id) = c.label_id else { continue };
                                if !label_ids.contains(&label_id) {
                                    continue;
                                }
                                if !c.ty.accepts(&json_value) {
                                    return Err(Error::ConstraintViolation(format!(
                                        "ERR_CONSTRAINT_VIOLATED: kind=PROPERTY_TYPE \
                                         property={:?} expected={} got={}",
                                        c.property_key,
                                        c.ty.name(),
                                        super::json_type_label(&json_value),
                                    )));
                                }
                            }
                        }
                        // B8 — `SET n.p = null` removes the key (Neo4j
                        // semantics: a property whose value is NULL is
                        // absent), rather than storing a literal JSON null.
                        if matches!(json_value, serde_json::Value::Null) {
                            state.properties.remove(property);
                        } else {
                            state.properties.insert(property.clone(), json_value);
                        }
                    }
                }
                executor::parser::SetItem::Label { target, label } => {
                    let node_ids = context.get(target).ok_or_else(|| {
                        Error::CypherExecution(format!(
                            "Unknown variable '{}' in SET clause",
                            target
                        ))
                    })?;

                    // phase6_opencypher-advanced-types §2 — resolve
                    // `:$param` in SET position. A single parser-emitted
                    // label may fan out to multiple names when the
                    // parameter is a `LIST<STRING>`.
                    let resolved = self.resolve_dynamic_labels(std::slice::from_ref(label))?;
                    for node_id in node_ids.clone() {
                        let state = self.ensure_node_state(node_id, &mut state_map)?;
                        for lbl in &resolved {
                            // phase6_opencypher-constraint-enforcement §4 —
                            // adding a label whose NOT NULL constraint is
                            // not satisfied by the current property bag
                            // must fail before the label lands on the
                            // pending state.
                            self.enforce_add_label_constraints(lbl, &state.properties)?;
                            state.labels.insert(lbl.clone());
                        }
                    }
                }
                // phase6_opencypher-quickwins §6 — `SET lhs += mapExpr`.
                executor::parser::SetItem::MapMerge { target, map } => {
                    // #25 — `SET r += {…}` on a relationship variable.
                    if let Some(rels) = rel_context.get(target) {
                        for (rel_id, _ty) in rels.clone() {
                            self.merge_relationship_map(target, rel_id, map)?;
                        }
                        continue;
                    }
                    let node_ids = context.get(target).ok_or_else(|| {
                        Error::CypherExecution(format!(
                            "Unknown variable '{}' in SET clause",
                            target
                        ))
                    })?;
                    for node_id in node_ids.clone() {
                        let state = self.ensure_node_state(node_id, &mut state_map)?;
                        let evaluated =
                            self.evaluate_set_expression(map, target, &state.properties)?;
                        match evaluated {
                            Value::Null => {
                                // NULL RHS is a no-op — preserves current bag.
                            }
                            Value::Object(rhs) => {
                                for (k, v) in rhs.into_iter() {
                                    if matches!(v, Value::Null) {
                                        state.properties.remove(&k);
                                    } else {
                                        state.properties.insert(k, v);
                                    }
                                }
                            }
                            other => {
                                return Err(Error::CypherExecution(format!(
                                    "ERR_SET_NON_MAP: SET {} += <rhs> requires a MAP or NULL \
                                     (got {})",
                                    target,
                                    match other {
                                        Value::Bool(_) => "BOOLEAN",
                                        Value::Number(n) => {
                                            if n.is_i64() || n.is_u64() {
                                                "INTEGER"
                                            } else {
                                                "FLOAT"
                                            }
                                        }
                                        Value::String(_) => "STRING",
                                        Value::Array(_) => "LIST",
                                        _ => "?",
                                    }
                                )));
                            }
                        }
                    }
                }
            }
        }

        tracing::info!(
            "[apply_set_clause] About to persist {} nodes",
            state_map.len()
        );
        for (node_id, state) in state_map.into_iter() {
            tracing::info!(
                "[apply_set_clause] Persisting node_id={}, properties={:?}",
                node_id,
                state.properties
            );
            self.persist_node_state(node_id, state)?;
        }
        tracing::info!("[apply_set_clause] DONE");

        Ok(())
    }

    pub(super) fn apply_remove_clause(
        &mut self,
        context: &HashMap<String, Vec<u64>>,
        remove_clause: &executor::parser::RemoveClause,
    ) -> Result<()> {
        if remove_clause.items.is_empty() {
            return Ok(());
        }

        let mut state_map: HashMap<u64, NodeWriteState> = HashMap::new();

        for item in &remove_clause.items {
            match item {
                executor::parser::RemoveItem::Property { target, property } => {
                    let node_ids = context.get(target).ok_or_else(|| {
                        Error::CypherExecution(format!(
                            "Unknown variable '{}' in REMOVE clause",
                            target
                        ))
                    })?;

                    for node_id in node_ids {
                        let state = self.ensure_node_state(*node_id, &mut state_map)?;
                        // phase6_opencypher-constraint-enforcement §4/§5 —
                        // reject REMOVE of a NOT NULL / NODE KEY
                        // component before mutating the pending
                        // property bag.
                        let label_ids = self.label_ids_for_state(state)?;
                        self.enforce_not_null_on_prop_change(&label_ids, property, None)?;
                        state.properties.remove(property);
                    }
                }
                executor::parser::RemoveItem::Label { target, label } => {
                    let node_ids = context.get(target).ok_or_else(|| {
                        Error::CypherExecution(format!(
                            "Unknown variable '{}' in REMOVE clause",
                            target
                        ))
                    })?;

                    // phase6_opencypher-advanced-types §2 — resolve
                    // `:$param` in REMOVE position (same semantics as
                    // SET, inverted operation).
                    let resolved = self.resolve_dynamic_labels(std::slice::from_ref(label))?;
                    for node_id in node_ids.clone() {
                        let state = self.ensure_node_state(node_id, &mut state_map)?;
                        for lbl in &resolved {
                            state.labels.remove(lbl);
                        }
                    }
                }
            }
        }

        for (node_id, state) in state_map.into_iter() {
            self.persist_node_state(node_id, state)?;
        }

        Ok(())
    }

    pub(super) fn execute_foreach_clause(
        &mut self,
        context: &HashMap<String, Vec<u64>>,
        foreach_clause: &executor::parser::ForeachClause,
    ) -> Result<()> {
        // Evaluate the list expression
        let list_value = match &foreach_clause.list_expression {
            executor::parser::Expression::Variable(var_name) => {
                // Variable from context - assume it's a list of node IDs
                // Convert node IDs to a list of values (we'll use node IDs as the iteration items)
                // For FOREACH, we typically iterate over node IDs, not values
                context.get(var_name).cloned().unwrap_or_default()
            }
            executor::parser::Expression::Literal(executor::parser::Literal::Null) => {
                // NULL list - no iteration
                return Ok(());
            }
            executor::parser::Expression::List(items) => {
                // Literal list - evaluate each item
                // For now, we'll treat list items as node IDs if they're integers
                // This is a simplified implementation
                let mut node_ids = Vec::new();
                for item in items {
                    if let executor::parser::Expression::Literal(
                        executor::parser::Literal::Integer(id),
                    ) = item
                    {
                        node_ids.push(*id as u64);
                    }
                }
                node_ids
            }
            _ => {
                return Err(Error::CypherExecution(format!(
                    "FOREACH list expression must be a variable or literal list, got: {:?}",
                    foreach_clause.list_expression
                )));
            }
        };

        // Iterate over each item in the list
        for item_value in list_value {
            // Create a new context for this iteration with the FOREACH variable
            // The variable contains a single node ID for this iteration
            let mut iteration_context = context.clone();
            iteration_context.insert(foreach_clause.variable.clone(), vec![item_value]);

            // Execute each update clause for this iteration
            for update_clause in &foreach_clause.update_clauses {
                match update_clause {
                    executor::parser::ForeachUpdateClause::Set(set_clause) => {
                        self.apply_set_clause(&iteration_context, &HashMap::new(), set_clause)?;
                    }
                    executor::parser::ForeachUpdateClause::Delete(delete_clause) => {
                        // Apply DELETE for this iteration
                        // DELETE in FOREACH context means delete the node referenced by the variable
                        let node_ids = iteration_context
                            .get(&foreach_clause.variable)
                            .cloned()
                            .unwrap_or_default();

                        for node_id in node_ids {
                            if delete_clause.detach {
                                // DETACH DELETE: remove all relationships first
                                self.delete_node_relationships(node_id)?;
                                self.delete_node(node_id)?;
                            } else {
                                // Regular DELETE: the relationship-existence
                                // guard is centralized in `delete_node` (both
                                // outgoing AND incoming edges — the local
                                // `first_rel_ptr != 0` check only saw outgoing
                                // ones and let an incoming-only node slip past;
                                // phase0_fix-delete-node-dangling-relationships).
                                self.delete_node(node_id)?;
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    pub(super) fn build_return_result(
        &mut self,
        context: &HashMap<String, Vec<u64>>,
        return_clause: &executor::parser::ReturnClause,
    ) -> Result<executor::ResultSet> {
        if return_clause.items.is_empty() {
            return Ok(executor::ResultSet::new(vec![], vec![]));
        }

        // Check if we have any complex expressions (function calls, aggregations)
        // If so, delegate to the full executor by converting to a query
        let has_complex_expressions = return_clause.items.iter().any(|item| {
            !matches!(
                &item.expression,
                executor::parser::Expression::Variable(_)
                    | executor::parser::Expression::PropertyAccess { .. }
            )
        });

        if has_complex_expressions {
            // For complex expressions, we need to use the full executor
            // Build a complete query with the context data materialized
            return self.build_return_result_with_executor(context, return_clause);
        }

        // Simple case: only variables and property access
        // Determine which variable(s) we need nodes from
        let mut var_for_iteration: Option<String> = None;
        let mut columns = Vec::new();

        for item in &return_clause.items {
            let (var, col_name) = match &item.expression {
                executor::parser::Expression::Variable(var) => {
                    let col = item.alias.clone().unwrap_or_else(|| var.clone());
                    (var.clone(), col)
                }
                executor::parser::Expression::PropertyAccess { variable, property } => {
                    let col = item
                        .alias
                        .clone()
                        .unwrap_or_else(|| format!("{}.{}", variable, property));
                    (variable.clone(), col)
                }
                _ => unreachable!("Complex expressions should be handled above"),
            };

            if var_for_iteration.is_none() {
                var_for_iteration = Some(var.clone());
            } else if var_for_iteration.as_ref() != Some(&var) {
                return Err(Error::CypherExecution(
                    "Multiple different variables in RETURN not supported for write queries"
                        .to_string(),
                ));
            }
            columns.push(col_name);
        }

        let var_name = match var_for_iteration {
            Some(v) => v,
            None => {
                return Ok(executor::ResultSet::new(columns, vec![]));
            }
        };

        let node_ids = context.get(&var_name).cloned().unwrap_or_default();
        let mut seen = HashSet::new();
        let mut rows = Vec::new();

        for node_id in node_ids {
            if seen.insert(node_id) {
                let mut row_values = Vec::new();

                for item in &return_clause.items {
                    let value = match &item.expression {
                        executor::parser::Expression::Variable(_) => {
                            self.node_to_result_value(node_id)?
                        }
                        executor::parser::Expression::PropertyAccess { property, .. } => {
                            // Get the property value from the node
                            let props = self.storage.load_node_properties(node_id)?;
                            tracing::info!(
                                "[build_return_result] node_id={}, loaded props={:?}",
                                node_id,
                                props
                            );
                            if let Some(Value::Object(map)) = props {
                                let result = map.get(property).cloned().unwrap_or(Value::Null);
                                tracing::info!(
                                    "[build_return_result] property={}, result={:?}",
                                    property,
                                    result
                                );
                                result
                            } else {
                                tracing::info!(
                                    "[build_return_result] property={}, no props found",
                                    property
                                );
                                Value::Null
                            }
                        }
                        _ => Value::Null,
                    };
                    row_values.push(value);
                }

                rows.push(executor::Row { values: row_values });
            }
        }

        Ok(executor::ResultSet::new(columns, rows))
    }

    pub(super) fn build_return_result_with_executor(
        &mut self,
        context: &HashMap<String, Vec<u64>>,
        return_clause: &executor::parser::ReturnClause,
    ) -> Result<executor::ResultSet> {
        // For complex expressions, convert the context into a MATCH query
        // and let the full executor handle it

        // Find the variable name from context
        let var_name = context.keys().next().ok_or_else(|| {
            Error::CypherExecution("No context variable for complex RETURN".to_string())
        })?;

        let node_ids = context.get(var_name).cloned().unwrap_or_default();

        if node_ids.is_empty() {
            // Build empty result with correct columns
            let columns = return_clause
                .items
                .iter()
                .map(|item| item.alias.clone().unwrap_or_else(|| "?column?".to_string()))
                .collect();
            return Ok(executor::ResultSet::new(columns, vec![]));
        }

        // Build a query like: MATCH (var) WHERE id(var) IN [ids] RETURN ...
        let ids_str = node_ids
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join(", ");

        let return_str = return_clause
            .items
            .iter()
            .map(|item| {
                let expr_str = self.expression_to_string(&item.expression);
                if let Some(alias) = &item.alias {
                    format!("{} AS {}", expr_str, alias)
                } else {
                    expr_str
                }
            })
            .collect::<Vec<_>>()
            .join(", ");

        let query_str = format!(
            "MATCH ({}) WHERE id({}) IN [{}] RETURN {}",
            var_name, var_name, ids_str, return_str
        );

        // Execute through the full executor
        let query_obj = executor::Query {
            cypher: query_str,
            params: std::collections::HashMap::new(),
        };

        self.executor.execute(&query_obj)
    }
}
