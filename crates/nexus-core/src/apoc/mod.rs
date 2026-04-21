//! APOC ("Awesome Procedures on Cypher") compatibility surface
//! (phase6_opencypher-apoc-ecosystem).
//!
//! Neo4j's APOC is the ecosystem's unofficial standard library —
//! every real-world Cypher migration we've seen leans on it. This
//! module ships a Nexus-native implementation of the ~100 most-used
//! procedures across five namespaces:
//!
//! - `apoc.coll.*` — list / set operations
//! - `apoc.map.*` — map manipulation
//! - `apoc.text.*` — string similarity, regex, phonetic metrics
//! - `apoc.date.*` — timezone-aware date formatting / parsing
//! - `apoc.schema.*` — schema introspection beyond `db.schema.*`
//!
//! External surfaces (`apoc.load.*`, `apoc.export.*`, `apoc.path.*`,
//! `apoc.periodic.*`) live in follow-up tasks — they pull in
//! HTTP / filesystem sandboxing and depend on the
//! `CALL ... IN TRANSACTIONS` subquery task not yet merged.
//!
//! ## Shape
//!
//! Every procedure consumes `Vec<serde_json::Value>` arguments and
//! returns a `Result<ApocResult>`. `ApocResult` is a thin wrapper
//! around `(columns, rows)` matching the executor's `ResultSet`
//! shape so dispatch can hand the result directly to the caller.
//!
//! Dispatch is string-matched from
//! `executor::operators::procedures::execute_call_procedure` through
//! [`dispatch`] below — the executor identifies the `apoc.*` prefix
//! and routes here. Unknown `apoc.*` names raise
//! `ERR_PROC_NOT_FOUND` with a list of the known procedures from
//! the same namespace.

use crate::{Error, Result};
use serde_json::Value;

pub mod coll;
pub mod date;
pub mod map;
pub mod schema;
pub mod text;

/// Return shape of every APOC procedure — column names + row values.
/// Matches the columns/rows pair the executor exposes to drivers.
#[derive(Debug)]
pub struct ApocResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Value>>,
}

impl ApocResult {
    /// Single-value result — one column named `value`, one row.
    pub fn scalar(value: Value) -> Self {
        Self {
            columns: vec!["value".to_string()],
            rows: vec![vec![value]],
        }
    }

    /// Multi-row single-column — each row holds one `value` cell.
    pub fn stream(values: Vec<Value>) -> Self {
        Self {
            columns: vec!["value".to_string()],
            rows: values.into_iter().map(|v| vec![v]).collect(),
        }
    }
}

/// Route `apoc.<namespace>.<proc>(args)` to the right implementation.
///
/// Returns `Ok(None)` if `name` does not begin with `apoc.` — the
/// caller falls back to other procedure handlers. Returns
/// `Ok(Some(result))` when we recognise and execute the procedure,
/// `Err(ERR_PROC_NOT_FOUND)` when the prefix matches but no
/// implementation exists.
pub fn dispatch(name: &str, args: Vec<Value>) -> Result<Option<ApocResult>> {
    let Some(rest) = name.strip_prefix("apoc.") else {
        return Ok(None);
    };
    let (ns, proc) = match rest.split_once('.') {
        Some(pair) => pair,
        None => return Err(not_found(name)),
    };
    let out = match ns {
        "coll" => coll::dispatch(proc, args)?,
        "map" => map::dispatch(proc, args)?,
        "text" => text::dispatch(proc, args)?,
        "date" => date::dispatch(proc, args)?,
        "schema" => schema::dispatch(proc, args)?,
        _ => return Err(not_found(name)),
    };
    Ok(Some(out))
}

/// List every `apoc.*` procedure name we recognise. Used by
/// `dbms.procedures()` and by the not-found error message.
pub fn list_procedures() -> Vec<&'static str> {
    let mut out = Vec::new();
    out.extend(coll::list().iter().copied());
    out.extend(map::list().iter().copied());
    out.extend(text::list().iter().copied());
    out.extend(date::list().iter().copied());
    out.extend(schema::list().iter().copied());
    out.sort_unstable();
    out
}

pub(crate) fn not_found(name: &str) -> Error {
    Error::CypherExecution(format!(
        "ERR_PROC_NOT_FOUND: procedure {name:?} is not implemented"
    ))
}

/// Convenience — emit an `ERR_INVALID_ARG_*` error with the procedure
/// name for the caller's log context.
pub(crate) fn bad_arg(proc: &str, reason: &str) -> Error {
    Error::CypherExecution(format!("ERR_INVALID_ARG_VALUE: {proc}: {reason}"))
}

/// Resolve an optional argument slot, treating a missing / NULL arg
/// as the supplied default.
pub(crate) fn arg_or<T, F>(args: &[Value], idx: usize, default: T, f: F) -> Result<T>
where
    F: FnOnce(&Value) -> Result<T>,
{
    match args.get(idx) {
        None | Some(Value::Null) => Ok(default),
        Some(v) => f(v),
    }
}

/// Pull a required list argument; `null` collapses to an empty list
/// per APOC convention.
pub(crate) fn as_list(v: &Value) -> Vec<Value> {
    match v {
        Value::Array(a) => a.clone(),
        Value::Null => Vec::new(),
        other => vec![other.clone()],
    }
}

/// Type-ordinal used by `apoc.coll.sort` for mixed-type lists.
/// Matches Neo4j's NULL < BOOLEAN < INTEGER < FLOAT < STRING < LIST
/// < MAP rule.
pub(crate) fn type_ordinal(v: &Value) -> u8 {
    match v {
        Value::Null => 0,
        Value::Bool(_) => 1,
        Value::Number(n) if n.is_i64() || n.is_u64() => 2,
        Value::Number(_) => 3,
        Value::String(_) => 4,
        Value::Array(_) => 5,
        Value::Object(_) => 6,
    }
}

/// Cross-type comparator used by `apoc.coll.sort` — falls back to
/// the type ordinal when two values aren't same-type comparable.
pub(crate) fn cmp_values(a: &Value, b: &Value) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let ord_a = type_ordinal(a);
    let ord_b = type_ordinal(b);
    if ord_a != ord_b {
        return ord_a.cmp(&ord_b);
    }
    match (a, b) {
        (Value::Null, Value::Null) => Ordering::Equal,
        (Value::Bool(x), Value::Bool(y)) => x.cmp(y),
        (Value::Number(x), Value::Number(y)) => {
            let xf = x.as_f64().unwrap_or(0.0);
            let yf = y.as_f64().unwrap_or(0.0);
            xf.partial_cmp(&yf).unwrap_or(Ordering::Equal)
        }
        (Value::String(x), Value::String(y)) => x.cmp(y),
        (Value::Array(x), Value::Array(y)) => x.len().cmp(&y.len()),
        (Value::Object(x), Value::Object(y)) => x.len().cmp(&y.len()),
        _ => Ordering::Equal,
    }
}
