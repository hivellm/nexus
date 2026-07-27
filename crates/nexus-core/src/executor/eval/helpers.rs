//! Evaluation helpers that sit between the operator layer and the
//! row-level evaluator. Includes Cartesian-product application,
//! row↔variable materialisation, EXISTS-style pattern checks, entity
//! ID extraction, relationship value serialisation, and the context
//! expression evaluator used before operators have materialised rows.

use super::super::context::{ExecutionContext, RelationshipInfo};
use super::super::engine::Executor;
use super::super::operators::path::MAX_VAR_LENGTH_PATH_DEPTH;
use super::super::parser;
use super::super::push_with_row_cap;
use super::super::types::{Direction, Row};
use crate::storage::RecordStore;
use crate::{Error, Result};
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};

/// Candidate-resolution outcome for the node that anchors a fresh
/// component of an `EXISTS { … }` pattern probe (the pattern's first
/// element, or a node that starts a new comma-separated pattern part).
/// `Null` signals that the anchor is correlated to an outer variable
/// that is bound but currently `NULL` — under Cypher's three-valued
/// logic this must propagate a `NULL` result rather than `false`.
enum ExistsAnchor {
    Null,
    Ids(Vec<u64>),
}

/// Outcome of testing a single candidate node against a pattern node's
/// constraints (labels, properties, and — when the pattern reuses an
/// already-bound variable name — identity with the existing binding).
enum ExistsAcceptOutcome {
    /// The candidate satisfies every constraint; carries the row
    /// extended with the node's variable (unchanged if it was already
    /// bound).
    Accepted(HashMap<String, Value>),
    /// The candidate fails a structural constraint (deleted, wrong
    /// label, property mismatch, or conflicts with an existing
    /// same-name binding).
    Rejected,
    /// The pattern reuses a variable name that is already bound in the
    /// row to `NULL` (e.g. correlated to an unmatched `OPTIONAL
    /// MATCH`) — under three-valued logic the candidate's fate is
    /// unknown, not rejected.
    Null,
}

/// Three-valued result of probing an `EXISTS` pattern (or a sub-step of
/// one): `True` once a witness binding is found, `False` when the
/// search is exhausted without one, and `Null` when a required
/// correlated variable was `NULL` along every path that was tried and
/// no path produced `True`. Mirrors Cypher's `NULL` propagation for
/// pattern predicates: `NULL` composes correctly under `NOT` and is
/// filtered out (treated as not-true) by `WHERE`, exactly like any
/// other `NULL` boolean expression.
enum ExistsOutcome {
    True,
    False,
    Null,
}

impl ExistsOutcome {
    fn into_value(self) -> Value {
        match self {
            Self::True => Value::Bool(true),
            Self::False => Value::Bool(false),
            Self::Null => Value::Null,
        }
    }
}

/// Loop-invariant arguments for [`Executor::exists_probe_var_length`]'s
/// depth-first walk of a variable-length relationship hop. Everything
/// here stays fixed across the whole recursive walk of one `-[:T*m..n]->`
/// segment; only the binding, frontier node id, depth, and the shared
/// `bound_relationships` set change per recursive call. Bundled into one
/// borrowed struct so the recursive function itself stays under
/// clippy's argument-count lint without duplicating any of these values.
struct ExistsVarLengthWalk<'a> {
    context: &'a ExecutionContext,
    elements: &'a [parser::PatternElement],
    /// Index of the `Relationship` element itself within `elements` —
    /// the following node lives at `elements[pos + 1]`
    /// (`next_node`, cached separately below) and the rest of the
    /// pattern resumes at `pos + 2` once a witness for this segment is
    /// found.
    pos: usize,
    rel: &'a parser::RelationshipPattern,
    next_node: &'a parser::NodePattern,
    min_hops: usize,
    max_hops: usize,
    /// `false` when the declared relationship type(s) never resolved
    /// to a real catalog type id — see the doc comment on
    /// [`Executor::exists_probe_var_length`].
    can_extend: bool,
    type_ids: &'a [u32],
    direction: Direction,
    where_clause: Option<&'a parser::Expression>,
}

impl Executor {
    pub(in crate::executor) fn evaluate_expression_in_context(
        &self,
        context: &ExecutionContext,
        expr: &parser::Expression,
    ) -> Result<Value> {
        // Fast path — literals & unadorned variables avoid the full
        // projection evaluator's row-level setup. Everything else
        // (LIST / MAP literals, FunctionCall, BinaryOp, nested
        // procedure arguments such as `apoc.coll.union([1,2],[3,4])`
        // or `apoc.map.merge({a:1}, {b:2})`) routes through
        // `evaluate_projection_expression` with an empty row, which
        // is the same evaluator RETURN / WITH / WHERE clauses use.
        match expr {
            parser::Expression::Literal(parser::Literal::String(s)) => Ok(Value::String(s.clone())),
            parser::Expression::Literal(parser::Literal::Integer(i)) => {
                Ok(Value::Number((*i).into()))
            }
            parser::Expression::Literal(parser::Literal::Float(f)) => Ok(Value::Number(
                serde_json::Number::from_f64(*f).unwrap_or_else(|| 0.into()),
            )),
            parser::Expression::Literal(parser::Literal::Boolean(b)) => Ok(Value::Bool(*b)),
            parser::Expression::Literal(parser::Literal::Null) => Ok(Value::Null),
            parser::Expression::Literal(parser::Literal::Point(p)) => Ok(p.to_json_value()),
            parser::Expression::Variable(var) => context
                .get_variable(var)
                .cloned()
                .ok_or_else(|| Error::CypherSyntax(format!("Variable '{}' not found", var))),
            _ => {
                let empty_row: std::collections::HashMap<String, Value> =
                    std::collections::HashMap::new();
                self.evaluate_projection_expression(&empty_row, context, expr)
            }
        }
    }

    /// Apply Cartesian product of new values with existing variables in context
    /// This expands all existing array variables by repeating each element M times (where M is new_values.len())
    /// and creates the new variable by repeating the whole sequence N times (where N is existing row count).
    pub(in crate::executor) fn apply_cartesian_product(
        &self,
        context: &mut ExecutionContext,
        new_var: &str,
        new_values: Vec<Value>,
    ) -> Result<()> {
        // 1. Determine current row count (N)
        // Find the length of the first array variable
        let current_count = context
            .variables
            .values()
            .filter_map(|v| {
                if let Value::Array(arr) = v {
                    Some(arr.len())
                } else {
                    None
                }
            })
            .max() // Use max just in case, though they should be equal
            .unwrap_or(0);

        if current_count == 0 {
            // No existing rows (or only scalars), just set the new variable
            context.set_variable(new_var, Value::Array(new_values));
            return Ok(());
        }

        let new_count = new_values.len();
        if new_count == 0 {
            // New set is empty -> Cartesian product is empty
            // Clear all variables to empty arrays
            for val in context.variables.values_mut() {
                *val = Value::Array(Vec::new());
            }
            context.set_variable(new_var, Value::Array(Vec::new()));
            return Ok(());
        }

        // Audit (phase0_fix-cypher-oom-process-abort §3.3): this function has
        // exactly two sites that size an allocation from a product of counts
        // rather than from data already in hand — the per-column rebuild
        // below (`Vec::with_capacity(arr.len() * new_count)`) and the
        // new-variable expansion further down
        // (`Vec::with_capacity(new_count * current_count)`). Both derive
        // their length from the same `current_count * new_count` product
        // computed here, so a single pre-allocation check bounds both. The
        // clone loops that follow only push into these pre-sized vecs and
        // never allocate beyond them. No other allocation in this function
        // is sized from a product of counts.
        //
        // Check the size BEFORE allocating: `Vec::with_capacity` on an
        // unchecked product aborts the process rather than failing the
        // query — an UNWIND of 5 000 rows over two 5 000-node patterns
        // reaches 1.25e11 cells and asks the allocator for ~4 TB. The
        // budget is expressed in bytes, not rows, because the true cost is
        // `rows * size_of::<Value>() * columns` and a row limit means a
        // different amount of memory for a 2-column context than for a
        // 20-column one.
        let product = current_count.checked_mul(new_count).ok_or_else(|| {
            Error::OutOfMemory(format!(
                "Cartesian product {} x {} overflows usize; add LIMIT or narrow the query",
                current_count, new_count
            ))
        })?;

        // Every existing variable is rebuilt to `product` length, plus the
        // new variable itself adds one more column.
        let columns = context.variables.len() + 1;
        let est_bytes = product
            .checked_mul(columns)
            .and_then(|cells| cells.checked_mul(std::mem::size_of::<Value>()));

        let budget = self.config.cartesian_product_max_bytes;
        match est_bytes {
            Some(bytes) if bytes <= budget => {}
            Some(bytes) => {
                return Err(Error::OutOfMemory(format!(
                    "Cartesian product would materialise {} rows ({} x {}) across {} \
                     columns (~{} bytes), exceeding the configured budget of {} bytes; \
                     add LIMIT or narrow the query",
                    product, current_count, new_count, columns, bytes, budget
                )));
            }
            None => {
                return Err(Error::OutOfMemory(format!(
                    "Cartesian product would materialise {} rows ({} x {}) across {} \
                     columns, and the estimated byte size overflows usize, far exceeding \
                     the configured budget of {} bytes; add LIMIT or narrow the query",
                    product, current_count, new_count, columns, budget
                )));
            }
        }

        // 2. Expand existing variables: repeat each element M times (M = new_count)
        // We need to collect keys first to avoid borrowing issues
        let keys: Vec<String> = context.variables.keys().cloned().collect();

        for key in keys {
            if let Some(val) = context.variables.get_mut(&key) {
                if let Value::Array(arr) = val {
                    let mut new_arr = Vec::with_capacity(arr.len() * new_count);
                    for item in arr.iter() {
                        for _ in 0..new_count {
                            new_arr.push(item.clone());
                        }
                    }
                    *val = Value::Array(new_arr);
                }
            }
        }

        // 3. Expand new variable: repeat the whole sequence N times (N = current_count)
        let mut expanded_new_values = Vec::with_capacity(new_count * current_count);
        for _ in 0..current_count {
            expanded_new_values.extend(new_values.clone());
        }
        context.set_variable(new_var, Value::Array(expanded_new_values));

        Ok(())
    }

