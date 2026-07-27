# Null property values are absent — never index them, reject null MERGE keys

**Category**: architecture
**Tags**: null, index, merge, neo4j-compat, property-index, analysis:phase6_clean-graph-rebuild-null-ids

## Description

Neo4j treats a property whose value is null as absent. Mirror this: (1) never insert null into the typed property index (central no-op in PropertyIndex::add_property), so find_exact(..,Null) is always empty and null-keyed nodes can't be addressed/pollute seeks; (2) MERGE with a null property value errors "Cannot merge node using null property value for {key}", checked before match-or-create.

## Example

// crates/nexus-core/src/index/mod.rs — add_property
if value == PropertyValue::Null { return Ok(()); }

// crates/nexus-core/src/engine/mod.rs — process_merge_clause (before find/create)
if let Some(prop_map) = &node_pattern.properties {
    for (key, expr) in &prop_map.properties {
        if matches!(self.expression_to_json_value(expr)?, serde_json::Value::Null) {
            return Err(Error::CypherExecution(format!(
                "Cannot merge node using null property value for {key}")));
        }
    }
}

## When to Use

Any property-graph write/index path where Neo4j compatibility matters and null values could otherwise be stored or indexed.

## When NOT to Use

Stores that intentionally treat null as a first-class, queryable value (non-Neo4j semantics).
