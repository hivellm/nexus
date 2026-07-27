# Rows with no relationship/source marker silently collapse in generic row dedup

**Category**: code
**Tags**: executor, expand, cypher, dedup, count, aggregation, opencypher

## Description

`update_result_set_from_rows` (`executor/eval/helpers.rs`) keys its dedup on the `_nexus_id`-bearing values found in a row: rows with >1 entity id use the relationship id (found via `is_relationship_value`) plus the other bound vars; rows with exactly 1 entity id fall back to `node_{id}` alone. `execute_expand`'s source-less scan branch (`source_var.is_empty()`, `expand.rs`) builds a row per RELATIONSHIP it enumerates, but when BOTH `source_var` and `rel_var` are empty (a fully-anonymous pattern like `MATCH ()-[:TYPE]->()`) the row it emits carries only the target node's identity — no source, no relationship marker. That row then hits the 1-entity dedup branch and gets keyed purely by target-node id, so every relationship sharing a target node collapses into a single row.

Confirmed repro: `MATCH ()-[:HAS_CREATOR]->() RETURN count(*)` returned the number of DISTINCT target `Person` nodes (1461) instead of the true relationship count (286744); binding the rel var or labelling either endpoint routed the query onto a different Expand path that already carries >=2 entity ids per row and dedups correctly.

Fixed by stashing the relationship's own identity under an internal-only key (`ANON_REL_IDENTITY_KEY = "__nexus_anon_rel_identity"`) in the source-less branch whenever `rel_var` is empty, so the dedup step's >1-entity/relationship-keyed branch is taken instead of the 1-entity node-identity fallback. Never surfaces in RETURN output because only user-declared variables are ever projected; double-underscore-prefixed keys are the established internal-key convention in this same dedup function (see the `has_match_columns` `col.starts_with("__")` exclusion).

## Example

```rust
// In execute_expand's source-less scan branch (source_var.is_empty()):
if !rel_var.is_empty() {
    let relationship_value = self.read_relationship_as_value(&rel_info)?;
    new_row.insert(rel_var.to_string(), relationship_value);
} else {
    // No rel_var AND no source_var: without this, the row carries only
    // the target node's identity and the generic dedup collapses every
    // relationship sharing a target into one row.
    let relationship_value = self.read_relationship_as_value(&rel_info)?;
    new_row.insert(ANON_REL_IDENTITY_KEY.to_string(), relationship_value);
}
```

## When to Use

Whenever adding a new Expand-family operator branch (or any operator feeding rows into `update_result_set_from_rows`) that can emit a row containing fewer than 2 `_nexus_id`-bearing values for what is logically a relationship match — always carry at least the relationship id (or source+target) in the row, even under an internal-only key, so the generic dedup does not mistake relationship-cardinality output for node-existence output.