    pub(in crate::executor) fn materialize_rows_from_variables(
        &self,
        context: &ExecutionContext,
    ) -> Result<Vec<HashMap<String, Value>>> {
        // TRACE: Log variables before creating cartesian product
        let mut has_relationships = false;
        let mut var_types: Vec<(String, String)> = Vec::new();
        for (var, value) in &context.variables {
            let var_type = match value {
                Value::Object(_) => {
                    if crate::executor::is_relationship_value(value) {
                        has_relationships = true;
                        "RELATIONSHIP".to_string()
                    } else {
                        "NODE".to_string()
                    }
                }
                Value::Array(arr) => {
                    let has_rel = arr.iter().any(crate::executor::is_relationship_value);
                    if has_rel {
                        has_relationships = true;
                    }
                    format!(
                        "ARRAY({})",
                        if has_rel {
                            "HAS_RELATIONSHIPS"
                        } else {
                            "NODES_ONLY"
                        }
                    )
                }
                _ => "OTHER".to_string(),
            };
            var_types.push((var.clone(), var_type));
        }
        tracing::trace!(
            "materialize_rows_from_variables: variables={:?}, has_relationships={}, creating cartesian product",
            var_types,
            has_relationships
        );

        let mut arrays: HashMap<String, Vec<Value>> = HashMap::new();

        for (var, value) in &context.variables {
            match value {
                Value::Array(values) => {
                    // Only include non-empty arrays
                    if !values.is_empty() {
                        arrays.insert(var.clone(), values.clone());
                    }
                }
                other => {
                    // Include non-null single values
                    if !matches!(other, Value::Null) {
                        arrays.insert(var.clone(), vec![other.clone()]);
                    }
                }
            }
        }

        if arrays.is_empty() {
            return Ok(Vec::new());
        }

        // CRITICAL FIX: Implement true cartesian product instead of zip
        // When we have multiple node arrays (e.g., p1=[Alice, Bob], c2=[Acme, TechCorp]),
        // we need ALL combinations (4 rows), not just pairs (2 rows)

        // Check if all arrays have the same length and all are nodes (not single values)
        let all_same_len = arrays
            .values()
            .map(|v| v.len())
            .collect::<std::collections::HashSet<_>>()
            .len()
            == 1;
        let has_multiple_arrays = arrays.len() > 1;
        let all_multi_element = arrays.values().all(|v| v.len() > 1);

        let needs_cartesian_product = has_multiple_arrays && all_multi_element && all_same_len;

        if needs_cartesian_product {
            // TRUE CARTESIAN PRODUCT: Generate ALL combinations
            let var_names: Vec<String> = arrays.keys().cloned().collect();
            let array_values: Vec<Vec<Value>> =
                var_names.iter().map(|k| arrays[k].clone()).collect();

            // Calculate total number of combinations with CHECKED
            // arithmetic — the unguarded `.product()` this replaced
            // wraps silently on overflow in a release build (overflow
            // checks are off) and panics in debug, neither of which is
            // a Cypher error. Mirrors the same checked-multiplication +
            // byte-budget precheck `apply_cartesian_product` (above,
            // same file) already applies, so both cross-product sources
            // in this module share one budget contract. Do NOT change
            // `apply_cartesian_product`'s own guard/message — this is a
            // parallel check, not a shared call, because the two
            // functions size their `columns` differently (`var_names.len()`
            // here vs. `context.variables.len() + 1` there).
            let mut total_combinations: usize = 1;
            for arr in &array_values {
                total_combinations =
                    total_combinations.checked_mul(arr.len()).ok_or_else(|| {
                        Error::OutOfMemory(format!(
                            "materialize_rows_from_variables: cartesian product across {} \
                         variables overflows usize; add LIMIT or narrow the query",
                            array_values.len()
                        ))
                    })?;
            }

            let columns = var_names.len();
            let est_bytes = total_combinations
                .checked_mul(columns)
                .and_then(|cells| cells.checked_mul(std::mem::size_of::<Value>()));

            let budget = self.config.cartesian_product_max_bytes;
            match est_bytes {
                Some(bytes) if bytes <= budget => {}
                Some(bytes) => {
                    return Err(Error::OutOfMemory(format!(
                        "materialize_rows_from_variables would produce {total_combinations} \
                         rows across {columns} columns (~{bytes} bytes), exceeding the \
                         configured budget of {budget} bytes; add LIMIT or narrow the query"
                    )));
                }
                None => {
                    return Err(Error::OutOfMemory(format!(
                        "materialize_rows_from_variables would produce {total_combinations} \
                         rows across {columns} columns, and the estimated byte size overflows \
                         usize, far exceeding the configured budget of {budget} bytes; add \
                         LIMIT or narrow the query"
                    )));
                }
            }

            let mut rows = Vec::with_capacity(total_combinations);

            // Generate all combinations using nested iteration
            let mut indices = vec![0usize; array_values.len()];

            loop {
                // Create a row from current indices
                let mut row = HashMap::new();
                for (i, var_name) in var_names.iter().enumerate() {
                    let value = array_values[i][indices[i]].clone();
                    row.insert(var_name.clone(), value);
                }
                rows.push(row);

                // Increment indices (like odometer)
                let mut carry = true;
                for i in (0..indices.len()).rev() {
                    if carry {
                        indices[i] += 1;
                        if indices[i] < array_values[i].len() {
                            carry = false;
                        } else {
                            indices[i] = 0;
                        }
                    }
                }

                // If carry is still true, we've exhausted all combinations
                if carry {
                    break;
                }
            }

            return Ok(rows);
        }

        // FALLBACK: Old zip-based logic for single arrays or mixed sizes.
        // Bounded by `max_len` (not a product of array lengths), so no
        // cartesian-style guard is needed here.
        let max_len = arrays
            .values()
            .map(|values| values.len())
            .max()
            .unwrap_or(0);

        if max_len == 0 {
            return Ok(Vec::new());
        }

        let mut rows = Vec::new();

        for idx in 0..max_len {
            let mut row = HashMap::new();
            let mut all_null = true;
            let mut entity_ids = Vec::new();

            for (var, values) in &arrays {
                let value = if values.len() == max_len {
                    values.get(idx).cloned().unwrap_or(Value::Null)
                } else if values.len() == 1 {
                    values.first().cloned().unwrap_or(Value::Null)
                } else {
                    // For arrays with different lengths, only use value if index exists
                    if idx < values.len() {
                        values.get(idx).cloned().unwrap_or(Value::Null)
                    } else {
                        Value::Null
                    }
                };

                // Track if row has at least one non-null value
                if !matches!(value, Value::Null) {
                    all_null = false;

                    // Extract entity ID (node or relationship) for deduplication
                    if let Value::Object(obj) = &value {
                        if let Some(Value::Number(id)) = obj.get("_nexus_id") {
                            if let Some(nid) = id.as_u64() {
                                entity_ids.push(nid);
                            }
                        }
                    }
                }

                row.insert(var.clone(), value);
            }

            // Add row if it has content and is not a duplicate
            if !all_null {
                /*
                let is_duplicate = if !entity_ids.is_empty() {
                    // Sort IDs to ensure consistent key regardless of column order
                    entity_ids.sort();
                    let key = entity_ids
                        .iter()
                        .map(|id| id.to_string())
                        .collect::<Vec<String>>()
                        .join("_");
                    !seen_row_keys.insert(key)
                } else {
                    // Fallback for rows without entities (e.g. literals) - no deduplication or full content deduplication?
                    // For now, allow all since we can't identify entities
                    false
                };

                if !is_duplicate {
                    rows.push(row);
                }
                */
                // DEBUG: Disable deduplication to see if rows are being generated
                rows.push(row);
            }
        }

        Ok(rows)
    }

    /// Materialises rows by ZIPPING already-aligned column variables — the
    /// index-aligned counterpart to the cross-producing path in
    /// [`Self::materialize_rows_from_variables`].
    ///
    /// Called right after [`Self::apply_cartesian_product`], which leaves
    /// every array variable aligned to the SAME product length (index `i`
    /// is one output row). Running the general materialiser there instead
    /// would hit its `needs_cartesian_product` branch and RE-cross the
    /// already-crossed columns into `N^k` rows (`384^3 ≈ 56.6M` for a
    /// two-pattern `MATCH` over an 8-node label with 6 driving rows — a
    /// ~13 GB allocation that freezes the host). Zipping returns the `N`
    /// rows those aligned columns already represent.
    /// (phase0_fix-materialize-recrosses-aligned-columns)
    ///
    /// Length-1 arrays and scalars broadcast across all rows, matching the
    /// fallback (zip) semantics of [`Self::materialize_rows_from_variables`];
    /// all-`Null` rows are dropped identically.
    pub(in crate::executor) fn materialize_aligned_rows(
        &self,
        context: &ExecutionContext,
    ) -> Vec<HashMap<String, Value>> {
        let mut arrays: HashMap<String, Vec<Value>> = HashMap::new();
        for (var, value) in &context.variables {
            match value {
                Value::Array(values) => {
                    if !values.is_empty() {
                        arrays.insert(var.clone(), values.clone());
                    }
                }
                other => {
                    if !matches!(other, Value::Null) {
                        arrays.insert(var.clone(), vec![other.clone()]);
                    }
                }
            }
        }

        if arrays.is_empty() {
            return Vec::new();
        }

        let max_len = arrays.values().map(|v| v.len()).max().unwrap_or(0);
        if max_len == 0 {
            return Vec::new();
        }

        let mut rows = Vec::with_capacity(max_len);
        for idx in 0..max_len {
            let mut row = HashMap::new();
            let mut all_null = true;
            for (var, values) in &arrays {
                let value = if values.len() == max_len {
                    values.get(idx).cloned().unwrap_or(Value::Null)
                } else if values.len() == 1 {
                    values[0].clone()
                } else if idx < values.len() {
                    values[idx].clone()
                } else {
                    Value::Null
                };
                if !matches!(value, Value::Null) {
                    all_null = false;
                }
                row.insert(var.clone(), value);
            }
            if !all_null {
                rows.push(row);
            }
        }
        rows
    }

    pub(in crate::executor) fn update_result_set_from_rows(
        &self,
        context: &mut ExecutionContext,
        rows: &[HashMap<String, Value>],
    ) {
        // TRACE: Check if input rows contain relationships
        let mut rows_with_relationships = 0;
        for row in rows {
            let has_rel = row.values().any(crate::executor::is_relationship_value);
            if has_rel {
                rows_with_relationships += 1;
            }
        }

        // CRITICAL FIX: Only use columns from rows, not from context.variables
        // Context variables may contain old/unused variables that cause null rows
        // Only include variables that are actually present in the rows
        let mut columns: std::collections::HashSet<String> = std::collections::HashSet::new();
        for row in rows {
            columns.extend(row.keys().cloned());
        }

        // Don't include variables from context - they may be stale
        // Only use what's actually in the rows

        let mut columns: Vec<String> = columns.into_iter().collect();
        columns.sort();

        // CRITICAL FIX: Deduplicate rows intelligently - consider full row content for relationship rows
        // When we have relationships (multiple rows with same source node), we need to check the full row
        // content, not just the source node ID, to avoid removing valid relationship rows
        use std::collections::HashSet;
        let mut seen_row_keys = HashSet::new();
        let mut unique_rows = Vec::new();

        for row_map in rows {
            // Collect all entity IDs (nodes and relationships) in this row
            // CRITICAL FIX: Extract all _nexus_id values, which can be from nodes or relationships
            // For relationship rows, we need to use ALL IDs (source node + target node + relationship)
            // to correctly differentiate between different relationships
            let mut all_entity_ids: Vec<u64> = Vec::new();

            // Extract all _nexus_id values from the row (both nodes and relationships have this)
            for value in row_map.values() {
                if let Value::Object(obj) = value {
                    if let Some(Value::Number(id)) = obj.get("_nexus_id") {
                        if let Some(entity_id) = id.as_u64() {
                            all_entity_ids.push(entity_id);
                        }
                    }
                }
            }

            // Fold non-entity columns (values with no `_nexus_id`, e.g. an
            // `UNWIND` driving map like `{s: 10}`) into the dedup key. Without
            // it, two rows that matched the SAME nodes from DIFFERENT driving
            // rows collapse to one, dropping every driving row after the first
            // — the truncation that surfaced once the aligned multi-pattern
            // path stopped re-crossing into `N^k`
            // (phase0_fix-materialize-recrosses-aligned-columns). Keying by
            // content only makes keys MORE specific (keeps more rows), which is
            // the correct direction: Cypher `MATCH` does not deduplicate rows.
            let non_entity_suffix = {
                let mut parts: Vec<String> = row_map
                    .iter()
                    .filter(|(_, v)| {
                        !matches!(v, Value::Object(o) if o.contains_key("_nexus_id"))
                            && !matches!(v, Value::Null)
                    })
                    .map(|(k, v)| format!("{}={}", k, serde_json::to_string(v).unwrap_or_default()))
                    .collect();
                parts.sort();
                parts.join("|")
            };

            // CRITICAL FIX: Determine deduplication key based on number of entity IDs
            // Relationship rows typically have multiple entity IDs (source node + target node + relationship)
            // Non-relationship rows have only one entity ID (just the node)
            let is_duplicate = if all_entity_ids.len() > 1 {
                // Relationship row or row with multiple entities
                // CRITICAL FIX: Find relationship ID and use it as primary key for deduplication
                // This ensures that rows with the same relationship ID are considered duplicates
                // even if they appear in different contexts (e.g., bidirectional relationships from source vs target)
                let relationship_id = row_map.values().find_map(|value| {
                    // Structural check. A NODE carrying a property named
                    // `type` (LDBC `Organisation.type`) used to be picked
                    // here, in HashMap order, and the key below then
                    // dropped both real node variables, collapsing
                    // unrelated rows into one.
                    if crate::executor::is_relationship_value(value) {
                        if let Value::Object(obj) = value {
                            if let Some(Value::Number(nid)) = obj.get("_nexus_id") {
                                return nid.as_u64();
                            }
                        }
                    }
                    None
                });

                if let Some(rel_id) = relationship_id {
                    // CRITICAL FIX: For relationship rows, use relationship ID + variable values
                    // This ensures that rows with same relationship ID but different variable assignments
                    // are not considered duplicates (e.g., bidirectional relationships: a=778,b=779 vs a=779,b=778)
                    // Build key using relationship ID + sorted list of variable names and their node IDs
                    let mut var_entries: Vec<(String, u64)> = Vec::new();

                    for (key, value) in row_map {
                        if let Value::Object(obj) = value {
                            if let Some(Value::Number(nid)) = obj.get("_nexus_id") {
                                if let Some(entity_id) = nid.as_u64() {
                                    // Every node variable belongs in the
                                    // key; skip only the relationship
                                    // itself, already keyed above.
                                    if entity_id != rel_id
                                        && !crate::executor::is_relationship_value(value)
                                    {
                                        var_entries.push((key.clone(), entity_id));
                                    }
                                }
                            }
                        }
                    }

                    // Sort variable entries by variable name for consistent key generation
                    var_entries.sort_by(|a, b| a.0.cmp(&b.0));

                    // Build deduplication key: rel_{id}_{var1}_{id1}_{var2}_{id2}...
                    let mut key_parts = vec![format!("rel_{}", rel_id)];
                    for (var_name, var_id) in &var_entries {
                        key_parts.push(format!("{}_{}", var_name, var_id));
                    }
                    if !non_entity_suffix.is_empty() {
                        key_parts.push(non_entity_suffix.clone());
                    }
                    let row_key = key_parts.join("_");

                    let is_dup = !seen_row_keys.insert(row_key.clone());
                    is_dup
                } else {
                    // Fallback: Can't find rel_id but have multiple entities - include variables in key
                    // This handles bidirectional relationships where we need to differentiate by variable assignment
                    let mut var_entries: Vec<(String, u64)> = Vec::new();

                    for (key, value) in row_map {
                        if let Value::Object(obj) = value {
                            if let Some(Value::Number(nid)) = obj.get("_nexus_id") {
                                if let Some(entity_id) = nid.as_u64() {
                                    // Include all entities with their variable names
                                    var_entries.push((key.clone(), entity_id));
                                }
                            }
                        }
                    }

                    // Sort by variable name for consistent key generation
                    var_entries.sort_by(|a, b| a.0.cmp(&b.0));

                    // Build key: var1_id1_var2_id2_var3_id3...
                    let mut key_parts: Vec<String> = var_entries
                        .iter()
                        .map(|(var_name, var_id)| format!("{}_{}", var_name, var_id))
                        .collect();
                    if !non_entity_suffix.is_empty() {
                        key_parts.push(non_entity_suffix.clone());
                    }
                    let row_key = key_parts.join("_");

                    let is_dup = !seen_row_keys.insert(row_key.clone());
                    is_dup
                }
            } else if let Some(first_id) = all_entity_ids.first() {
                // Non-relationship row - but check if this is from OPTIONAL MATCH (has NULL values)
                // CRITICAL FIX: For OPTIONAL MATCH NULL rows, include NULL variable names in key
                // to prevent incorrect deduplication of different source nodes
                let has_null_values = row_map.values().any(|v| matches!(v, Value::Null));

                if has_null_values {
                    // OPTIONAL MATCH NULL row - include all variable names and their values/NULL status
                    let mut var_entries: Vec<String> = Vec::new();
                    for (key, value) in row_map {
                        if let Value::Object(obj) = value {
                            if let Some(Value::Number(nid)) = obj.get("_nexus_id") {
                                if let Some(entity_id) = nid.as_u64() {
                                    var_entries.push(format!("{}_{}", key, entity_id));
                                }
                            }
                        } else if matches!(value, Value::Null) {
                            var_entries.push(format!("{}_null", key));
                        }
                    }
                    var_entries.sort();
                    let row_key = var_entries.join("_");
                    !seen_row_keys.insert(row_key)
                } else {
                    // Regular non-relationship row - use only entity ID
                    let entity_key = format!("node_{}", first_id);
                    !seen_row_keys.insert(entity_key)
                }
            } else {
                // No entity IDs found - use full row content as fallback dedup
                // key. If JSON serialisation fails (usually: non-finite floats
                // in a property map) we fall back to a `{:?}` key rather than
                // the empty string; otherwise every failing row collapses into
                // a single dedup bucket. A warn! + metric marks the event.
                //
                // This helper returns `()` with 18 call sites — propagating
                // errors here would be a wide cascade. The failure is
                // confined to the dedup decision, so degrading to Rust Debug
                // for the key is a safe compromise (different values still
                // produce different strings).
                let row_key = match serde_json::to_string(row_map) {
                    Ok(s) => s,
                    Err(e) => {
                        super::super::serde_metrics::record_fallback(
                            super::super::serde_metrics::SerdeFallbackSite::HelperRowDedupKey,
                        );
                        tracing::warn!(
                            target: "nexus_core::executor",
                            error = %e,
                            "update_result_set_from_rows: serde_json::to_string failed for \
                             dedup key; falling back to Debug representation. \
                             See nexus_executor_serde_fallback_total{{site=\"helper_row_dedup_key\"}}."
                        );
                        format!("{:?}", row_map)
                    }
                };
                !seen_row_keys.insert(row_key)
            };

            // Only add row if it's not a duplicate
            if !is_duplicate {
                unique_rows.push(row_map.clone());
            }
        }

        tracing::debug!(
            "update_result_set_from_rows: deduplicated {} rows to {} unique rows",
            rows.len(),
            unique_rows.len()
        );

        // DEBUG: Log details of each row for debugging
        for (idx, row_map) in rows.iter().enumerate() {
            let mut all_entity_ids: Vec<u64> = Vec::new();
            for value in row_map.values() {
                if let Value::Object(obj) = value {
                    if let Some(Value::Number(id)) = obj.get("_nexus_id") {
                        if let Some(entity_id) = id.as_u64() {
                            all_entity_ids.push(entity_id);
                        }
                    }
                }
            }
            all_entity_ids.sort();
        }

        // CRITICAL FIX: Always clear result_set.rows before updating to ensure complete replacement
        // This prevents mixing old rows with new ones
        context.result_set.rows.clear();
        context.result_set.columns = columns.clone();
        context.result_set.rows = unique_rows
            .iter()
            .map(|row_map| Row {
                values: columns
                    .iter()
                    .map(|column| row_map.get(column).cloned().unwrap_or(Value::Null))
                    .collect(),
            })
            .collect();
    }

