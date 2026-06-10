//! Constraint registration, backfill validation, and write-path
//! enforcement for the Engine. Extracted from `engine/mod.rs`.

use super::Engine;
use super::typed_collections;
use crate::{Error, Result, catalog};
use std::collections::HashMap;

impl Engine {
    // ────────── phase6 constraint-enforcement — programmatic APIs ──────────

    /// Flip the relaxed-enforcement flag at runtime.
    /// When `true`, every violation from `check_constraints` /
    /// `enforce_rel_constraints` downgrades to a `warn` log and the
    /// write succeeds. Intended only for data-migration windows;
    /// scheduled for removal at v1.5.
    pub fn set_relaxed_constraint_enforcement(&mut self, relaxed: bool) {
        if relaxed {
            tracing::warn!(
                "relaxed_constraint_enforcement=true — constraint violations will be logged \
                 only, not rejected. This flag is scheduled for removal at v1.5."
            );
        }
        self.relaxed_constraint_enforcement = relaxed;
    }

    /// Register a `REQUIRE (n.p1, n.p2, ...) IS NODE KEY` constraint.
    /// Creates (or reuses) a UNIQUE composite B-tree over the property
    /// list and backfills from existing nodes — CREATE aborts with an
    /// offending-row report if any existing tuple violates uniqueness
    /// or has a NULL component.
    pub fn add_node_key_constraint(
        &mut self,
        label: &str,
        property_keys: &[&str],
        name: Option<&str>,
    ) -> Result<()> {
        if property_keys.is_empty() {
            return Err(Error::CypherSyntax(
                "NODE KEY requires at least one property".to_string(),
            ));
        }
        let label_id = self.catalog.get_or_create_label(label)?;
        for p in property_keys {
            let _ = self.catalog.get_or_create_key(p)?;
        }
        let property_keys: Vec<String> = property_keys.iter().map(|s| s.to_string()).collect();

        // Backfill scan — validate existing data before registering.
        self.backfill_node_key(label_id, label, &property_keys)?;

        // Register the composite index (UNIQUE flag on).
        self.indexes.composite_btree.register(
            label_id,
            property_keys.clone(),
            true,
            name.map(|s| s.to_string()),
            true,
        )?;
        // Track the logical constraint separately so db.constraints()
        // can report it and enforcement checks can route through a
        // single lookup rather than grovelling through the index
        // registry.
        self.node_key_constraints
            .push(crate::constraints::NodeKeyConstraint {
                name: name.map(|s| s.to_string()),
                label_id,
                property_keys,
            });
        Ok(())
    }

    /// Register a `REQUIRE r.p IS NOT NULL` constraint for relationships
    /// of a given type. Backfill rejects existing rels that lack the
    /// property.
    pub fn add_rel_not_null_constraint(
        &mut self,
        rel_type: &str,
        property_key: &str,
        name: Option<&str>,
    ) -> Result<()> {
        let rel_type_id = self.catalog.get_or_create_type(rel_type)?;
        let _ = self.catalog.get_or_create_key(property_key)?;
        self.backfill_rel_not_null(rel_type_id, rel_type, property_key)?;
        self.rel_not_null_constraints
            .push(crate::constraints::RelNotNullConstraint {
                name: name.map(|s| s.to_string()),
                rel_type_id,
                property_key: property_key.to_string(),
            });
        Ok(())
    }

    /// Register a `REQUIRE n.p IS :: <TYPE>` constraint on a node
    /// label. Backfill rejects existing nodes whose value is present
    /// but of a different type.
    pub fn add_property_type_constraint(
        &mut self,
        label: &str,
        property_key: &str,
        ty: crate::constraints::ScalarType,
        name: Option<&str>,
    ) -> Result<()> {
        let label_id = self.catalog.get_or_create_label(label)?;
        let _ = self.catalog.get_or_create_key(property_key)?;
        self.backfill_property_type(label_id, label, property_key, ty)?;
        self.property_type_constraints
            .push(crate::constraints::PropertyTypeConstraint {
                name: name.map(|s| s.to_string()),
                label_id: Some(label_id),
                rel_type_id: None,
                property_key: property_key.to_string(),
                ty,
            });
        Ok(())
    }

