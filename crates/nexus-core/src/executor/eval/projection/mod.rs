//! Projection expression evaluator — directory module.
//!
//! Previously the monolithic `projection.rs` (4297 lines). Split into
//! cohesive submodules by function family; zero logic changes, only
//! `use`-path adjustments and visibility annotations.
//!
//! Public entry points (visibility `pub(in crate::executor)`):
//! - [`Executor::evaluate_projection_expression`] — in `core`
//! - [`Executor::evaluate_collect_subquery`]      — in `core`

// ── submodules ───────────────────────────────────────────────────────────────
mod core;
mod fn_geo;
mod fn_graph;
mod fn_list;
mod fn_math;
mod fn_string;
mod fn_temporal;

// ── shared free function ─────────────────────────────────────────────────────

/// openCypher-ish type name used in error messages from type-check and
/// list-coercion builtins. Keeps the error surface aligned with the
/// openCypher spec's `INTEGER`, `FLOAT`, `STRING`, etc.
pub(super) fn type_name_of(v: &serde_json::Value) -> &'static str {
    match v {
        serde_json::Value::Null => "NULL",
        serde_json::Value::Bool(_) => "BOOLEAN",
        serde_json::Value::Number(n) => {
            if n.is_i64() || n.is_u64() {
                "INTEGER"
            } else {
                "FLOAT"
            }
        }
        serde_json::Value::String(_) => "STRING",
        serde_json::Value::Array(_) => "LIST",
        serde_json::Value::Object(_) => "MAP",
    }
}