    /// Check if an expression can be evaluated without variables (only literals and operations)
    pub(in crate::executor) fn can_evaluate_without_variables(
        &self,
        expr: &parser::Expression,
    ) -> bool {
        match expr {
            parser::Expression::Literal(_) => true,
            parser::Expression::Parameter(_) => true, // Parameters can be evaluated
            parser::Expression::Variable(_) => false, // Variables need context
            parser::Expression::PropertyAccess { .. } => false, // Property access needs variables
            parser::Expression::ArrayIndex { base, index } => {
                // Can evaluate if both base and index can be evaluated without variables
                self.can_evaluate_without_variables(base)
                    && self.can_evaluate_without_variables(index)
            }
            parser::Expression::ArraySlice { base, start, end } => {
                // Can evaluate if base and both indices can be evaluated without variables
                self.can_evaluate_without_variables(base)
                    && start
                        .as_ref()
                        .map(|s| self.can_evaluate_without_variables(s))
                        .unwrap_or(true)
                    && end
                        .as_ref()
                        .map(|e| self.can_evaluate_without_variables(e))
                        .unwrap_or(true)
            }
            parser::Expression::BinaryOp { left, right, .. } => {
                // Can evaluate if both operands can be evaluated
                self.can_evaluate_without_variables(left)
                    && self.can_evaluate_without_variables(right)
            }
            parser::Expression::UnaryOp { operand, .. } => {
                // Can evaluate if operand can be evaluated
                self.can_evaluate_without_variables(operand)
            }
            parser::Expression::FunctionCall { args, .. } => {
                // Can evaluate if all arguments can be evaluated
                args.iter()
                    .all(|arg| self.can_evaluate_without_variables(arg))
            }
            parser::Expression::Case {
                input,
                when_clauses,
                else_clause,
            } => {
                // Can evaluate if input (if present) and all when/else expressions can be evaluated
                let input_ok = input
                    .as_ref()
                    .map(|e| self.can_evaluate_without_variables(e))
                    .unwrap_or(true);
                let when_ok = when_clauses.iter().all(|when| {
                    self.can_evaluate_without_variables(&when.condition)
                        && self.can_evaluate_without_variables(&when.result)
                });
                let else_ok = else_clause
                    .as_ref()
                    .map(|e| self.can_evaluate_without_variables(e))
                    .unwrap_or(true);
                input_ok && when_ok && else_ok
            }
            parser::Expression::IsNull { expr, .. } => self.can_evaluate_without_variables(expr),
            parser::Expression::List(exprs) => {
                exprs.iter().all(|e| self.can_evaluate_without_variables(e))
            }
            parser::Expression::Map(map) => {
                map.values().all(|e| self.can_evaluate_without_variables(e))
            }
            parser::Expression::Exists { .. } => false, // EXISTS needs graph context
            parser::Expression::CollectSubquery { .. } => {
                // COLLECT { … } evaluates the inner subquery against
                // the storage layer; when the *outer* row is empty the
                // synthetic-row gate is the only thing standing
                // between us and "RETURN COLLECT { … } AS x" silently
                // emitting zero rows. The inner may reference outer
                // variables, but in that case a preceding clause has
                // already populated rows and this gate is irrelevant.
                true
            }
            parser::Expression::PatternComprehension { .. } => false, // Pattern needs graph context
            parser::Expression::MapProjection { .. } => false, // Map projection needs variables
            parser::Expression::ListComprehension {
                list_expression, ..
            } => {
                // List comprehension can be evaluated if the list expression can be evaluated.
                // The where_clause and transform_expression may reference the comprehension variable,
                // which is fine - it will be bound during comprehension execution.
                self.can_evaluate_without_variables(list_expression)
            }
        }
    }

    /// Evaluate `EXISTS { pattern [WHERE expr] }` (and the bare
    /// pattern-predicate form the parser lowers to the same AST node)
    /// against live graph state.
    ///
    /// The pattern is walked from `row`'s already-bound variables via
    /// the authoritative store adjacency (`find_relationships`, never
    /// `relationship_index()` — that index is non-authoritative for
    /// correctness). The anchor of each connected component (the
    /// pattern's first element, and every node that starts a fresh
    /// comma-separated pattern part) is resolved from the outer
    /// binding when its variable is already in scope (correlated
    /// subquery semantics); otherwise it is enumerated from the store.
    /// Every relationship hop binds its target node fresh per
    /// candidate, checking type(s), direction, and the target node's
    /// label/property constraints, and observes Cypher's relationship-
    /// isomorphism rule: no single witness binding reuses the same
    /// relationship id in two different hops. `EXISTS` is true iff at
    /// least one complete binding satisfies both the pattern's
    /// structural constraints and the optional inner `WHERE`, which is
    /// evaluated per candidate binding — a candidate that fails
    /// `WHERE` does not abort the probe, the walk simply continues to
    /// the next candidate. The search is depth-first and short-
    /// circuits on the first witness.
    ///
    /// Returns `Value::Null` (never plain `Value::Bool(false)`) when a
    /// correlated outer variable the pattern depends on is bound to
    /// `NULL` along every path tried, and no path produced a witness:
    /// three-valued logic — a pattern predicate over unknown input is
    /// itself unknown, not false. `Value::Null` composes correctly
    /// under `NOT` and is filtered out by `WHERE` exactly like any
    /// other `NULL` boolean expression.
    pub(in crate::executor) fn evaluate_exists_pattern(
        &self,
        row: &HashMap<String, Value>,
        context: &ExecutionContext,
        pattern: &parser::Pattern,
        where_clause: Option<&parser::Expression>,
    ) -> Result<Value> {
        if pattern.elements.is_empty() {
            // Defensive-only: the parser never produces an empty
            // pattern.
            return Ok(Value::Bool(false));
        }
        let mut bound_relationships: HashSet<u64> = HashSet::new();
        let outcome = self.exists_probe(
            context,
            &pattern.elements,
            0,
            row.clone(),
            None,
            &mut bound_relationships,
            where_clause,
        )?;
        Ok(outcome.into_value())
    }

    /// Resolve the candidate node id(s) that anchor a fresh component
    /// of an `EXISTS` pattern (the very first element, or a node that
    /// starts a new comma-separated pattern part).
    fn exists_resolve_anchor(
        &self,
        binding: &HashMap<String, Value>,
        context: &ExecutionContext,
        node: &parser::NodePattern,
    ) -> Result<ExistsAnchor> {
        if let Some(var) = &node.variable {
            if let Some(value) = binding.get(var) {
                return Ok(Self::exists_anchor_from_single_value(value));
            }
            if let Some(value) = context.get_variable(var) {
                // `update_variables_from_rows` stores every variable as
                // `Value::Array(values)` — one entry per materialised
                // row — not a single scalar. Expand it into the full
                // candidate id list instead of feeding the array
                // itself to `extract_entity_id` (which only recognises
                // a single node/relationship object and would silently
                // resolve to zero candidates).
                return Ok(match value {
                    Value::Null => ExistsAnchor::Null,
                    Value::Array(values) => ExistsAnchor::Ids(
                        values.iter().filter_map(Self::extract_entity_id).collect(),
                    ),
                    other => Self::exists_anchor_from_single_value(other),
                });
            }
        }
        // Fresh variable (or anonymous node): enumerate candidate ids
        // directly from the store/label index — never through
        // `execute_node_by_label` / `execute_all_nodes_scan`, which
        // materialise a full JSON `Value` per node and hard-error via
        // `Error::OutOfMemory` above `MAX_INTERMEDIATE_ROWS`. The
        // remaining labels/properties are re-checked per candidate in
        // `exists_accept_node_candidate`.
        let ids = self.exists_enumerate_candidate_ids(node.labels.first().map(String::as_str))?;
        Ok(ExistsAnchor::Ids(ids))
    }

    /// Resolve a single already-bound row value to an anchor: `NULL`
    /// propagates as `ExistsAnchor::Null`; a node/relationship object
    /// resolves to its id; anything else (bound to a non-entity value)
    /// yields zero candidates.
    fn exists_anchor_from_single_value(value: &Value) -> ExistsAnchor {
        if value.is_null() {
            ExistsAnchor::Null
        } else {
            match Self::extract_entity_id(value) {
                Some(id) => ExistsAnchor::Ids(vec![id]),
                None => ExistsAnchor::Ids(Vec::new()),
            }
        }
    }

    /// Enumerate live node ids for a fresh `EXISTS` pattern anchor:
    /// the label bitmap when `label` is declared, otherwise every live
    /// node id in the store. Returns raw ids only — no per-node JSON
    /// materialisation — so a large unlabeled anchor cannot blow the
    /// `MAX_INTERMEDIATE_ROWS` ceiling the way `execute_all_nodes_scan`
    /// would.
    fn exists_enumerate_candidate_ids(&self, label: Option<&str>) -> Result<Vec<u64>> {
        if let Some(label) = label {
            return match self.catalog().get_label_id(label) {
                Ok(label_id) => {
                    let bitmap = self.label_index().get_nodes(label_id)?;
                    Ok(bitmap.iter().map(u64::from).collect())
                }
                Err(_) => Ok(Vec::new()), // label never assigned to any node
            };
        }
        let store = self.store();
        let total_nodes = store.node_count();
        let mut ids = Vec::new();
        for node_id in 0..total_nodes {
            if let Ok(record) = store.read_node(node_id) {
                if !record.is_deleted() {
                    ids.push(node_id);
                }
            }
        }
        Ok(ids)
    }