    /// Property-type constraint for relationships (`()-[r:T]-()` form).
    pub fn add_rel_property_type_constraint(
        &mut self,
        rel_type: &str,
        property_key: &str,
        ty: crate::constraints::ScalarType,
        name: Option<&str>,
    ) -> Result<()> {
        let rel_type_id = self.catalog.get_or_create_type(rel_type)?;
        let _ = self.catalog.get_or_create_key(property_key)?;
        self.property_type_constraints
            .push(crate::constraints::PropertyTypeConstraint {
                name: name.map(|s| s.to_string()),
                label_id: None,
                rel_type_id: Some(rel_type_id),
                property_key: property_key.to_string(),
                ty,
            });
        Ok(())
    }

    pub fn add_typed_list_constraint(
        &mut self,
        label: &str,
        property: &str,
        elem_type: typed_collections::ListElemType,
    ) -> Result<()> {
        let label_id = self.catalog.get_or_create_label(label)?;
        let key_id = self.catalog.get_or_create_key(property)?;
        self.typed_list_constraints
            .insert((label_id, key_id), elem_type);
        Ok(())
    }

    /// Remove a previously-registered typed-list constraint.
    /// No-op when nothing is registered for the pair.
    pub fn drop_typed_list_constraint(&mut self, label: &str, property: &str) -> Result<()> {
        let Ok(label_id) = self.catalog.get_label_id(label) else {
            return Ok(());
        };
        let Ok(key_id) = self.catalog.get_key_id(property) else {
            return Ok(());
        };
        self.typed_list_constraints.remove(&(label_id, key_id));
        Ok(())
    }

    // ────────── Backfill validators (§8) ──────────

    /// Verify every existing node with `label_id` has non-NULL values
    /// for each property in `props`, AND that the tuple is globally
    /// unique. Returns a violation error when the report isn't empty.
    fn backfill_node_key(&self, label_id: u32, label: &str, props: &[String]) -> Result<()> {
        let bitmap = self
            .indexes
            .label_index
            .get_nodes_with_labels(&[label_id])?;
        let mut report = crate::constraints::BackfillReport::default();
        let mut seen: HashMap<Vec<String>, u64> = HashMap::new();
        for nid in bitmap.iter() {
            let nid = nid as u64;
            report.total_scanned += 1;
            let props_value = self.storage.load_node_properties(nid)?;
            let obj = match props_value {
                Some(serde_json::Value::Object(m)) => m,
                _ => {
                    report.record(nid, format!("missing properties on :{label}"));
                    continue;
                }
            };
            let mut tuple: Vec<String> = Vec::with_capacity(props.len());
            let mut bad = false;
            for p in props {
                match obj.get(p) {
                    Some(serde_json::Value::Null) | None => {
                        report.record(nid, format!("property {p:?} is NULL"));
                        bad = true;
                        break;
                    }
                    Some(v) => tuple.push(v.to_string()),
                }
            }
            if bad {
                continue;
            }
            if let Some(prev) = seen.insert(tuple.clone(), nid) {
                report.record(
                    nid,
                    format!("duplicate tuple already present at node {prev}"),
                );
            }
        }
        if report.has_violations() {
            return Err(report.into_error("NODE_KEY"));
        }
        Ok(())
    }

    fn backfill_rel_not_null(
        &self,
        rel_type_id: u32,
        rel_type: &str,
        property_key: &str,
    ) -> Result<()> {
        let mut report = crate::constraints::BackfillReport::default();
        let total = self.storage.relationship_count();
        for rid in 0..total {
            let rec = match self.storage.read_rel(rid) {
                Ok(r) => r,
                Err(_) => continue,
            };
            if rec.is_deleted() || rec.type_id != rel_type_id {
                continue;
            }
            report.total_scanned += 1;
            let props = self
                .storage
                .load_relationship_properties(rid)
                .ok()
                .flatten();
            let ok = matches!(
                props.as_ref().and_then(|v| v.as_object()).and_then(|m| m.get(property_key)),
                Some(v) if !matches!(v, serde_json::Value::Null)
            );
            if !ok {
                report.record(
                    rid,
                    format!("rel :{rel_type} missing property {property_key:?}"),
                );
            }
        }
        if report.has_violations() {
            return Err(report.into_error("RELATIONSHIP_PROPERTY_EXISTENCE"));
        }
        Ok(())
    }

