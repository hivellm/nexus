//! Node/relationship creation operators and expression coercion helpers,
//! split into focused submodules:
//! - `pattern` — `execute_create_pattern_with_variables` /
//!   `execute_create_pattern_internal`: realise a standalone CREATE pattern
//!   into actual nodes/relationships, threading previously-bound variables
//!   into the pattern.
//! - `properties` — `resolve_property_expr_for_create` /
//!   `resolve_standalone_create_property` / `expression_to_json_value` /
//!   `expression_to_string`: coerce parser expressions into property-value
//!   form for persistence. `check_constraints` runs NOT NULL and uniqueness
//!   guards before insert.
//! - `with_context` — `execute_create_with_context`: drives CREATE with
//!   upstream MATCH context (row-aware path).

mod pattern;
mod properties;
mod with_context;

use super::super::parser;

/// Convert an AST-level conflict policy to the storage-level one.
pub(in crate::executor) fn ast_conflict_policy_to_storage(
    p: parser::AstConflictPolicy,
) -> crate::storage::external_id::ConflictPolicy {
    use crate::storage::external_id::ConflictPolicy;
    match p {
        parser::AstConflictPolicy::Error => ConflictPolicy::Error,
        parser::AstConflictPolicy::Match => ConflictPolicy::Match,
        parser::AstConflictPolicy::Replace => ConflictPolicy::Replace,
    }
}