    /// Verify `candidate_id` satisfies a pattern node's label/property
    /// constraints and, when the node's variable is already present in
    /// `binding` (correlated re-use — e.g. a closed triangle
    /// `(a)-->(b)-->(a)`, or a hop target that reuses an outer
    /// variable name), that the existing binding agrees with
    /// `candidate_id`.
    ///
    /// Reads the candidate exactly once: a single `store.read_node`
    /// backs the liveness check, the label check, and (when accepted)
    /// the property load reused for both the inline property-map match
    /// and the binding's materialised node value.
    fn exists_accept_node_candidate(
        &self,
        binding: &HashMap<String, Value>,
        context: &ExecutionContext,
        node: &parser::NodePattern,
        candidate_id: u64,
    ) -> Result<ExistsAcceptOutcome> {
        let store = self.store();
        let record = match store.read_node(candidate_id) {
            Ok(r) => r,
            Err(_) => return Ok(ExistsAcceptOutcome::Rejected),
        };
        if record.is_deleted() {
            return Ok(ExistsAcceptOutcome::Rejected);
        }

        let label_names = self.catalog().get_labels_from_bitmap(record.label_bits)?;
        if !node.labels.is_empty() {
            for required in &node.labels {
                if !label_names.iter().any(|l| l == required) {
                    return Ok(ExistsAcceptOutcome::Rejected);
                }
            }
        }

        if let Some(var) = &node.variable {
            if let Some(existing) = binding.get(var) {
                if existing.is_null() {
                    return Ok(ExistsAcceptOutcome::Null);
                }
                match Self::extract_entity_id(existing) {
                    Some(existing_id) if existing_id == candidate_id => {}
                    _ => return Ok(ExistsAcceptOutcome::Rejected),
                }
            }
        }

        let properties_value = store
            .load_node_properties_with_ptr(candidate_id, record.prop_ptr)?
            .unwrap_or_else(|| Value::Object(Map::new()));
        let mut node_map = match properties_value {
            Value::Object(map) => map,
            other => {
                let mut map = Map::new();
                map.insert("value".to_string(), other);
                map
            }
        };
        node_map.insert("_nexus_id".to_string(), Value::Number(candidate_id.into()));
        node_map.insert(
            "_nexus_labels".to_string(),
            Value::Array(label_names.into_iter().map(Value::String).collect()),
        );
        let node_value = Value::Object(node_map);
        drop(store);

        if !self.exists_node_properties_match(
            binding,
            context,
            node.properties.as_ref(),
            &node_value,
        )? {
            return Ok(ExistsAcceptOutcome::Rejected);
        }

        let mut next = binding.clone();
        if let Some(var) = &node.variable {
            next.entry(var.clone()).or_insert(node_value);
        }
        Ok(ExistsAcceptOutcome::Accepted(next))
    }

