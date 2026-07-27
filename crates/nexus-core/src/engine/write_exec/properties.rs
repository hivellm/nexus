//! SET / REMOVE / FOREACH execution for the write path, plus the
//! relationship property-mutation primitives (`SET <rel>.<k> = v`,
//! `SET <rel> += <map>`) that both the SET clause and MERGE's
//! `ON CREATE`/`ON MATCH` route through. Extracted from
//! `engine/write_exec.rs`.

use super::super::Engine;
use super::super::crud::NodeWriteState;
use crate::{Error, Result, executor};
use serde_json::{Map, Value};
use std::collections::HashMap;

impl Engine {
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
        // Side-effect count (openCypher TCK `+labels`): only labels that were
        // not already present on the node are counted, so `SET n:L` on a node
        // that already carries `L` is the idempotent no-op the TCK expects.
        // Accumulated locally to avoid borrowing `self` while `state` is held,
        // then folded into `self.side_effects` once below.
        let mut labels_added = 0u64;
        // Side-effect counts for properties (openCypher TCK `+properties` /
        // `-properties`): every `SET n.k = <non-null>` is a write (counted even
        // when the value is unchanged, per the TCK); `SET n.k = null` and the
        // null branch of `SET n += {…}` remove a key (counted only when the key
        // was present). Same local-accumulator-then-fold pattern as labels.
        let mut properties_set = 0u64;
        let mut properties_removed = 0u64;

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
                                        super::super::json_type_label(&json_value),
                                    )));
                                }
                            }
                        }
                        // B8 — `SET n.p = null` removes the key (Neo4j
                        // semantics: a property whose value is NULL is
                        // absent), rather than storing a literal JSON null.
                        if matches!(json_value, serde_json::Value::Null) {
                            if state.properties.remove(property).is_some() {
                                properties_removed += 1;
                            }
                        } else {
                            // openCypher counts overwriting a property that
                            // already holds a value as BOTH `+properties 1`
                            // and `-properties 1` (the old value is removed,
                            // the new one set); a brand-new key is `+1` only
                            // (`Set1[1][2]` vs `Set1[3]`).
                            let overwrote = state
                                .properties
                                .insert(property.clone(), json_value)
                                .is_some_and(|old| !old.is_null());
                            properties_set += 1;
                            if overwrote {
                                properties_removed += 1;
                            }
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
                            if state.labels.insert(lbl.clone()) {
                                labels_added += 1;
                            }
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
                                        if state.properties.remove(&k).is_some() {
                                            properties_removed += 1;
                                        }
                                    } else {
                                        // Same overwrite accounting as the
                                        // scalar `SET n.p = v` path above.
                                        let overwrote = state
                                            .properties
                                            .insert(k, v)
                                            .is_some_and(|old| !old.is_null());
                                        properties_set += 1;
                                        if overwrote {
                                            properties_removed += 1;
                                        }
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
                // `SET lhs = rhsExpr` whole-entity property replace. Every
                // existing key NOT present in the evaluated RHS map is
                // dropped first; every key IN the map is then applied with
                // the same overwrite accounting as the `SET n.k = v` /
                // `SET n += {...}` arms above.
                executor::parser::SetItem::Replace { target, value } => {
                    let node_ids = context
                        .get(target)
                        .ok_or_else(|| {
                            Error::CypherExecution(format!(
                                "Unknown variable '{}' in SET clause",
                                target
                            ))
                        })?
                        .clone();

                    // `SET n = m` — copy another bound node's CURRENT
                    // property bag (including edits already staged earlier
                    // in this same SET clause) instead of routing through
                    // `evaluate_set_expression`, which only resolves bare
                    // variables against UNWIND row bindings.
                    let other_ids = match value {
                        executor::parser::Expression::Variable(other) if other != target => {
                            context.get(other).cloned()
                        }
                        _ => None,
                    };

                    for (idx, node_id) in node_ids.iter().copied().enumerate() {
                        let new_props: Map<String, Value> = if let Some(other_ids) = &other_ids {
                            let source_id =
                                other_ids.get(idx).or_else(|| other_ids.first()).copied();
                            match source_id {
                                Some(sid) => {
                                    if let Some(staged) = state_map.get(&sid) {
                                        staged.properties.clone()
                                    } else {
                                        self.load_node_properties_map(sid)?
                                    }
                                }
                                None => Map::new(),
                            }
                        } else {
                            let state = self.ensure_node_state(node_id, &mut state_map)?;
                            let evaluated =
                                self.evaluate_set_expression(value, target, &state.properties)?;
                            match evaluated {
                                // `SET n = null` clears every property — an
                                // absent map keeps nothing.
                                Value::Null => Map::new(),
                                Value::Object(m) => m,
                                other => {
                                    return Err(Error::CypherExecution(format!(
                                        "ERR_SET_NON_MAP: SET {} = <rhs> requires a MAP or NULL \
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
                        };

                        let state = self.ensure_node_state(node_id, &mut state_map)?;
                        let stale_keys: Vec<String> = state
                            .properties
                            .keys()
                            .filter(|k| !new_props.contains_key(*k))
                            .cloned()
                            .collect();
                        for k in stale_keys {
                            if state.properties.remove(&k).is_some() {
                                properties_removed += 1;
                            }
                        }
                        for (k, v) in new_props.into_iter() {
                            if matches!(v, Value::Null) {
                                if state.properties.remove(&k).is_some() {
                                    properties_removed += 1;
                                }
                            } else {
                                let overwrote = state
                                    .properties
                                    .insert(k, v)
                                    .is_some_and(|old| !old.is_null());
                                properties_set += 1;
                                if overwrote {
                                    properties_removed += 1;
                                }
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

        self.side_effects.labels_added += labels_added;
        self.side_effects.properties_set += properties_set;
        self.side_effects.properties_removed += properties_removed;
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
        // Side-effect count (openCypher TCK `-labels`): only labels actually
        // present are counted, so `REMOVE n:L` of an absent label is the
        // idempotent no-op the TCK expects. Local accumulator, folded into
        // `self.side_effects` below.
        let mut labels_removed = 0u64;
        // `REMOVE n.k` removes a property key — counted (TCK `-properties`)
        // only when the key was actually present.
        let mut properties_removed = 0u64;

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
                        if state.properties.remove(property).is_some() {
                            properties_removed += 1;
                        }
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
                            if state.labels.remove(lbl) {
                                labels_removed += 1;
                            }
                        }
                    }
                }
            }
        }

        for (node_id, state) in state_map.into_iter() {
            self.persist_node_state(node_id, state)?;
        }

        self.side_effects.labels_removed += labels_removed;
        self.side_effects.properties_removed += properties_removed;
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
}