    fn backfill_property_type(
        &self,
        label_id: u32,
        label: &str,
        property_key: &str,
        ty: crate::constraints::ScalarType,
    ) -> Result<()> {
        let bitmap = self
            .indexes
            .label_index
            .get_nodes_with_labels(&[label_id])?;
        let mut report = crate::constraints::BackfillReport::default();
        for nid in bitmap.iter() {
            let nid = nid as u64;
            report.total_scanned += 1;
            let props = match self.storage.load_node_properties(nid)? {
                Some(serde_json::Value::Object(m)) => m,
                _ => continue,
            };
            if let Some(v) = props.get(property_key) {
                // NULL is treated as "absent" here — the NOT NULL
                // constraint handles null separately.
                if matches!(v, serde_json::Value::Null) {
                    continue;
                }
                if !ty.accepts(v) {
                    report.record(
                        nid,
                        format!(
                            "node :{label}.{property_key} is {got}, expected {want}",
                            got = super::json_type_label(v),
                            want = ty.name()
                        ),
                    );
                }
            }
        }
        if report.has_violations() {
            return Err(report.into_error("PROPERTY_TYPE"));
        }
        Ok(())
    }

    // ────────── Write-path enforcement hooks ──────────

    /// Extra constraint checks that run alongside the legacy
    /// `check_constraints` path. Called from every site that writes
    /// node properties. Applies property-type constraints and NODE
    /// KEY uniqueness/NOT-NULL.
    pub(crate) fn enforce_extended_node_constraints(
        &self,
        label_ids: &[u32],
        properties: &serde_json::Value,
        exclude_node_id: Option<u64>,
    ) -> Result<()> {
        // Property-type checks (node-scoped).
        if let Some(props) = properties.as_object() {
            for c in &self.property_type_constraints {
                let Some(label_id) = c.label_id else {
                    continue;
                };
                if !label_ids.contains(&label_id) {
                    continue;
                }
                if let Some(v) = props.get(&c.property_key) {
                    if matches!(v, serde_json::Value::Null) {
                        continue;
                    }
                    if !c.ty.accepts(v) {
                        return self.maybe_violation(format!(
                            "ERR_CONSTRAINT_VIOLATED: kind=PROPERTY_TYPE property={:?} \
                             expected={} got={}",
                            c.property_key,
                            c.ty.name(),
                            super::json_type_label(v),
                        ));
                    }
                }
            }
        }

        // NODE KEY: each property present + non-null, tuple unique.
        for nk in &self.node_key_constraints {
            if !label_ids.contains(&nk.label_id) {
                continue;
            }
            let obj = match properties.as_object() {
                Some(m) => m,
                None => continue,
            };
            let mut tuple_vals: Vec<crate::index::PropertyValue> = Vec::new();
            for p in &nk.property_keys {
                match obj.get(p) {
                    None | Some(serde_json::Value::Null) => {
                        return self.maybe_violation(format!(
                            "ERR_CONSTRAINT_VIOLATED: kind=NODE_KEY property={p:?} is NULL"
                        ));
                    }
                    Some(v) => tuple_vals.push(super::json_to_property_value(v)),
                }
            }
            // Uniqueness against the composite B-tree registry.
            if let Some(idx) = self
                .indexes
                .composite_btree
                .find(nk.label_id, &nk.property_keys)
            {
                let hits = idx.read().seek_exact(&tuple_vals);
                if hits.iter().any(|id| Some(*id) != exclude_node_id) {
                    return self.maybe_violation(format!(
                        "ERR_CONSTRAINT_VIOLATED: kind=NODE_KEY tuple={:?} not unique",
                        nk.property_keys,
                    ));
                }
            }
        }
        Ok(())
    }