    /// Evaluate a pattern node's inline property map (`{prop: expr}`)
    /// against an already-materialised candidate node value. Each
    /// expected value is evaluated with the full projection evaluator
    /// (not just literals), so a property constraint may reference
    /// outer-row/correlated variables, e.g.
    /// `EXISTS { (a)-->(b {id: a.id}) }`. A `NULL` on either side never
    /// matches (mirrors Cypher's `NULL = x` → `NULL` under WHERE
    /// truthiness).
    fn exists_node_properties_match(
        &self,
        binding: &HashMap<String, Value>,
        context: &ExecutionContext,
        properties: Option<&parser::PropertyMap>,
        node_value: &Value,
    ) -> Result<bool> {
        let Some(props) = properties else {
            return Ok(true);
        };
        if props.properties.is_empty() {
            return Ok(true);
        }
        for (key, expected_expr) in &props.properties {
            let expected = self.evaluate_projection_expression(binding, context, expected_expr)?;
            let actual = Self::extract_property(node_value, key);
            if actual.is_null()
                || expected.is_null()
                || !self.values_equal_for_comparison(&actual, &expected)
            {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Evaluate a pattern relationship's inline property map against a
    /// candidate relationship. Mirrors `exists_node_properties_match`.
    fn exists_relationship_properties_match(
        &self,
        binding: &HashMap<String, Value>,
        context: &ExecutionContext,
        properties: Option<&parser::PropertyMap>,
        rel_info: &RelationshipInfo,
    ) -> Result<bool> {
        let Some(props) = properties else {
            return Ok(true);
        };
        if props.properties.is_empty() {
            return Ok(true);
        }
        let rel_value = self.read_relationship_as_value(rel_info)?;
        for (key, expected_expr) in &props.properties {
            let expected = self.evaluate_projection_expression(binding, context, expected_expr)?;
            let actual = Self::extract_property(&rel_value, key);
            if actual.is_null()
                || expected.is_null()
                || !self.values_equal_for_comparison(&actual, &expected)
            {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Depth-first walk of an `EXISTS` pattern's element list, starting
    /// at `elements[pos]`.
    ///
    /// A `Node` reached here always starts a fresh component: by
    /// construction of `parse_pattern_until_where_or_brace`, the only
    /// element positions that dispatch to this arm are `pos == 0` and
    /// a `Node` immediately following another `Node` — the flat
    /// encoding of a comma-separated independent pattern part, e.g.
    /// `EXISTS { (a)-->(b), (c)-->(d) }`. Every `Node` that is the
    /// target of a relationship hop is consumed inline by the
    /// `Relationship` arm below via `elements.get(pos + 1)`, so it
    /// never reaches this dispatch.
    ///
    /// `anchor_id` is the id of the most recently bound node, used by
    /// the `Relationship` arm to expand from; it is `None` exactly
    /// when `pos` lands on a fresh-component `Node` (the arm resolves
    /// its own anchor and ignores the parameter). `bound_relationships`
    /// carries the set of relationship ids already consumed by earlier
    /// hops in the CURRENT witness candidate (across every component,
    /// not just the current one) — Cypher's relationship-isomorphism
    /// rule: a single pattern match never traverses the same edge
    /// twice. Entries are inserted before recursing into a hop and
    /// removed again on backtrack, so sibling candidates at the same
    /// or an earlier position see the set as it was before that hop
    /// was tried.
    ///
    /// Returns `ExistsOutcome::True` on the first witness found
    /// (short-circuiting); otherwise `ExistsOutcome::Null` if any
    /// explored path hit a correlated `NULL` and no witness was found,
    /// else `ExistsOutcome::False`.
    fn exists_probe(
        &self,
        context: &ExecutionContext,
        elements: &[parser::PatternElement],
        pos: usize,
        binding: HashMap<String, Value>,
        anchor_id: Option<u64>,
        bound_relationships: &mut HashSet<u64>,
        where_clause: Option<&parser::Expression>,
    ) -> Result<ExistsOutcome> {
        let Some(element) = elements.get(pos) else {
            // Pattern fully walked — the accumulated binding is a
            // complete match. It is a witness iff it also satisfies
            // the inner WHERE (vacuously true when there is none).
            // The inner WHERE is a subquery filter: a NULL result
            // excludes the candidate row exactly like false does, so
            // it yields False here — unlike a NULL correlated pattern
            // variable, which makes the whole predicate Null.
            return match where_clause {
                Some(expr) => Ok(
                    if self.evaluate_predicate_on_row(&binding, context, expr)? {
                        ExistsOutcome::True
                    } else {
                        ExistsOutcome::False
                    },
                ),
                None => Ok(ExistsOutcome::True),
            };
        };

        match element {
            parser::PatternElement::Node(node) => {
                let candidates = match self.exists_resolve_anchor(&binding, context, node)? {
                    ExistsAnchor::Null => return Ok(ExistsOutcome::Null),
                    ExistsAnchor::Ids(ids) => ids,
                };
                let mut saw_null = false;
                for candidate_id in candidates {
                    match self.exists_accept_node_candidate(
                        &binding,
                        context,
                        node,
                        candidate_id,
                    )? {
                        ExistsAcceptOutcome::Rejected => {}
                        ExistsAcceptOutcome::Null => saw_null = true,
                        ExistsAcceptOutcome::Accepted(next_binding) => {
                            match self.exists_probe(
                                context,
                                elements,
                                pos + 1,
                                next_binding,
                                Some(candidate_id),
                                bound_relationships,
                                where_clause,
                            )? {
                                ExistsOutcome::True => return Ok(ExistsOutcome::True),
                                ExistsOutcome::Null => saw_null = true,
                                ExistsOutcome::False => {}
                            }
                        }
                    }
                }
                Ok(if saw_null {
                    ExistsOutcome::Null
                } else {
                    ExistsOutcome::False
                })
            }
            parser::PatternElement::Relationship(rel) => {
                let Some(anchor_id) = anchor_id else {
                    // A Relationship can only follow a Node in a
                    // well-formed pattern; defensive-only.
                    return Ok(ExistsOutcome::False);
                };
                let Some(parser::PatternElement::Node(next_node)) = elements.get(pos + 1) else {
                    // The grammar always pairs a relationship with a
                    // following node; defensive-only.
                    return Ok(ExistsOutcome::False);
                };

                // Resolve each declared type via a plain (read-only)
                // catalog lookup — NEVER `get_or_create_type` here:
                // this is a read path (WHERE-clause evaluation), and
                // interning a brand-new type id into the LMDB catalog
                // as a side effect of probing a pattern would corrupt
                // catalog state for a query that creates nothing. A
                // type name that has never been assigned to any
                // relationship simply contributes no id; if NONE of
                // the declared names resolve, the hop cannot match
                // anything (an empty `type_ids` list would instead be
                // reinterpreted by `find_relationships` as "match
                // every type", a false positive), so the probe fails
                // this hop without touching the catalog.
                let mut type_ids: Vec<u32> = Vec::with_capacity(rel.types.len());
                for type_name in &rel.types {
                    if let Some(id) = self.catalog().get_type_id(type_name)? {
                        type_ids.push(id);
                    }
                }
                let direction = match rel.direction {
                    parser::RelationshipDirection::Outgoing => Direction::Outgoing,
                    parser::RelationshipDirection::Incoming => Direction::Incoming,
                    parser::RelationshipDirection::Both => Direction::Both,
                };

                if let Some(quantifier) = &rel.quantifier {
                    // A named relationship variable on a variable-length
                    // hop binds a LIST<RELATIONSHIP> in full Cypher; the
                    // probe only ever materialises a single relationship
                    // value per hop variable. Reject clearly rather than
                    // bind something wrong.
                    if rel.variable.is_some() {
                        return Err(Error::CypherExecution(
                            "ERR_VAR_LENGTH_REL_VARIABLE_NOT_IMPLEMENTED: a named \
                             relationship variable on a variable-length relationship \
                             inside EXISTS is not supported (it binds a \
                             LIST<RELATIONSHIP> in full Cypher); use an anonymous \
                             variable-length relationship instead"
                                .to_string(),
                        ));
                    }
                    let (min_hops, max_hops) = match quantifier {
                        // openCypher defines a bare `*` as `*1..` (one
                        // or more), NOT `*0..` — deliberately diverging
                        // here from `execute_variable_length_path`
                        // (path.rs:489), whose `ZeroOrMore => (0,
                        // usize::MAX)` is a pre-existing MATCH-side
                        // off-by-one left untouched (out of scope for
                        // this probe). Getting this right at THIS call
                        // site matters more than bug-for-bug parity:
                        // without it, `EXISTS { (a)-[:T*]->() }` would
                        // be unconditionally true for every node (the
                        // zero-length case accepts the anchor itself
                        // against an unconstrained target). Explicit
                        // zero-length ranges (`*0..1`, `*0..`) are
                        // untouched — they already carry their own
                        // literal `0` lower bound via `Range`/parsed
                        // quantifiers, not this arm.
                        parser::RelationshipQuantifier::ZeroOrMore => (1, usize::MAX),
                        parser::RelationshipQuantifier::OneOrMore => (1, usize::MAX),
                        parser::RelationshipQuantifier::ZeroOrOne => (0, 1),
                        parser::RelationshipQuantifier::Exact(n) => (*n, *n),
                        parser::RelationshipQuantifier::Range(min, max) => (*min, *max),
                    };
                    // Mirrors `execute_variable_length_path`'s own
                    // clamp (path.rs) — an unbounded `*`/`+` quantifier
                    // faces the identical exponential-trail-count
                    // hazard that constant exists to cap; without it a
                    // no-witness probe over a dense, cyclic graph must
                    // exhaust every isomorphic trail before returning
                    // `False`, and an uncapped depth could also make
                    // `EXISTS` witness a path longer than `MATCH`'s own
                    // variable-length operator would ever allow.
                    let max_hops = max_hops.min(MAX_VAR_LENGTH_PATH_DEPTH);
                    // A declared type list that resolved to zero ids
                    // means the type has never been assigned to any
                    // relationship: no hop of length >= 1 can ever be
                    // taken, but a zero-length match (`min_hops == 0`)
                    // is still evaluated on its own merits below.
                    let can_extend = rel.types.is_empty() || !type_ids.is_empty();
                    let walk = ExistsVarLengthWalk {
                        context,
                        elements,
                        pos,
                        rel,
                        next_node,
                        min_hops,
                        max_hops,
                        can_extend,
                        type_ids: &type_ids,
                        direction,
                        where_clause,
                    };
                    return self.exists_probe_var_length(
                        &walk,
                        binding,
                        anchor_id,
                        0,
                        bound_relationships,
                    );
                }

                if !rel.types.is_empty() && type_ids.is_empty() {
                    return Ok(ExistsOutcome::False);
                }

                // Authoritative store adjacency — never
                // `relationship_index()`, which is not authoritative
                // for correctness.
                let relationships = self.find_relationships(
                    anchor_id, &type_ids, direction, None, // No cache for EXISTS probes
                )?;

                let mut saw_null = false;
                for rel_info in &relationships {
                    // Cypher relationship-isomorphism: a witness
                    // binding never traverses the same edge twice
                    // (e.g. a self-loop can't satisfy two consecutive
                    // hops, and an undirected `--` re-scan of the same
                    // edge from the other side doesn't count as a
                    // second hop).
                    if bound_relationships.contains(&rel_info.id) {
                        continue;
                    }
                    if !self.exists_relationship_properties_match(
                        &binding,
                        context,
                        rel.properties.as_ref(),
                        rel_info,
                    )? {
                        continue;
                    }

                    let mut hop_binding = binding.clone();
                    if let Some(var) = &rel.variable {
                        if let Some(existing) = hop_binding.get(var) {
                            match Self::extract_entity_id(existing) {
                                Some(existing_id) if existing_id == rel_info.id => {}
                                _ => continue,
                            }
                        } else {
                            hop_binding
                                .insert(var.clone(), self.read_relationship_as_value(rel_info)?);
                        }
                    }

                    let target_id = match direction {
                        Direction::Outgoing => rel_info.target_id,
                        Direction::Incoming => rel_info.source_id,
                        Direction::Both => {
                            if rel_info.source_id == anchor_id {
                                rel_info.target_id
                            } else {
                                rel_info.source_id
                            }
                        }
                    };

                    match self.exists_accept_node_candidate(
                        &hop_binding,
                        context,
                        next_node,
                        target_id,
                    )? {
                        ExistsAcceptOutcome::Rejected => {}
                        ExistsAcceptOutcome::Null => saw_null = true,
                        ExistsAcceptOutcome::Accepted(next_binding) => {
                            bound_relationships.insert(rel_info.id);
                            let outcome = self.exists_probe(
                                context,
                                elements,
                                pos + 2,
                                next_binding,
                                Some(target_id),
                                bound_relationships,
                                where_clause,
                            )?;
                            bound_relationships.remove(&rel_info.id);
                            match outcome {
                                ExistsOutcome::True => return Ok(ExistsOutcome::True),
                                ExistsOutcome::Null => saw_null = true,
                                ExistsOutcome::False => {}
                            }
                        }
                    }
                }
                Ok(if saw_null {
                    ExistsOutcome::Null
                } else {
                    ExistsOutcome::False
                })
            }
            parser::PatternElement::QuantifiedGroup(_) => Err(Error::CypherExecution(
                "ERR_QPP_NOT_IMPLEMENTED: quantified path patterns inside EXISTS subqueries \
                 need the QPP operator"
                    .to_string(),
            )),
        }
    }

    /// Depth-first walk of a variable-length relationship hop
    /// (`-[:T*min..max]->`) inside an `EXISTS` pattern.
    ///
    /// `current_id` is the frontier node reached after `depth` hops
    /// from the segment's starting anchor. At every depth within
    /// `[walk.min_hops, walk.max_hops]` the walk first tries accepting
    /// `current_id` itself against `walk.next_node`'s constraints —
    /// the zero-length case (`depth == min_hops == 0`) binds the
    /// pattern's target straight to the segment's anchor, consuming no
    /// edge — then, if `depth < walk.max_hops`, extends by one more
    /// hop. An unbounded `max_hops` (`usize::MAX` for bare `*` / `+`,
    /// clamped to `MAX_VAR_LENGTH_PATH_DEPTH` by the caller — see
    /// [`Self::exists_probe`]) needs no further artificial cap here:
    /// `bound_relationships` below forbids re-entering an
    /// already-consumed edge, so no DFS branch can exceed the graph's
    /// distinct live-edge count within that ceiling.
    ///
    /// `walk.can_extend` is `false` when the declared relationship
    /// type(s) never resolved to a real catalog type id — no hop of
    /// length >= 1 is possible in that case, but the zero-length case
    /// at `depth == 0` is still tried when `min_hops == 0` (an
    /// unmatched type still allows `*0..n` to degrade to "target is
    /// the anchor itself").
    ///
    /// Isomorphism (`bound_relationships`) is enforced exactly as for
    /// a fixed-length hop: an edge id is inserted before recursing
    /// into the extension that consumes it and removed again on
    /// backtrack, so it becomes unavailable to every later hop in the
    /// CURRENT witness candidate — including hops belonging to a
    /// different pattern component — without leaking across sibling
    /// candidates.
    fn exists_probe_var_length(
        &self,
        walk: &ExistsVarLengthWalk<'_>,
        binding: HashMap<String, Value>,
        current_id: u64,
        depth: usize,
        bound_relationships: &mut HashSet<u64>,
    ) -> Result<ExistsOutcome> {
        let mut saw_null = false;

        if depth >= walk.min_hops {
            match self.exists_accept_node_candidate(
                &binding,
                walk.context,
                walk.next_node,
                current_id,
            )? {
                ExistsAcceptOutcome::Rejected => {}
                ExistsAcceptOutcome::Null => saw_null = true,
                ExistsAcceptOutcome::Accepted(next_binding) => {
                    match self.exists_probe(
                        walk.context,
                        walk.elements,
                        walk.pos + 2,
                        next_binding,
                        Some(current_id),
                        bound_relationships,
                        walk.where_clause,
                    )? {
                        ExistsOutcome::True => return Ok(ExistsOutcome::True),
                        ExistsOutcome::Null => saw_null = true,
                        ExistsOutcome::False => {}
                    }
                }
            }
        }

        if walk.can_extend && depth < walk.max_hops {
            // Authoritative store adjacency — never `relationship_index()`.
            let relationships =
                self.find_relationships(current_id, walk.type_ids, walk.direction, None)?;
            for rel_info in &relationships {
                // Cypher relationship-isomorphism, enforced across the
                // whole var-length segment (and beyond it, into the
                // rest of the pattern): an edge already consumed by an
                // earlier hop in this witness candidate can't satisfy
                // a later one.
                if bound_relationships.contains(&rel_info.id) {
                    continue;
                }
                if !self.exists_relationship_properties_match(
                    &binding,
                    walk.context,
                    walk.rel.properties.as_ref(),
                    rel_info,
                )? {
                    continue;
                }

                let hop_target = match walk.direction {
                    Direction::Outgoing => rel_info.target_id,
                    Direction::Incoming => rel_info.source_id,
                    Direction::Both => {
                        if rel_info.source_id == current_id {
                            rel_info.target_id
                        } else {
                            rel_info.source_id
                        }
                    }
                };

                bound_relationships.insert(rel_info.id);
                let outcome = self.exists_probe_var_length(
                    walk,
                    binding.clone(),
                    hop_target,
                    depth + 1,
                    bound_relationships,
                )?;
                bound_relationships.remove(&rel_info.id);
                match outcome {
                    ExistsOutcome::True => return Ok(ExistsOutcome::True),
                    ExistsOutcome::Null => saw_null = true,
                    ExistsOutcome::False => {}
                }
            }
        }

        Ok(if saw_null {
            ExistsOutcome::Null
        } else {
            ExistsOutcome::False
        })
    }

    pub(in crate::executor) fn extract_property(entity: &Value, property: &str) -> Value {
        if let Value::Object(obj) = entity {
            // First check directly in the object (for nodes with flat properties)
            // This is the primary case - nodes have properties directly in the object
            if let Some(value) = obj.get(property) {
                // CRITICAL FIX: Allow _nexus_id to be returned when explicitly requested
                // Only skip truly internal properties that shouldn't be exposed
                if property == "_nexus_id" {
                    // _nexus_id is allowed and commonly used in queries
                    return value.clone();
                }
                // Skip other internal properties
                if property != "_nexus_type"
                    && property != "_source"
                    && property != "_target"
                    && property != "_element_id"
                {
                    return value.clone();
                }
            }
            // Then check if there's a nested "properties" object (for compatibility with other formats)
            if let Some(Value::Object(props)) = obj.get("properties") {
                if let Some(value) = props.get(property) {
                    return value.clone();
                }
            }
        }
        Value::Null
    }

    /// Check if value is a duration object (has years, months, days, hours, minutes, or seconds keys)

    pub(in crate::executor) fn update_variables_from_rows(
        &self,
        context: &mut ExecutionContext,
        rows: &[HashMap<String, Value>],
    ) {
        let mut arrays: HashMap<String, Vec<Value>> = HashMap::new();
        for row in rows {
            for (var, value) in row {
                arrays.entry(var.clone()).or_default().push(value.clone());
            }
        }

        context.variables.clear();

        for (var, values) in arrays {
            context.variables.insert(var, Value::Array(values));
        }
    }

    pub(in crate::executor) fn evaluate_predicate_on_row(
        &self,
        row: &HashMap<String, Value>,
        context: &ExecutionContext,
        expr: &parser::Expression,
    ) -> Result<bool> {
        let value = self.evaluate_projection_expression(row, context, expr)?;
        self.value_to_bool(&value)
    }

    pub(in crate::executor) fn extract_entity_id(value: &Value) -> Option<u64> {
        match value {
            Value::Object(obj) => {
                if let Some(id) = obj.get("_nexus_id").and_then(|id| id.as_u64()) {
                    Some(id)
                } else if let Some(id) = obj
                    .get("_element_id")
                    .and_then(|id| id.as_str())
                    .and_then(|s| s.parse::<u64>().ok())
                {
                    Some(id)
                } else if let Some(id_value) = obj.get("id") {
                    match id_value {
                        Value::Number(num) => num.as_u64(),
                        Value::String(s) => s.parse::<u64>().ok(),
                        _ => None,
                    }
                } else {
                    None
                }
            }
            Value::Number(num) => num.as_u64(),
            _ => None,
        }
    }

    /// Read a relationship as a JSON value.
    ///
    /// Acquires its own `store()` read guard. Bulk loops that already
    /// hold a guard (e.g. alongside repeated
    /// `read_node_as_value_with_store` calls) should use
    /// [`Self::read_relationship_as_value_with_store`] instead to avoid
    /// a second acquisition per element — see that method's doc comment.
    pub(in crate::executor) fn read_relationship_as_value(
        &self,
        rel: &RelationshipInfo,
    ) -> Result<Value> {
        let store = self.store();
        self.read_relationship_as_value_with_store(&store, rel)
    }

    /// Same as [`Self::read_relationship_as_value`], but for callers
    /// that already hold a `store()` read guard.
    ///
    /// phase8_neo4j-concurrency-gaps §2 — mirrors
    /// `Executor::read_node_as_value_with_store`: bulk loops that
    /// materialise both a node and its relationship per element (e.g.
    /// `Expand`'s target loop) would otherwise take a SECOND
    /// independent `self.store()` acquisition here on top of the one
    /// already held for the node read. Threading the held guard through
    /// avoids that, and — critically — avoids a same-thread recursive
    /// acquire of the non-reentrant `parking_lot::RwLock` while an outer
    /// guard from the same call chain is still alive (see the
    /// `parking-lot-rwlock-does-not-allow-recursive-acquire` anti-pattern
    /// entry).
    pub(in crate::executor) fn read_relationship_as_value_with_store(
        &self,
        store: &RecordStore,
        rel: &RelationshipInfo,
    ) -> Result<Value> {
        let type_name = self
            .catalog()
            .get_type_name(rel.type_id)?
            .unwrap_or_else(|| format!("type_{}", rel.type_id));

        let properties_value = store
            .load_relationship_properties(rel.id)?
            .unwrap_or_else(|| Value::Object(Map::new()));

        let properties_map = match properties_value {
            Value::Object(map) => map,
            other => {
                let mut map = Map::new();
                map.insert("value".to_string(), other);
                map
            }
        };

        // `_nexus_id` carries the internal id; `type` carries the relationship
        // type under the name the Neo4j-shaped flat format and `type(r)`
        // expect. Neither can be used to TELL a relationship from a node:
        // `type` is an ordinary property name a node may legitimately carry
        // (LDBC's `Organisation.type` / `Place.type` do), which is why
        // `_nexus_rel_type` exists — a reserved key that only this constructor
        // writes. See `is_relationship_value`.
        let mut rel_obj = properties_map;
        rel_obj.insert("_nexus_id".to_string(), Value::Number(rel.id.into()));
        rel_obj.insert(
            crate::executor::REL_TYPE_MARKER.to_string(),
            Value::String(type_name.clone()),
        );
        rel_obj.insert("type".to_string(), Value::String(type_name));

        // Return only the properties as a flat object, matching Neo4j's format
        Ok(Value::Object(rel_obj))
    }

    /// Phase 2.4.2: Optimize result_set_as_rows to reduce intermediate copies
    pub(in crate::executor) fn result_set_as_rows(
        &self,
        context: &ExecutionContext,
    ) -> Vec<HashMap<String, Value>> {
        // Pre-size the result vector to avoid reallocations
        let capacity = context.result_set.rows.len();
        let mut result = Vec::with_capacity(capacity);

        for row in &context.result_set.rows {
            // Pre-size HashMap based on column count
            let mut map = HashMap::with_capacity(context.result_set.columns.len());
            for (idx, column) in context.result_set.columns.iter().enumerate() {
                if idx < row.values.len() {
                    // Use reference when possible, only clone when necessary
                    map.insert(column.clone(), row.values[idx].clone());
                } else {
                    map.insert(column.clone(), Value::Null);
                }
            }
            result.push(map);
        }

        result
    }
}

#[cfg(test)]
mod tests {
    //! phase0_fix-cypher-oom-process-abort §4.2 — unit coverage for the
    //! byte budget check in [`Executor::apply_cartesian_product`]. The
    //! integration-level regression test (the §1.1 minimal repro shape
    //! surviving end-to-end instead of aborting the process) lives in
    //! `crates/nexus-core/tests/cypher_oom_guard_test.rs`; these tests
    //! pin the ceiling itself: it fires deterministically, is
    //! configurable via `ExecutorConfig::cartesian_product_max_bytes`,
    //! and does not reject legitimate small products under the default
    //! budget.

    use super::*;
    use crate::executor::Query;
    use crate::testing::create_test_executor;
    use serde_json::json;

    #[test]
    fn exists_true_when_matching_relationship_present() {
        let (mut executor, _ctx) = create_test_executor();

        let create = Query {
            cypher: "CREATE (a:ExistsProbeA {name: 'a'})-[:EXISTS_PROBE_REL]->\
                     (b:ExistsProbeA {name: 'b'})"
                .to_string(),
            params: HashMap::new(),
        };
        executor.execute(&create).expect("create should succeed");

        let query = Query {
            cypher: "MATCH (a:ExistsProbeA {name: 'a'}) \
                     WHERE EXISTS { (a)-[:EXISTS_PROBE_REL]->() } \
                     RETURN a.name AS name"
                .to_string(),
            params: HashMap::new(),
        };
        let result = executor.execute(&query).expect("match should succeed");

        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.rows[0].values[0], json!("a"));
    }

    #[test]
    fn exists_false_when_no_matching_relationship() {
        let (mut executor, _ctx) = create_test_executor();

        let create = Query {
            cypher: "CREATE (a:ExistsProbeB {name: 'a'})".to_string(),
            params: HashMap::new(),
        };
        executor.execute(&create).expect("create should succeed");

        let query = Query {
            cypher: "MATCH (a:ExistsProbeB {name: 'a'}) \
                     WHERE EXISTS { (a)-[:EXISTS_PROBE_REL_B]->() } \
                     RETURN a.name AS name"
                .to_string(),
            params: HashMap::new(),
        };
        let result = executor.execute(&query).expect("match should succeed");

        assert_eq!(result.rows.len(), 0);
    }

    #[test]
    fn not_exists_negates_the_pattern_probe() {
        let (mut executor, _ctx) = create_test_executor();

        let create = Query {
            cypher: "CREATE (a:ExistsProbeC {name: 'a'})-[:EXISTS_PROBE_REL_C]->\
                     (b:ExistsProbeC {name: 'b'}), (c:ExistsProbeC {name: 'c'})"
                .to_string(),
            params: HashMap::new(),
        };
        executor.execute(&create).expect("create should succeed");

        let query = Query {
            cypher: "MATCH (n:ExistsProbeC) \
                     WHERE NOT EXISTS { (n)-[:EXISTS_PROBE_REL_C]->() } \
                     RETURN n.name AS name ORDER BY name"
                .to_string(),
            params: HashMap::new(),
        };
        let result = executor.execute(&query).expect("match should succeed");

        let names: Vec<&str> = result
            .rows
            .iter()
            .map(|row| row.values[0].as_str().expect("name is a string"))
            .collect();
        assert_eq!(names, vec!["b", "c"]);
    }

    #[test]
    fn exists_probes_multi_hop_chains() {
        let (mut executor, _ctx) = create_test_executor();

        let create = Query {
            cypher: "CREATE (a:ExistsProbeD {name: 'a'})-[:STEP1]->\
                     (b:ExistsProbeD {name: 'b'})-[:STEP2]->(c:ExistsProbeD {name: 'c'}), \
                     (x:ExistsProbeD {name: 'x'})-[:STEP1]->(y:ExistsProbeD {name: 'y'})"
                .to_string(),
            params: HashMap::new(),
        };
        executor.execute(&create).expect("create should succeed");

        let query = Query {
            cypher: "MATCH (n:ExistsProbeD) \
                     WHERE EXISTS { (n)-[:STEP1]->()-[:STEP2]->() } \
                     RETURN n.name AS name"
                .to_string(),
            params: HashMap::new(),
        };
        let result = executor.execute(&query).expect("match should succeed");

        // `x` has a STEP1 hop but its target `y` has no outgoing STEP2,
        // so only the full two-hop chain from `a` is a witness.
        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.rows[0].values[0], json!("a"));
    }

    #[test]
    fn exists_inner_where_filters_candidate_bindings() {
        let (mut executor, _ctx) = create_test_executor();

        let create = Query {
            cypher: "CREATE (a:ExistsProbeE {name: 'a'})-[:KNOWS]->\
                     (b:ExistsProbeE {name: 'b', age: 10}), \
                     (a)-[:KNOWS]->(c:ExistsProbeE {name: 'c', age: 30})"
                .to_string(),
            params: HashMap::new(),
        };
        executor.execute(&create).expect("create should succeed");

        let matches = Query {
            cypher: "MATCH (a:ExistsProbeE {name: 'a'}) \
                     WHERE EXISTS { (a)-[:KNOWS]->(x) WHERE x.age > 20 } \
                     RETURN a.name AS name"
                .to_string(),
            params: HashMap::new(),
        };
        let result = executor.execute(&matches).expect("match should succeed");
        assert_eq!(result.rows.len(), 1, "one KNOWS target has age > 20");
        assert_eq!(result.rows[0].values[0], json!("a"));

        let no_matches = Query {
            cypher: "MATCH (a:ExistsProbeE {name: 'a'}) \
                     WHERE EXISTS { (a)-[:KNOWS]->(x) WHERE x.age > 100 } \
                     RETURN a.name AS name"
                .to_string(),
            params: HashMap::new(),
        };
        let result = executor.execute(&no_matches).expect("match should succeed");
        assert_eq!(
            result.rows.len(),
            0,
            "no KNOWS target satisfies the inner WHERE"
        );
    }

    #[test]
    fn exists_respects_the_correlated_outer_variable() {
        let (mut executor, _ctx) = create_test_executor();

        let create = Query {
            cypher: "CREATE (a:ExistsProbeF {name: 'a'})-[:REL_F]->\
                     (t:ExistsProbeF {name: 'target'}), (b:ExistsProbeF {name: 'b'})"
                .to_string(),
            params: HashMap::new(),
        };
        executor.execute(&create).expect("create should succeed");

        let query = Query {
            cypher: "MATCH (n:ExistsProbeF) \
                     WHERE EXISTS { (n)-[:REL_F]->() } \
                     RETURN n.name AS name"
                .to_string(),
            params: HashMap::new(),
        };
        let result = executor.execute(&query).expect("match should succeed");

        // Only `a` (bound to the outer `n`) has an outgoing REL_F; `b`
        // and `target` do not. This proves the probe is evaluated
        // against each row's own correlated binding, not a single
        // global existence check.
        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.rows[0].values[0], json!("a"));
    }

    #[test]
    fn exists_returns_null_when_correlated_variable_is_null() {
        let (mut executor, _ctx) = create_test_executor();

        let create = Query {
            cypher: "CREATE (a:ExistsProbeG {name: 'a'})".to_string(),
            params: HashMap::new(),
        };
        executor.execute(&create).expect("create should succeed");

        let query = Query {
            cypher: "MATCH (a:ExistsProbeG {name: 'a'}) \
                     OPTIONAL MATCH (a)-[:NEVER_CREATED]->(m:ExistsProbeG) \
                     RETURN EXISTS { (m)-[:ALSO_NEVER_CREATED]->() } AS result"
                .to_string(),
            params: HashMap::new(),
        };
        let result = executor.execute(&query).expect("match should succeed");

        // `m` is bound but NULL (the OPTIONAL MATCH found nothing), so
        // the pattern correlated to it cannot be probed: the predicate
        // is unknown (NULL), not false — and evaluation must not crash.
        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.rows[0].values[0], Value::Null);
    }

    #[test]
    fn exists_matches_comma_separated_pattern_parts() {
        let (mut executor, _ctx) = create_test_executor();

        let create = Query {
            cypher: "CREATE (a:ExistsProbeH {name: 'a'})-[:H1]->(b:ExistsProbeH {name: 'b'}), \
                     (c:ExistsProbeH {name: 'c'})-[:H2]->(d:ExistsProbeH {name: 'd'})"
                .to_string(),
            params: HashMap::new(),
        };
        executor.execute(&create).expect("create should succeed");

        // The first pattern part is correlated to the outer `n`; the
        // second is an independent, freshly-anchored comma-separated
        // part (label + inline property, no shared variable). Both
        // must be satisfied simultaneously.
        let matches = Query {
            cypher: "MATCH (n:ExistsProbeH {name: 'a'}) \
                     WHERE EXISTS { (n)-[:H1]->(), (:ExistsProbeH {name: 'c'})-[:H2]->() } \
                     RETURN n.name AS name"
                .to_string(),
            params: HashMap::new(),
        };
        let result = executor.execute(&matches).expect("match should succeed");
        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.rows[0].values[0], json!("a"));

        let no_match = Query {
            cypher: "MATCH (n:ExistsProbeH {name: 'a'}) \
                     WHERE EXISTS { \
                         (n)-[:H1]->(), \
                         (:ExistsProbeH {name: 'c'})-[:NEVER_CREATED_H2]->() \
                     } \
                     RETURN n.name AS name"
                .to_string(),
            params: HashMap::new(),
        };
        let result = executor.execute(&no_match).expect("match should succeed");
        assert_eq!(
            result.rows.len(),
            0,
            "second comma-separated part has no matching relationship"
        );
    }

    #[test]
    fn exists_probes_incoming_direction() {
        let (mut executor, _ctx) = create_test_executor();

        let create = Query {
            cypher: "CREATE (a:ExistsProbeI {name: 'a'})-[:INTO]->(b:ExistsProbeI {name: 'b'})"
                .to_string(),
            params: HashMap::new(),
        };
        executor.execute(&create).expect("create should succeed");

        let query = Query {
            cypher: "MATCH (b:ExistsProbeI {name: 'b'}) \
                     WHERE EXISTS { (b)<-[:INTO]-() } \
                     RETURN b.name AS name"
                .to_string(),
            params: HashMap::new(),
        };
        let result = executor.execute(&query).expect("match should succeed");
        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.rows[0].values[0], json!("b"));
    }

    #[test]
    fn exists_probes_both_direction_from_either_endpoint() {
        let (mut executor, _ctx) = create_test_executor();

        let create = Query {
            cypher: "CREATE (a:ExistsProbeJ {name: 'a'})-[:LINK]->(b:ExistsProbeJ {name: 'b'})"
                .to_string(),
            params: HashMap::new(),
        };
        executor.execute(&create).expect("create should succeed");

        // Undirected `--` must find the edge starting from EITHER
        // endpoint: `a` takes the `source_id == anchor_id` branch of
        // the Both target-selection logic, `b` takes the other.
        let query = Query {
            cypher: "MATCH (n:ExistsProbeJ) \
                     WHERE EXISTS { (n)-[:LINK]-() } \
                     RETURN n.name AS name ORDER BY name"
                .to_string(),
            params: HashMap::new(),
        };
        let result = executor.execute(&query).expect("match should succeed");

        let names: Vec<&str> = result
            .rows
            .iter()
            .map(|row| row.values[0].as_str().expect("name is a string"))
            .collect();
        assert_eq!(names, vec!["a", "b"]);
    }

    #[test]
    fn exists_enforces_relationship_isomorphism_on_a_single_edge() {
        let (mut executor, _ctx) = create_test_executor();

        // Exactly one edge in the whole graph: a two-hop undirected
        // probe from `a` must NOT be satisfiable by re-traversing that
        // same edge from the other side.
        let create = Query {
            cypher:
                "CREATE (a:ExistsProbeK {name: 'a'})-[:ONLY_EDGE]->(b:ExistsProbeK {name: 'b'})"
                    .to_string(),
            params: HashMap::new(),
        };
        executor.execute(&create).expect("create should succeed");

        let query = Query {
            cypher: "MATCH (n:ExistsProbeK {name: 'a'}) \
                     WHERE EXISTS { (n)--()--() } \
                     RETURN n.name AS name"
                .to_string(),
            params: HashMap::new(),
        };
        let result = executor.execute(&query).expect("match should succeed");
        assert_eq!(
            result.rows.len(),
            0,
            "the only edge cannot satisfy both hops of a two-hop probe"
        );
    }

    #[test]
    fn exists_enforces_relationship_isomorphism_on_a_self_loop() {
        let (mut executor, _ctx) = create_test_executor();

        let create = Query {
            cypher: "CREATE (a:ExistsProbeL {name: 'a'})".to_string(),
            params: HashMap::new(),
        };
        executor.execute(&create).expect("create should succeed");
        let create_loop = Query {
            cypher: "MATCH (a:ExistsProbeL {name: 'a'}) CREATE (a)-[:SELF_LOOP]->(a)".to_string(),
            params: HashMap::new(),
        };
        executor
            .execute(&create_loop)
            .expect("create should succeed");

        // The self-loop is the only :SELF_LOOP edge from `a`; a
        // two-hop probe must not be able to traverse it twice.
        let query = Query {
            cypher: "MATCH (n:ExistsProbeL {name: 'a'}) \
                     WHERE EXISTS { (n)-[:SELF_LOOP]->()-[:SELF_LOOP]->() } \
                     RETURN n.name AS name"
                .to_string(),
            params: HashMap::new(),
        };
        let result = executor.execute(&query).expect("match should succeed");
        assert_eq!(
            result.rows.len(),
            0,
            "the self-loop cannot satisfy both hops of a two-hop probe"
        );
    }

    #[test]
    fn exists_inline_node_property_map_filters_targets() {
        let (mut executor, _ctx) = create_test_executor();

        let create = Query {
            cypher: "CREATE (a:ExistsProbeM {name: 'a'})-[:KNOWS]->\
                     (b:ExistsProbeM {name: 'b', age: 10}), \
                     (a)-[:KNOWS]->(c:ExistsProbeM {name: 'c', age: 30})"
                .to_string(),
            params: HashMap::new(),
        };
        executor.execute(&create).expect("create should succeed");

        let matches = Query {
            cypher: "MATCH (a:ExistsProbeM {name: 'a'}) \
                     WHERE EXISTS { (a)-[:KNOWS]->({age: 30}) } \
                     RETURN a.name AS name"
                .to_string(),
            params: HashMap::new(),
        };
        let result = executor.execute(&matches).expect("match should succeed");
        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.rows[0].values[0], json!("a"));