    /// Fire extra enforcement for relationship writes. Applies
    /// relationship NOT NULL + property-type constraints.
    pub(crate) fn enforce_rel_constraints(
        &self,
        rel_type_id: u32,
        properties: &serde_json::Value,
    ) -> Result<()> {
        let obj = properties.as_object();
        for c in &self.rel_not_null_constraints {
            if c.rel_type_id != rel_type_id {
                continue;
            }
            let v = obj.and_then(|m| m.get(&c.property_key));
            if !matches!(v, Some(v) if !matches!(v, serde_json::Value::Null)) {
                return self.maybe_violation(format!(
                    "ERR_CONSTRAINT_VIOLATED: kind=RELATIONSHIP_PROPERTY_EXISTENCE \
                     property={:?} must be non-null",
                    c.property_key,
                ));
            }
        }
        if let Some(obj) = obj {
            for c in &self.property_type_constraints {
                let Some(target) = c.rel_type_id else {
                    continue;
                };
                if target != rel_type_id {
                    continue;
                }
                if let Some(v) = obj.get(&c.property_key) {
                    if matches!(v, serde_json::Value::Null) {
                        continue;
                    }
                    if !c.ty.accepts(v) {
                        return self.maybe_violation(format!(
                            "ERR_CONSTRAINT_VIOLATED: kind=PROPERTY_TYPE (rel) \
                             property={:?} expected={} got={}",
                            c.property_key,
                            c.ty.name(),
                            super::json_type_label(v),
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    /// Reject writes that would remove a required property / set it to
    /// NULL. Called from `apply_set_clause` / `apply_remove_clause`.
    pub(crate) fn enforce_not_null_on_prop_change(
        &self,
        label_ids: &[u32],
        property_key: &str,
        new_value: Option<&serde_json::Value>,
    ) -> Result<()> {
        // Legacy EXISTS constraint via the catalog.
        let mgr = self.catalog.constraint_manager().read();
        for label_id in label_ids {
            let cs = mgr.get_constraints_for_label(*label_id)?;
            for c in cs {
                if matches!(
                    c.constraint_type,
                    catalog::constraints::ConstraintType::Exists
                ) {
                    let name = self
                        .catalog
                        .get_key_name(c.property_key_id)?
                        .unwrap_or_default();
                    if name == property_key
                        && matches!(new_value, None | Some(serde_json::Value::Null))
                    {
                        return self.maybe_violation(format!(
                            "ERR_CONSTRAINT_VIOLATED: kind=NODE_PROPERTY_EXISTENCE \
                             property={property_key:?} must be non-null",
                        ));
                    }
                }
            }
        }
        // NODE KEY: each component is implicitly NOT NULL.
        for nk in &self.node_key_constraints {
            if !label_ids.contains(&nk.label_id) {
                continue;
            }
            if nk.property_keys.iter().any(|p| p == property_key)
                && matches!(new_value, None | Some(serde_json::Value::Null))
            {
                return self.maybe_violation(format!(
                    "ERR_CONSTRAINT_VIOLATED: kind=NODE_KEY component={property_key:?} \
                     cannot be NULL",
                ));
            }
        }
        Ok(())
    }

    /// Reject a label-add when the constraint on that label is
    /// unsatisfied by the current property map.
    pub(crate) fn enforce_add_label_constraints(
        &self,
        label: &str,
        properties: &serde_json::Map<String, serde_json::Value>,
    ) -> Result<()> {
        let label_id = match self.catalog.get_label_id(label) {
            Ok(id) => id,
            Err(_) => return Ok(()), // label not catalogued yet → no constraint can target it
        };
        // Legacy EXISTS constraints.
        let mgr = self.catalog.constraint_manager().read();
        let cs = mgr.get_constraints_for_label(label_id)?;
        for c in cs {
            if matches!(
                c.constraint_type,
                catalog::constraints::ConstraintType::Exists
            ) {
                let prop = self
                    .catalog
                    .get_key_name(c.property_key_id)?
                    .unwrap_or_default();
                if !matches!(properties.get(&prop), Some(v) if !matches!(v, serde_json::Value::Null))
                {
                    return self.maybe_violation(format!(
                        "ERR_CONSTRAINT_VIOLATED: kind=NODE_PROPERTY_EXISTENCE label={label:?} \
                         property={prop:?} missing while adding label",
                    ));
                }
            }
        }
        drop(mgr);
        // NODE KEY constraints.
        for nk in &self.node_key_constraints {
            if nk.label_id != label_id {
                continue;
            }
            for p in &nk.property_keys {
                if !matches!(properties.get(p), Some(v) if !matches!(v, serde_json::Value::Null)) {
                    return self.maybe_violation(format!(
                        "ERR_CONSTRAINT_VIOLATED: kind=NODE_KEY label={label:?} component={p:?} \
                         missing while adding label",
                    ));
                }
            }
        }
        // Property-type constraints scoped to this label.
        for c in &self.property_type_constraints {
            if c.label_id != Some(label_id) {
                continue;
            }
            if let Some(v) = properties.get(&c.property_key) {
                if matches!(v, serde_json::Value::Null) {
                    continue;
                }
                if !c.ty.accepts(v) {
                    return self.maybe_violation(format!(
                        "ERR_CONSTRAINT_VIOLATED: kind=PROPERTY_TYPE label={label:?} \
                         property={:?} expected={} got={}",
                        c.property_key,
                        c.ty.name(),
                        super::json_type_label(v),
                    ));
                }
            }
        }
        Ok(())
    }

    /// Resolve the pending label-set on a `NodeWriteState` into the
    /// catalog's u32 IDs. Missing labels are skipped (they aren't
    /// catalogued yet, so no constraint can target them).
    pub(crate) fn label_ids_for_state(
        &self,
        state: &super::crud::NodeWriteState,
    ) -> Result<Vec<u32>> {
        let mut out = Vec::with_capacity(state.labels.len());
        for lbl in &state.labels {
            if let Ok(id) = self.catalog.get_label_id(lbl) {
                out.push(id);
            }
        }
        Ok(out)
    }

    pub(super) fn maybe_violation(&self, message: String) -> Result<()> {
        if self.relaxed_constraint_enforcement {
            tracing::warn!("relaxed_constraint_enforcement: {message}");
            Ok(())
        } else {
            Err(Error::ConstraintViolation(message))
        }
    }

    /// Check constraints before creating or updating a node
    pub(super) fn check_constraints(
        &self,
        label_ids: &[u32],
        properties: &serde_json::Value,
        exclude_node_id: Option<u64>,
    ) -> Result<()> {
        // phase6_opencypher-advanced-types §4.3 — typed-list
        // constraint enforcement. Run first so a clearly-typed
        // violation short-circuits before we touch the single-column
        // UNIQUE / EXISTS machinery.
        if !self.typed_list_constraints.is_empty() {
            if let Some(props) = properties.as_object() {
                for &label_id in label_ids {
                    for ((lbl, key_id), elem_type) in &self.typed_list_constraints {
                        if *lbl != label_id {
                            continue;
                        }
                        let key_name = match self.catalog.get_key_name(*key_id)? {
                            Some(n) => n,
                            None => continue,
                        };
                        if let Some(val) = props.get(&key_name) {
                            typed_collections::validate_list(val, *elem_type)?;
                        }
                    }
                }
            }
        }

        let constraint_manager = self.catalog.constraint_manager().read();

        // Check constraints for each label
        for &label_id in label_ids {
            let constraints = constraint_manager.get_constraints_for_label(label_id)?;

            for constraint in constraints {
                // Get property value
                let property_name = self
                    .catalog
                    .get_key_name(constraint.property_key_id)?
                    .ok_or_else(|| Error::Internal("Property key not found".to_string()))?;

                let property_value = properties.as_object().and_then(|m| m.get(&property_name));

                match constraint.constraint_type {
                    catalog::constraints::ConstraintType::Exists => {
                        // Property must exist (not null)
                        if property_value.is_none()
                            || property_value == Some(&serde_json::Value::Null)
                        {
                            let label_name = self
                                .catalog
                                .get_label_name(label_id)?
                                .unwrap_or_else(|| format!("ID{}", label_id));
                            return Err(Error::ConstraintViolation(format!(
                                "EXISTS constraint violated: property '{}' must exist on nodes with label '{}'",
                                property_name, label_name
                            )));
                        }
                    }
                    catalog::constraints::ConstraintType::Unique => {
                        // Property value must be unique across all nodes with this label
                        if let Some(value) = property_value {
                            // Check if any other node with this label has the same property value
                            let label_name = self
                                .catalog
                                .get_label_name(label_id)?
                                .unwrap_or_else(|| format!("ID{}", label_id));

                            // Get all nodes with this label
                            let bitmap = self
                                .indexes
                                .label_index
                                .get_nodes_with_labels(&[label_id])?;

                            for node_id in bitmap.iter() {
                                let node_id_u64 = node_id as u64;

                                // Skip the node being updated
                                if Some(node_id_u64) == exclude_node_id {
                                    continue;
                                }

                                let node_props = self.storage.load_node_properties(node_id_u64)?;
                                if let Some(serde_json::Value::Object(props_map)) = node_props {
                                    if let Some(existing_value) = props_map.get(&property_name) {
                                        if existing_value == value {
                                            return Err(Error::ConstraintViolation(format!(
                                                "UNIQUE constraint violated: property '{}' value already exists on another node with label '{}'",
                                                property_name, label_name
                                            )));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }
}