        let no_match = Query {
            cypher: "MATCH (a:ExistsProbeM {name: 'a'}) \
                     WHERE EXISTS { (a)-[:KNOWS]->({age: 99}) } \
                     RETURN a.name AS name"
                .to_string(),
            params: HashMap::new(),
        };
        let result = executor.execute(&no_match).expect("match should succeed");
        assert_eq!(result.rows.len(), 0);
    }

    #[test]
    fn exists_correlated_inline_property_map_matches_outer_variable() {
        let (mut executor, _ctx) = create_test_executor();

        let create = Query {
            cypher: "CREATE (a:ExistsProbeN {name: 'a', id: 1})-[:SELF_REF]->\
                     (b:ExistsProbeN {name: 'b', id: 1}), \
                     (a)-[:SELF_REF]->(c:ExistsProbeN {name: 'c', id: 2})"
                .to_string(),
            params: HashMap::new(),
        };
        executor.execute(&create).expect("create should succeed");

        // The inline property expression `a.id` references the outer,
        // correlated `a` — only the target that shares `a`'s id
        // qualifies.
        let query = Query {
            cypher: "MATCH (a:ExistsProbeN {name: 'a'}) \
                     WHERE EXISTS { (a)-[:SELF_REF]->(x {id: a.id}) } \
                     RETURN a.name AS name"
                .to_string(),
            params: HashMap::new(),
        };
        let result = executor.execute(&query).expect("match should succeed");
        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.rows[0].values[0], json!("a"));
    }

    #[test]
    fn exists_null_property_expression_never_matches() {
        let (mut executor, _ctx) = create_test_executor();

        let create = Query {
            cypher: "CREATE (a:ExistsProbeO {name: 'a'})-[:REL_O]->\
                     (b:ExistsProbeO {name: 'b', tag: 'x'})"
                .to_string(),
            params: HashMap::new(),
        };
        executor.execute(&create).expect("create should succeed");

        // `a.missing_prop` evaluates to NULL (the property doesn't
        // exist on `a`); a NULL expected value must never match, even
        // though `b.tag` is a concrete non-null value.
        let query = Query {
            cypher: "MATCH (a:ExistsProbeO {name: 'a'}) \
                     WHERE EXISTS { (a)-[:REL_O]->({tag: a.missing_prop}) } \
                     RETURN a.name AS name"
                .to_string(),
            params: HashMap::new(),
        };
        let result = executor.execute(&query).expect("match should succeed");
        assert_eq!(result.rows.len(), 0);
    }

    #[test]
    fn exists_label_constraint_on_hop_target() {
        let (mut executor, _ctx) = create_test_executor();

        let create = Query {
            cypher: "CREATE (a:ExistsProbeP {name: 'a'})-[:REL_P]->(c:ExistsProbeP {name: 'c'})"
                .to_string(),
            params: HashMap::new(),
        };
        executor.execute(&create).expect("create should succeed");

        let matches = Query {
            cypher: "MATCH (a:ExistsProbeP {name: 'a'}) \
                     WHERE EXISTS { (a)-[:REL_P]->(:ExistsProbeP) } \
                     RETURN a.name AS name"
                .to_string(),
            params: HashMap::new(),
        };
        let result = executor.execute(&matches).expect("match should succeed");
        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.rows[0].values[0], json!("a"));

        let create_wrong_label = Query {
            cypher: "CREATE (a:ExistsProbeP2 {name: 'a'})-[:REL_P2]->(b:ExistsProbeQ2 {name: 'b'})"
                .to_string(),
            params: HashMap::new(),
        };
        executor
            .execute(&create_wrong_label)
            .expect("create should succeed");

        let no_match = Query {
            cypher: "MATCH (a:ExistsProbeP2 {name: 'a'}) \
                     WHERE EXISTS { (a)-[:REL_P2]->(:ExistsProbeP2) } \
                     RETURN a.name AS name"
                .to_string(),
            params: HashMap::new(),
        };
        let result = executor.execute(&no_match).expect("match should succeed");
        assert_eq!(
            result.rows.len(),
            0,
            "the only REL_P2 target carries the wrong label"
        );
    }

    #[test]
    fn exists_relationship_variable_reuse_requires_same_edge() {
        let (mut executor, _ctx) = create_test_executor();

        let create = Query {
            cypher: "CREATE (a:ExistsProbeR {name: 'a'})-[:REL_R {weight: 10}]->\
                     (b:ExistsProbeR {name: 'b'})"
                .to_string(),
            params: HashMap::new(),
        };
        executor.execute(&create).expect("create should succeed");

        // The relationship variable materialises into the binding and
        // is queryable from the inner WHERE.
        let binds = Query {
            cypher: "MATCH (a:ExistsProbeR {name: 'a'}) \
                     WHERE EXISTS { (a)-[r:REL_R]->() WHERE r.weight > 5 } \
                     RETURN a.name AS name"
                .to_string(),
            params: HashMap::new(),
        };
        let result = executor.execute(&binds).expect("match should succeed");
        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.rows[0].values[0], json!("a"));

        let create_two_edges = Query {
            cypher: "CREATE (a:ExistsProbeS {name: 'a'})-[:REL_S]->(b:ExistsProbeS {name: 'b'}), \
                     (a)-[:REL_S]->(c:ExistsProbeS {name: 'c'})"
                .to_string(),
            params: HashMap::new(),
        };
        executor
            .execute(&create_two_edges)
            .expect("create should succeed");

        // `a` has two distinct :REL_S edges (to `b` and to `c`).
        // Reusing the same relationship variable `r` across both
        // comma-separated parts demands the SAME edge id for both —
        // impossible here, since the edge to `b` and the edge to `c`
        // are different relationships.
        let reuse_mismatch = Query {
            cypher: "MATCH (a:ExistsProbeS {name: 'a'}) \
                     WHERE EXISTS { \
                         (a)-[r:REL_S]->(:ExistsProbeS {name: 'b'}), \
                         (a)-[r:REL_S]->(:ExistsProbeS {name: 'c'}) \
                     } \
                     RETURN a.name AS name"
                .to_string(),
            params: HashMap::new(),
        };
        let result = executor
            .execute(&reuse_mismatch)
            .expect("match should succeed");
        assert_eq!(
            result.rows.len(),
            0,
            "reusing `r` across parts requires literally the same edge"
        );
    }

    #[test]
    fn exists_triangle_correlation_closes_back_to_anchor() {
        let (mut executor, _ctx) = create_test_executor();

        let create = Query {
            cypher: "CREATE (a:ExistsProbeT {name: 'a'})-[:TRI]->(b:ExistsProbeT {name: 'b'})"
                .to_string(),
            params: HashMap::new(),
        };
        executor.execute(&create).expect("create should succeed");
        let close_triangle = Query {
            cypher: "MATCH (a:ExistsProbeT {name: 'a'}), (b:ExistsProbeT {name: 'b'}) \
                     CREATE (b)-[:TRI]->(a)"
                .to_string(),
            params: HashMap::new(),
        };
        executor
            .execute(&close_triangle)
            .expect("create should succeed");

        let closes = Query {
            cypher: "MATCH (a:ExistsProbeT {name: 'a'}) \
                     WHERE EXISTS { (a)-[:TRI]->()-[:TRI]->(a) } \
                     RETURN a.name AS name"
                .to_string(),
            params: HashMap::new(),
        };
        let result = executor.execute(&closes).expect("match should succeed");
        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.rows[0].values[0], json!("a"));

        let create_open_chain = Query {
            cypher: "CREATE (x:ExistsProbeU {name: 'x'})-[:TRI]->\
                     (y:ExistsProbeU {name: 'y'})-[:TRI]->(z:ExistsProbeU {name: 'z'})"
                .to_string(),
            params: HashMap::new(),
        };
        executor
            .execute(&create_open_chain)
            .expect("create should succeed");

        let does_not_close = Query {
            cypher: "MATCH (x:ExistsProbeU {name: 'x'}) \
                     WHERE EXISTS { (x)-[:TRI]->()-[:TRI]->(x) } \
                     RETURN x.name AS name"
                .to_string(),
            params: HashMap::new(),
        };
        let result = executor
            .execute(&does_not_close)
            .expect("match should succeed");
        assert_eq!(
            result.rows.len(),
            0,
            "the open chain never closes back to x"
        );
    }

    #[test]
    fn exists_inner_where_references_outer_variable() {
        let (mut executor, _ctx) = create_test_executor();

        let create = Query {
            cypher: "CREATE (a:ExistsProbeV {name: 'a', age: 20})-[:KNOWS]->\
                     (b:ExistsProbeV {name: 'b', age: 25}), \
                     (a)-[:KNOWS]->(c:ExistsProbeV {name: 'c', age: 15})"
                .to_string(),
            params: HashMap::new(),
        };
        executor.execute(&create).expect("create should succeed");

        let query = Query {
            cypher: "MATCH (a:ExistsProbeV {name: 'a'}) \
                     WHERE EXISTS { (a)-[:KNOWS]->(x) WHERE x.age > a.age } \
                     RETURN a.name AS name"
                .to_string(),
            params: HashMap::new(),
        };
        let result = executor.execute(&query).expect("match should succeed");
        assert_eq!(result.rows.len(), 1, "b.age (25) exceeds a.age (20)");
        assert_eq!(result.rows[0].values[0], json!("a"));
    }

    #[test]
    fn exists_null_inner_where_filters_like_false() {
        let (mut executor, _ctx) = create_test_executor();

        let create = Query {
            cypher: "CREATE (a:ExistsProbeW {name: 'a'})-[:OWNS_W]->(c:ExistsProbeW {name: 'c'})"
                .to_string(),
            params: HashMap::new(),
        };
        executor.execute(&create).expect("create should succeed");

        // `x.missing > 1` is NULL for every candidate (the property
        // doesn't exist). A NULL inner WHERE excludes the candidate
        // exactly like false, so EXISTS is false — and NOT EXISTS
        // must therefore return the row, not swallow it as NULL.
        let query = Query {
            cypher: "MATCH (a:ExistsProbeW {name: 'a'}) \
                     WHERE NOT EXISTS { (a)-[:OWNS_W]->(x) WHERE x.missing > 1 } \
                     RETURN a.name AS name"
                .to_string(),
            params: HashMap::new(),
        };
        let result = executor.execute(&query).expect("match should succeed");
        assert_eq!(
            result.rows.len(),
            1,
            "NULL inner WHERE must make EXISTS false"
        );
        assert_eq!(result.rows[0].values[0], json!("a"));
    }

    #[test]
    fn exists_bare_star_var_length_finds_a_two_hop_chain() {
        let (mut executor, _ctx) = create_test_executor();

        let create = Query {
            cypher: "CREATE (a:ExistsProbeAD {name: 'a'})-[:AD1]->(b:ExistsProbeAD {name: 'b'})-\
                     [:AD1]->(c:ExistsProbeAD {name: 'c'})"
                .to_string(),
            params: HashMap::new(),
        };
        executor.execute(&create).expect("create should succeed");

        let matches = Query {
            cypher: "MATCH (a:ExistsProbeAD {name: 'a'}) \
                     WHERE EXISTS { (a)-[:AD1*]->(:ExistsProbeAD {name: 'c'}) } \
                     RETURN a.name AS name"
                .to_string(),
            params: HashMap::new(),
        };
        let result = executor.execute(&matches).expect("match should succeed");
        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.rows[0].values[0], json!("a"));

        let no_match = Query {
            cypher: "MATCH (a:ExistsProbeAD {name: 'a'}) \
                     WHERE EXISTS { (a)-[:AD1*]->(:ExistsProbeAD {name: 'never'}) } \
                     RETURN a.name AS name"
                .to_string(),
            params: HashMap::new(),
        };
        let result = executor.execute(&no_match).expect("match should succeed");
        assert_eq!(result.rows.len(), 0);
    }

    #[test]
    fn exists_bare_star_var_length_has_a_lower_bound_of_one() {
        let (mut executor, _ctx) = create_test_executor();

        // openCypher's bare `*` means `*1..` (one or more), not
        // `*0..`. An isolated node with no `:X` edges at all must NOT
        // satisfy `EXISTS { (a)-[:X*]->() }` — the zero-length case
        // (accepting the anchor itself against an unconstrained
        // target) must not be reachable for a bare `*`.
        let create_isolated = Query {
            cypher: "CREATE (a:ExistsProbeAE {name: 'a'})".to_string(),
            params: HashMap::new(),
        };
        executor
            .execute(&create_isolated)
            .expect("create should succeed");

        let isolated_query = Query {
            cypher: "MATCH (a:ExistsProbeAE {name: 'a'}) \
                     WHERE EXISTS { (a)-[:X*]->() } \
                     RETURN a.name AS name"
                .to_string(),
            params: HashMap::new(),
        };
        let result = executor
            .execute(&isolated_query)
            .expect("match should succeed");
        assert_eq!(
            result.rows.len(),
            0,
            "an isolated node has no witness for a bare `*` (min 1 hop)"
        );

        let create_edge = Query {
            cypher: "CREATE (b:ExistsProbeAE {name: 'b'})-[:X]->(c:ExistsProbeAE {name: 'c'})"
                .to_string(),
            params: HashMap::new(),
        };
        executor
            .execute(&create_edge)
            .expect("create should succeed");

        let edge_query = Query {
            cypher: "MATCH (b:ExistsProbeAE {name: 'b'}) \
                     WHERE EXISTS { (b)-[:X*]->() } \
                     RETURN b.name AS name"
                .to_string(),
            params: HashMap::new(),
        };
        let result = executor.execute(&edge_query).expect("match should succeed");
        assert_eq!(result.rows.len(), 1, "one real edge is a valid witness");
        assert_eq!(result.rows[0].values[0], json!("b"));
    }

    #[test]
    fn exists_bounded_var_length_respects_the_max_hop() {
        let (mut executor, _ctx) = create_test_executor();

        let create = Query {
            cypher: "CREATE (a:ExistsProbeX {name: 'a'})-[:X1]->(b:ExistsProbeX {name: 'b'})-\
                     [:X1]->(c:ExistsProbeX {name: 'c'})-[:X1]->(d:ExistsProbeX {name: 'd'})"
                .to_string(),
            params: HashMap::new(),
        };
        executor.execute(&create).expect("create should succeed");

        // `c` sits within the 1..2 bound (2 hops).
        let within_bound = Query {
            cypher: "MATCH (a:ExistsProbeX {name: 'a'}) \
                     WHERE EXISTS { (a)-[:X1*1..2]->(:ExistsProbeX {name: 'c'}) } \
                     RETURN a.name AS name"
                .to_string(),
            params: HashMap::new(),
        };
        let result = executor
            .execute(&within_bound)
            .expect("match should succeed");
        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.rows[0].values[0], json!("a"));

        // `d` is only reachable via a 3-hop path, past the `*1..2` max.
        let past_bound = Query {
            cypher: "MATCH (a:ExistsProbeX {name: 'a'}) \
                     WHERE EXISTS { (a)-[:X1*1..2]->(:ExistsProbeX {name: 'd'}) } \
                     RETURN a.name AS name"
                .to_string(),
            params: HashMap::new(),
        };
        let result = executor.execute(&past_bound).expect("match should succeed");
        assert_eq!(
            result.rows.len(),
            0,
            "the only path to `d` is 3 hops, past the *1..2 max"
        );
    }

    #[test]
    fn exists_exact_var_length_requires_the_exact_hop_count() {
        let (mut executor, _ctx) = create_test_executor();

        let create = Query {
            cypher: "CREATE (a:ExistsProbeY {name: 'a'})-[:Y1]->(b:ExistsProbeY {name: 'b'})-\
                     [:Y1]->(c:ExistsProbeY {name: 'c'})"
                .to_string(),
            params: HashMap::new(),
        };
        executor.execute(&create).expect("create should succeed");

        let one_hop_neighbor = Query {
            cypher: "MATCH (a:ExistsProbeY {name: 'a'}) \
                     WHERE EXISTS { (a)-[:Y1*2]->(:ExistsProbeY {name: 'b'}) } \
                     RETURN a.name AS name"
                .to_string(),
            params: HashMap::new(),
        };
        let result = executor
            .execute(&one_hop_neighbor)
            .expect("match should succeed");
        assert_eq!(result.rows.len(), 0, "`b` is 1 hop away, not exactly 2");

        let two_hop_neighbor = Query {
            cypher: "MATCH (a:ExistsProbeY {name: 'a'}) \
                     WHERE EXISTS { (a)-[:Y1*2]->(:ExistsProbeY {name: 'c'}) } \
                     RETURN a.name AS name"
                .to_string(),
            params: HashMap::new(),
        };
        let result = executor
            .execute(&two_hop_neighbor)
            .expect("match should succeed");
        assert_eq!(result.rows.len(), 1, "`c` is exactly 2 hops away");
        assert_eq!(result.rows[0].values[0], json!("a"));
    }

    #[test]
    fn exists_zero_length_var_length_matches_the_anchor_itself() {
        let (mut executor, _ctx) = create_test_executor();

        let create = Query {
            cypher: "CREATE (a:ExistsProbeZ {name: 'a'})".to_string(),
            params: HashMap::new(),
        };
        executor.execute(&create).expect("create should succeed");

        // No `:Z1` relationship was ever created, so the type never
        // resolves in the catalog — the zero-length case must still
        // be evaluated on its own merits and match `a` against
        // itself, consuming no edge.
        let query = Query {
            cypher: "MATCH (a:ExistsProbeZ {name: 'a'}) \
                     WHERE EXISTS { (a)-[:Z1*0..1]->(:ExistsProbeZ {name: 'a'}) } \
                     RETURN a.name AS name"
                .to_string(),
            params: HashMap::new(),
        };
        let result = executor.execute(&query).expect("match should succeed");
        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.rows[0].values[0], json!("a"));
    }

    #[test]
    fn exists_undirected_var_length_traverses_both_ways() {
        let (mut executor, _ctx) = create_test_executor();

        // Both edges are stored Outgoing (a -> b -> c); probing
        // undirected from `c` must still walk them backwards to `a`.
        let create = Query {
            cypher: "CREATE (a:ExistsProbeAA {name: 'a'})-[:AA1]->\
                     (b:ExistsProbeAA {name: 'b'})-[:AA1]->(c:ExistsProbeAA {name: 'c'})"
                .to_string(),
            params: HashMap::new(),
        };
        executor.execute(&create).expect("create should succeed");

        let query = Query {
            cypher: "MATCH (c:ExistsProbeAA {name: 'c'}) \
                     WHERE EXISTS { (c)-[:AA1*]-(:ExistsProbeAA {name: 'a'}) } \
                     RETURN c.name AS name"
                .to_string(),
            params: HashMap::new(),
        };
        let result = executor.execute(&query).expect("match should succeed");
        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.rows[0].values[0], json!("c"));
    }

    #[test]
    fn exists_var_length_isomorphism_blocks_reusing_the_only_edge() {
        let (mut executor, _ctx) = create_test_executor();

        // Exactly one edge in the graph, stored directed a -> b.
        // BOTH segments are undirected (`-`, not `->`): the `+`
        // var-length segment (min 1 hop) must consume the only edge
        // to reach `b`, and the fixed hop that follows is undirected
        // too, so — without relationship-isomorphism enforcement — it
        // could walk the SAME edge backwards from `b` to `a` and be
        // satisfied. A directed fixed hop would fail here for an
        // unrelated reason (no OUTGOING edge from `b`) without ever
        // exercising the isomorphism check, which is why this test
        // deliberately keeps both segments undirected.
        let create = Query {
            cypher: "CREATE (a:ExistsProbeAB {name: 'a'})-[:AB1]->(b:ExistsProbeAB {name: 'b'})"
                .to_string(),
            params: HashMap::new(),
        };
        executor.execute(&create).expect("create should succeed");

        let query = Query {
            cypher: "MATCH (a:ExistsProbeAB {name: 'a'}) \
                     WHERE EXISTS { (a)-[:AB1+]-()-[:AB1]-() } \
                     RETURN a.name AS name"
                .to_string(),
            params: HashMap::new(),
        };
        let result = executor.execute(&query).expect("match should succeed");
        assert_eq!(
            result.rows.len(),
            0,
            "the only edge can't satisfy both the undirected var-length segment \
             and the undirected fixed hop that follows it"
        );
    }

    #[test]
    fn not_exists_composes_with_var_length_relationships() {
        let (mut executor, _ctx) = create_test_executor();

        let create = Query {
            cypher: "CREATE (a:ExistsProbeAC {name: 'a'})-[:AC1]->(b:ExistsProbeAC {name: 'b'})"
                .to_string(),
            params: HashMap::new(),
        };
        executor.execute(&create).expect("create should succeed");

        // Only a 1-hop path exists; an exact-2-hop probe is false, so
        // NOT EXISTS must be true.
        let query = Query {
            cypher: "MATCH (a:ExistsProbeAC {name: 'a'}) \
                     WHERE NOT EXISTS { (a)-[:AC1*2]->() } \
                     RETURN a.name AS name"
                .to_string(),
            params: HashMap::new(),
        };
        let result = executor.execute(&query).expect("match should succeed");
        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.rows[0].values[0], json!("a"));
    }

    #[test]
    fn exists_named_rel_variable_on_var_length_relationship_is_rejected() {
        let (mut executor, _ctx) = create_test_executor();

        let create = Query {
            cypher: "CREATE (a:ExistsProbeAF {name: 'a'})-[:AF1]->(b:ExistsProbeAF {name: 'b'})"
                .to_string(),
            params: HashMap::new(),
        };
        executor.execute(&create).expect("create should succeed");

        // A named relationship variable on a variable-length hop would
        // bind a LIST<RELATIONSHIP> in full Cypher, which this probe
        // does not support — it must fail loudly, not silently bind a
        // single relationship value or ignore the variable.
        let query = Query {
            cypher: "MATCH (a:ExistsProbeAF {name: 'a'}) \
                     WHERE EXISTS { (a)-[r:AF1*]->() } \
                     RETURN a.name AS name"
                .to_string(),
            params: HashMap::new(),
        };
        let err = executor
            .execute(&query)
            .expect_err("a named rel-var on a var-length relationship must be rejected");
        assert!(
            err.to_string()
                .contains("ERR_VAR_LENGTH_REL_VARIABLE_NOT_IMPLEMENTED"),
            "unexpected error message: {err}"
        );
    }

    #[test]
    fn apply_cartesian_product_rejects_when_budget_is_absurdly_low() {
        let (mut executor, _ctx) = create_test_executor();
        // A trivial 2x2 product estimates to 2 * 2 * columns * 32 bytes
        // (>= 128 bytes even at columns=1). A 1-byte budget must reject
        // it regardless of how small the product actually is.
        executor.config.cartesian_product_max_bytes = 1;

        let mut context = ExecutionContext::new(HashMap::new(), None);
        context.set_variable("a", Value::Array(vec![json!(1), json!(2)]));

        let result = executor.apply_cartesian_product(&mut context, "b", vec![json!(3), json!(4)]);

        match result {
            Err(Error::OutOfMemory(msg)) => {
                assert!(
                    msg.contains("Cartesian product"),
                    "OutOfMemory message should name the offending operation: {msg}"
                );
            }
            other => {
                panic!("expected Err(Error::OutOfMemory(_)) under a 1-byte budget, got {other:?}")
            }
        }
    }

    #[test]
    fn apply_cartesian_product_succeeds_under_default_budget() {
        // Same shape as the low-budget test above, but with the
        // default (1 GiB) budget left untouched — proves the rejection
        // above comes specifically from the configured ceiling, not
        // from `apply_cartesian_product` being broken for any input.
        let (mut executor, _ctx) = create_test_executor();

        let mut context = ExecutionContext::new(HashMap::new(), None);
        context.set_variable("a", Value::Array(vec![json!(1), json!(2)]));

        executor
            .apply_cartesian_product(&mut context, "b", vec![json!(3), json!(4)])
            .expect("a 2x2 product must stay well under the default 1 GiB budget");

        assert_eq!(
            context.get_variable("a"),
            Some(&Value::Array(vec![json!(1), json!(1), json!(2), json!(2)]))
        );
        assert_eq!(
            context.get_variable("b"),
            Some(&Value::Array(vec![json!(3), json!(4), json!(3), json!(4)]))
        );
    }

    /// phase0_fix-materialize-recrosses-aligned-columns — DISCRIMINATING.
    /// After `apply_cartesian_product` aligns two columns to length 4
    /// (`a=[1,1,2,2]`, `b=[3,4,3,4]`, each index = one output row), the
    /// aligned materialiser must ZIP them into exactly 4 rows, while the
    /// general materialiser RE-crosses them into 4*4 = 16. The `k`-column
    /// gap is `N^(k-1)`; at query scale (`N=384`, `k=3`) that same
    /// re-cross is `384^3 ≈ 56.6M` rows (~13 GB), which froze the host.
    #[test]
    fn materialize_aligned_rows_zips_instead_of_recrossing() {
        let (mut executor, _ctx) = create_test_executor();

        let mut context = ExecutionContext::new(HashMap::new(), None);
        context.set_variable("a", Value::Array(vec![json!(1), json!(2)]));
        executor
            .apply_cartesian_product(&mut context, "b", vec![json!(3), json!(4)])
            .expect("2x2 product stays under the default budget");

        // Preconditions: both columns are aligned to length 4.
        assert_eq!(
            context.get_variable("a"),
            Some(&Value::Array(vec![json!(1), json!(1), json!(2), json!(2)]))
        );
        assert_eq!(
            context.get_variable("b"),
            Some(&Value::Array(vec![json!(3), json!(4), json!(3), json!(4)]))
        );

        // The general materialiser RE-crosses the aligned columns: 4 x 4 = 16.
        // This is the over-production the fix avoids (documented, not desired).
        let recrossed = executor
            .materialize_rows_from_variables(&context)
            .expect("materialize should succeed for this small aligned context");
        assert_eq!(
            recrossed.len(),
            16,
            "materialize_rows_from_variables re-crosses aligned columns (N^k); \
             this pins the bug the aligned path must avoid"
        );

        // The aligned materialiser ZIPS: exactly the 4 rows the columns
        // already represent, in index order.
        let zipped = executor.materialize_aligned_rows(&context);
        assert_eq!(
            zipped.len(),
            4,
            "materialize_aligned_rows must zip aligned columns to N rows, not N^k"
        );

        let mut pairs: Vec<(i64, i64)> = zipped
            .iter()
            .map(|row| {
                (
                    row["a"].as_i64().expect("a is an integer"),
                    row["b"].as_i64().expect("b is an integer"),
                )
            })
            .collect();
        pairs.sort_unstable();
        assert_eq!(
            pairs,
            vec![(1, 3), (1, 4), (2, 3), (2, 4)],
            "zipped rows must be the exact index-aligned (a, b) pairs"
        );
    }

    /// The cartesian-materialisation path inside
    /// `materialize_rows_from_variables` (two-plus same-length,
    /// multi-element arrays) must reject with `Error::OutOfMemory` when
    /// the estimated product exceeds the configured byte budget, instead
    /// of building the full row set — the same contract
    /// `apply_cartesian_product` enforces.
    #[test]
    fn materialize_rows_rejects_cartesian_product_over_budget() {
        let (mut executor, _ctx) = create_test_executor();
        // A 1-byte budget rejects any real product (each row is >= a few
        // dozen bytes even for one column).
        executor.set_cartesian_product_max_bytes(1);

        let mut context = ExecutionContext::new(HashMap::new(), None);
        // Two same-length, multi-element arrays => the cartesian branch.
        context.set_variable("a", Value::Array(vec![json!(1), json!(2), json!(3)]));
        context.set_variable("b", Value::Array(vec![json!(4), json!(5), json!(6)]));

        let err = executor
            .materialize_rows_from_variables(&context)
            .expect_err("a cartesian materialisation over the byte budget must error");
        assert!(
            matches!(err, crate::Error::OutOfMemory(_)),
            "expected Error::OutOfMemory, got {err:?}"
        );
    }
}
