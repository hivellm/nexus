//! List, collection-predicate, type-check, and type-coerce built-in functions
//! for the projection evaluator.
//!
//! Covers: `size`, `head`, `tail`, `last`, `range`, `reverse`, `reduce`,
//! `extract`, `all`, `any`, `none`, `single`, `coalesce`, `flatten`, `zip`,
//! `exists`, `isempty`, type-check predicates (`isinteger`, `isfloat`,
//! `isstring`, `isboolean`, `islist`, `ismap`, `isnode`, `isrelationship`,
//! `ispath`), type-conversion (`tointeger`, `tofloat`, `tostring`,
//! `toboolean`), and list-coerce variants (`tointegerlist`, `tofloatlist`,
//! `tostringlist`, `tobooleanlist`).

use super::super::super::context::ExecutionContext;
use super::super::super::engine::Executor;
use super::super::super::parser;
use crate::{Error, Result};
use serde_json::Value;
use std::collections::HashMap;

/// Hard cap on the number of elements `range()` may materialise into a
/// `Vec<Value>`. `serde_json::Value` is roughly 32-48 bytes on 64-bit
/// targets (Number/Bool/Null are inline, String/Array/Object are
/// pointer-sized), so `2_000_000 * 48 = 96 MB` — comfortably under the
/// ~256 MB "well clear of trouble" target even with allocator/Vec
/// overhead, since the element count is validated before the single
/// `Vec::with_capacity` call (no doubling growth).
const MAX_RANGE_ELEMENTS: usize = 2_000_000;

impl Executor {
    /// Computes how many elements `range(start, end, step)` would
    /// produce, using checked arithmetic throughout so a pathological
    /// span (e.g. `end` near `i64::MAX`) is rejected as a Cypher error
    /// instead of overflowing. Returns `Ok(0)` for an empty range.
    /// `step` must be non-zero — callers special-case `step == 0`
    /// before reaching here.
    fn range_element_count(start: i64, end: i64, step: i64) -> Result<usize> {
        debug_assert_ne!(step, 0, "range_element_count requires a non-zero step");
        // Normalise so `hi >= lo` is the "non-empty" test regardless of
        // step direction — `step.unsigned_abs()` handles `step ==
        // i64::MIN` correctly (`i64::MIN.abs()` itself would overflow).
        let (lo, hi, abs_step) = if step > 0 {
            (start, end, step as u64)
        } else {
            (end, start, step.unsigned_abs())
        };
        if hi < lo {
            return Ok(0);
        }
        // `hi >= lo` here, so the subtraction cannot go negative; it can
        // only fail to fit in i64 when `lo` is very negative and `hi` is
        // very positive (e.g. `range(i64::MIN, i64::MAX)`).
        let diff = hi.checked_sub(lo).ok_or_else(|| {
            Error::CypherExecution(format!(
                "ERR_RANGE_TOO_LARGE: range({start}, {end}, {step}) span overflows i64"
            ))
        })?;
        let steps = (diff as u64).checked_div(abs_step).ok_or_else(|| {
            Error::CypherExecution(format!(
                "ERR_RANGE_TOO_LARGE: range({start}, {end}, {step}) step computation overflowed"
            ))
        })?;
        let count = steps.checked_add(1).ok_or_else(|| {
            Error::CypherExecution(format!(
                "ERR_RANGE_TOO_LARGE: range({start}, {end}, {step}) element count overflowed"
            ))
        })?;
        usize::try_from(count).map_err(|_| {
            Error::CypherExecution(format!(
                "ERR_RANGE_TOO_LARGE: range({start}, {end}, {step}) element count \
                 ({count}) does not fit in usize"
            ))
        })
    }

    /// Builds the `Vec<Value>` for `range(start, end, step)`. Rejects the
    /// query with `Err` BEFORE allocating when the element count exceeds
    /// [`MAX_RANGE_ELEMENTS`]. The generation loop still uses
    /// `checked_add` for the `i += step` update (rather than the bare
    /// `+=` the unfixed code used) so it can never silently wrap past
    /// `i64::MAX`/`i64::MIN` even in a release build with overflow
    /// checks disabled — belt-and-suspenders alongside the upfront
    /// count check, which should make this branch unreachable in
    /// practice.
    fn build_range(start: i64, end: i64, step: i64) -> Result<Vec<Value>> {
        debug_assert_ne!(step, 0, "build_range requires a non-zero step");
        let count = Self::range_element_count(start, end, step)?;
        if count > MAX_RANGE_ELEMENTS {
            return Err(Error::CypherExecution(format!(
                "ERR_RANGE_TOO_LARGE: range({start}, {end}, {step}) would produce {count} \
                 elements, exceeding the {MAX_RANGE_ELEMENTS}-element cap; narrow the range \
                 or add LIMIT"
            )));
        }
        let mut result = Vec::with_capacity(count);
        let mut i = start;
        loop {
            let in_range = if step > 0 { i <= end } else { i >= end };
            if !in_range {
                break;
            }
            result.push(Value::Number(i.into()));
            i = match i.checked_add(step) {
                Some(next) => next,
                None => break,
            };
        }
        Ok(result)
    }

    /// Evaluate list, type-check, type-coerce, and collection-predicate
    /// built-in functions.
    ///
    /// Returns `None` if the function name is not handled here.
    pub(super) fn eval_builtin_list(
        &self,
        row: &HashMap<String, Value>,
        context: &ExecutionContext,
        name: &str,
        args: &[parser::Expression],
    ) -> Option<Result<Value>> {
        match name {
            // List functions
            "size" => {
                if let Some(arg) = args.first() {
                    let value = match self.evaluate_projection_expression(row, context, arg) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    return Some(match value {
                        Value::Array(arr) => Ok(Value::Number((arr.len() as i64).into())),
                        Value::String(s) => Ok(Value::Number((s.len() as i64).into())),
                        _ => Ok(Value::Null),
                    });
                }
                Some(Ok(Value::Null))
            }
            "head" => {
                if let Some(arg) = args.first() {
                    let value = match self.evaluate_projection_expression(row, context, arg) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    if let Value::Array(arr) = value {
                        return Some(Ok(arr.first().cloned().unwrap_or(Value::Null)));
                    }
                }
                Some(Ok(Value::Null))
            }
            "tail" => {
                if let Some(arg) = args.first() {
                    let value = match self.evaluate_projection_expression(row, context, arg) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    if let Value::Array(arr) = value {
                        if arr.len() > 1 {
                            return Some(Ok(Value::Array(arr[1..].to_vec())));
                        }
                        return Some(Ok(Value::Array(Vec::new())));
                    }
                }
                Some(Ok(Value::Null))
            }
            "last" => {
                if let Some(arg) = args.first() {
                    let value = match self.evaluate_projection_expression(row, context, arg) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    if let Value::Array(arr) = value {
                        return Some(Ok(arr.last().cloned().unwrap_or(Value::Null)));
                    }
                }
                Some(Ok(Value::Null))
            }
            "range" => {
                // range(start, end, [step])
                if args.len() >= 2 {
                    let start_val =
                        match self.evaluate_projection_expression(row, context, &args[0]) {
                            Ok(v) => v,
                            Err(e) => return Some(Err(e)),
                        };
                    let end_val = match self.evaluate_projection_expression(row, context, &args[1])
                    {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };

                    if let (Value::Number(start_num), Value::Number(end_num)) = (start_val, end_val)
                    {
                        let start = start_num
                            .as_i64()
                            .or_else(|| start_num.as_f64().map(|f| f as i64))
                            .unwrap_or(0);
                        let end = end_num
                            .as_i64()
                            .or_else(|| end_num.as_f64().map(|f| f as i64))
                            .unwrap_or(0);
                        let step = if args.len() >= 3 {
                            let step_val =
                                match self.evaluate_projection_expression(row, context, &args[2]) {
                                    Ok(v) => v,
                                    Err(e) => return Some(Err(e)),
                                };
                            if let Value::Number(s) = step_val {
                                s.as_i64()
                                    .or_else(|| s.as_f64().map(|f| f as i64))
                                    .unwrap_or(1)
                            } else {
                                1
                            }
                        } else {
                            1
                        };

                        if step == 0 {
                            return Some(Ok(Value::Array(Vec::new())));
                        }

                        return Some(match Self::build_range(start, end, step) {
                            Ok(result) => Ok(Value::Array(result)),
                            Err(e) => Err(e),
                        });
                    }
                }
                Some(Ok(Value::Null))
            }
            "reverse" => {
                if let Some(arg) = args.first() {
                    let value = match self.evaluate_projection_expression(row, context, arg) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    if let Value::Array(mut arr) = value {
                        arr.reverse();
                        return Some(Ok(Value::Array(arr)));
                    }
                }
                Some(Ok(Value::Null))
            }
            "reduce" => {
                // reduce(accumulator, variable IN list | expression)
                if args.len() >= 3 {
                    let acc_init = match self.evaluate_projection_expression(row, context, &args[0])
                    {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    let var_name = match self.evaluate_projection_expression(row, context, &args[1])
                    {
                        Ok(Value::String(s)) => s,
                        Ok(_) => return Some(Ok(Value::Null)),
                        Err(e) => return Some(Err(e)),
                    };
                    let list_val = match self.evaluate_projection_expression(row, context, &args[2])
                    {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    if let Value::Array(list) = list_val {
                        let expr = args.get(3).cloned();
                        let mut accumulator = acc_init;
                        for item in list {
                            let mut new_row = row.clone();
                            new_row.insert(var_name.clone(), item);
                            if let Some(ref expr) = expr {
                                accumulator = match self
                                    .evaluate_projection_expression(&new_row, context, expr)
                                {
                                    Ok(v) => v,
                                    Err(e) => return Some(Err(e)),
                                };
                            } else {
                                accumulator =
                                    new_row.get(&var_name).cloned().unwrap_or(Value::Null);
                            }
                        }
                        return Some(Ok(accumulator));
                    }
                }
                Some(Ok(Value::Null))
            }
            "extract" => {
                // extract(variable IN list | expression)
                if args.len() >= 2 {
                    let var_name = match self.evaluate_projection_expression(row, context, &args[0])
                    {
                        Ok(Value::String(s)) => s,
                        Ok(_) => return Some(Ok(Value::Null)),
                        Err(e) => return Some(Err(e)),
                    };
                    let list_val = match self.evaluate_projection_expression(row, context, &args[1])
                    {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    if let Value::Array(list) = list_val {
                        let expr = args.get(2).cloned();
                        let mut results = Vec::new();
                        for item in list {
                            let mut new_row = row.clone();
                            new_row.insert(var_name.clone(), item);
                            if let Some(ref expr) = expr {
                                if let Ok(result) =
                                    self.evaluate_projection_expression(&new_row, context, expr)
                                {
                                    results.push(result);
                                }
                            } else {
                                results
                                    .push(new_row.get(&var_name).cloned().unwrap_or(Value::Null));
                            }
                        }
                        return Some(Ok(Value::Array(results)));
                    }
                }
                Some(Ok(Value::Null))
            }
            "all" => {
                // all(variable IN list WHERE predicate)
                if args.len() >= 2 {
                    let list_val = match self.evaluate_projection_expression(row, context, &args[1])
                    {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    if let Value::Array(list) = list_val {
                        if list.is_empty() {
                            return Some(Ok(Value::Bool(true)));
                        }
                        if let Some(predicate) = args.get(2) {
                            let var_name =
                                match self.evaluate_projection_expression(row, context, &args[0]) {
                                    Ok(Value::String(s)) => s,
                                    Ok(_) => return Some(Ok(Value::Bool(false))),
                                    Err(_) => return Some(Ok(Value::Bool(false))),
                                };
                            for item in list {
                                let mut new_row = row.clone();
                                new_row.insert(var_name.clone(), item);
                                let result = match self
                                    .evaluate_projection_expression(&new_row, context, predicate)
                                {
                                    Ok(v) => v,
                                    Err(e) => return Some(Err(e)),
                                };
                                if !result.as_bool().unwrap_or(false) {
                                    return Some(Ok(Value::Bool(false)));
                                }
                            }
                            return Some(Ok(Value::Bool(true)));
                        }
                    }
                }
                Some(Ok(Value::Bool(false)))
            }
            "any" => {
                // any(variable IN list WHERE predicate)
                if args.len() >= 2 {
                    let list_val = match self.evaluate_projection_expression(row, context, &args[1])
                    {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    if let Value::Array(list) = list_val {
                        if list.is_empty() {
                            return Some(Ok(Value::Bool(false)));
                        }
                        if let Some(predicate) = args.get(2) {
                            let var_name =
                                match self.evaluate_projection_expression(row, context, &args[0]) {
                                    Ok(Value::String(s)) => s,
                                    Ok(_) => return Some(Ok(Value::Bool(false))),
                                    Err(_) => return Some(Ok(Value::Bool(false))),
                                };
                            for item in list {
                                let mut new_row = row.clone();
                                new_row.insert(var_name.clone(), item);
                                let result = match self
                                    .evaluate_projection_expression(&new_row, context, predicate)
                                {
                                    Ok(v) => v,
                                    Err(e) => return Some(Err(e)),
                                };
                                if result.as_bool().unwrap_or(false) {
                                    return Some(Ok(Value::Bool(true)));
                                }
                            }
                            return Some(Ok(Value::Bool(false)));
                        }
                    }
                }
                Some(Ok(Value::Bool(false)))
            }
            "none" => {
                // none(variable IN list WHERE predicate)
                if args.len() >= 2 {
                    let list_val = match self.evaluate_projection_expression(row, context, &args[1])
                    {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    if let Value::Array(list) = list_val {
                        if list.is_empty() {
                            return Some(Ok(Value::Bool(true)));
                        }
                        if let Some(predicate) = args.get(2) {
                            let var_name =
                                match self.evaluate_projection_expression(row, context, &args[0]) {
                                    Ok(Value::String(s)) => s,
                                    Ok(_) => return Some(Ok(Value::Bool(false))),
                                    Err(_) => return Some(Ok(Value::Bool(false))),
                                };
                            for item in list {
                                let mut new_row = row.clone();
                                new_row.insert(var_name.clone(), item);
                                let result = match self
                                    .evaluate_projection_expression(&new_row, context, predicate)
                                {
                                    Ok(v) => v,
                                    Err(e) => return Some(Err(e)),
                                };
                                if result.as_bool().unwrap_or(false) {
                                    return Some(Ok(Value::Bool(false)));
                                }
                            }
                            return Some(Ok(Value::Bool(true)));
                        }
                    }
                }
                Some(Ok(Value::Bool(true)))
            }
            "single" => {
                // single(variable IN list WHERE predicate)
                if args.len() >= 2 {
                    let list_val = match self.evaluate_projection_expression(row, context, &args[1])
                    {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    if let Value::Array(list) = list_val {
                        if list.is_empty() {
                            return Some(Ok(Value::Bool(false)));
                        }
                        if let Some(predicate) = args.get(2) {
                            let var_name =
                                match self.evaluate_projection_expression(row, context, &args[0]) {
                                    Ok(Value::String(s)) => s,
                                    Ok(_) => return Some(Ok(Value::Bool(false))),
                                    Err(_) => return Some(Ok(Value::Bool(false))),
                                };
                            let mut count = 0;
                            for item in list {
                                let mut new_row = row.clone();
                                new_row.insert(var_name.clone(), item);
                                let result = match self
                                    .evaluate_projection_expression(&new_row, context, predicate)
                                {
                                    Ok(v) => v,
                                    Err(e) => return Some(Err(e)),
                                };
                                if result.as_bool().unwrap_or(false) {
                                    count += 1;
                                    if count > 1 {
                                        return Some(Ok(Value::Bool(false)));
                                    }
                                }
                            }
                            return Some(Ok(Value::Bool(count == 1)));
                        }
                    }
                }
                Some(Ok(Value::Bool(false)))
            }
            "coalesce" => {
                // coalesce(expr1, expr2, ...) - returns first non-null value
                for arg in args {
                    let value = match self.evaluate_projection_expression(row, context, arg) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    if !value.is_null() {
                        return Some(Ok(value));
                    }
                }
                Some(Ok(Value::Null))
            }
            // phase4_cypher-parity-quick-wins §1.4 — `shuffle(list)`
            // returns a random permutation of `list`. Uses `rand`'s
            // thread-local RNG (the same crate + pattern as
            // `apoc::coll::shuffle`) — non-deterministic by design, so
            // callers needing reproducible order must sort explicitly.
            "shuffle" => {
                if let Some(arg) = args.first() {
                    let value = match self.evaluate_projection_expression(row, context, arg) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    if let Value::Array(mut arr) = value {
                        use rand::seq::SliceRandom;
                        let mut rng = rand::thread_rng();
                        arr.shuffle(&mut rng);
                        return Some(Ok(Value::Array(arr)));
                    }
                }
                Some(Ok(Value::Null))
            }
            // List functions
            "flatten" => {
                // flatten(list) - flattens a list of lists by one level
                if let Some(arg) = args.first() {
                    let value = match self.evaluate_projection_expression(row, context, arg) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    if let Value::Array(arr) = value {
                        let mut result = Vec::new();
                        for item in arr {
                            if let Value::Array(inner) = item {
                                result.extend(inner);
                            } else {
                                result.push(item);
                            }
                        }
                        return Some(Ok(Value::Array(result)));
                    }
                }
                Some(Ok(Value::Null))
            }
            "zip" => {
                // zip(list1, list2, ...) - zips multiple lists together
                if args.len() >= 2 {
                    let mut lists: Vec<Vec<Value>> = Vec::new();
                    let mut min_len = usize::MAX;

                    for arg in args {
                        let value = match self.evaluate_projection_expression(row, context, arg) {
                            Ok(v) => v,
                            Err(e) => return Some(Err(e)),
                        };
                        if let Value::Array(arr) = value {
                            min_len = min_len.min(arr.len());
                            lists.push(arr);
                        } else {
                            return Some(Ok(Value::Null));
                        }
                    }

                    let mut result = Vec::new();
                    for i in 0..min_len {
                        let mut tuple = Vec::new();
                        for list in &lists {
                            tuple.push(list[i].clone());
                        }
                        result.push(Value::Array(tuple));
                    }
                    return Some(Ok(Value::Array(result)));
                }
                Some(Ok(Value::Null))
            }
            // phase6_opencypher-quickwins §7 — `exists(prop)` scalar.
            "exists" => {
                if args.is_empty() {
                    return Some(Ok(Value::Null));
                }
                match &args[0] {
                    parser::Expression::PropertyAccess { variable, property } => {
                        let target = row.get(variable).cloned().unwrap_or(Value::Null);
                        Some(match target {
                            Value::Null => Ok(Value::Null),
                            Value::Object(obj) => match obj.get(property) {
                                Some(Value::Null) | None => Ok(Value::Bool(false)),
                                Some(_) => Ok(Value::Bool(true)),
                            },
                            _ => Ok(Value::Bool(false)),
                        })
                    }
                    other => {
                        let v = match self.evaluate_projection_expression(row, context, other) {
                            Ok(v) => v,
                            Err(e) => return Some(Err(e)),
                        };
                        Some(Ok(Value::Bool(!matches!(v, Value::Null))))
                    }
                }
            }
            // phase6_opencypher-quickwins §3 — polymorphic `isEmpty`.
            "isempty" => {
                let v = match args.first() {
                    Some(a) => match self.evaluate_projection_expression(row, context, a) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    },
                    None => return Some(Ok(Value::Null)),
                };
                Some(match v {
                    Value::Null => Ok(Value::Null),
                    Value::String(s) => Ok(Value::Bool(s.is_empty())),
                    Value::Array(a) => Ok(Value::Bool(a.is_empty())),
                    Value::Object(obj) => {
                        // Treat serialised graph entities as non-empty
                        if obj.contains_key("_nexus_id") {
                            Ok(Value::Bool(false))
                        } else {
                            Ok(Value::Bool(obj.is_empty()))
                        }
                    }
                    other => Err(Error::TypeMismatch {
                        expected: "STRING, LIST, or MAP".to_string(),
                        actual: super::type_name_of(&other).to_string(),
                    }),
                })
            }
            // Type conversion functions
            "tointeger" => {
                if let Some(arg) = args.first() {
                    let value = match self.evaluate_projection_expression(row, context, arg) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    match value {
                        Value::Number(n) => {
                            if let Some(i) = n.as_i64() {
                                return Some(Ok(Value::Number(i.into())));
                            }
                            if let Some(f) = n.as_f64() {
                                return Some(Ok(Value::Number((f as i64).into())));
                            }
                        }
                        Value::String(s) => {
                            if let Ok(i) = s.parse::<i64>() {
                                return Some(Ok(Value::Number(i.into())));
                            }
                        }
                        _ => {}
                    }
                }
                Some(Ok(Value::Null))
            }
            "tofloat" => {
                if let Some(arg) = args.first() {
                    let value = match self.evaluate_projection_expression(row, context, arg) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    match value {
                        Value::Number(n) => {
                            if let Some(f) = n.as_f64() {
                                return Some(
                                    serde_json::Number::from_f64(f)
                                        .map(Value::Number)
                                        .ok_or_else(|| Error::TypeMismatch {
                                            expected: "float".to_string(),
                                            actual: "non-finite".to_string(),
                                        }),
                                );
                            }
                        }
                        Value::String(s) => {
                            if let Ok(f) = s.parse::<f64>() {
                                return Some(
                                    serde_json::Number::from_f64(f)
                                        .map(Value::Number)
                                        .ok_or_else(|| Error::TypeMismatch {
                                            expected: "float".to_string(),
                                            actual: "non-finite".to_string(),
                                        }),
                                );
                            }
                        }
                        _ => {}
                    }
                }
                Some(Ok(Value::Null))
            }
            "tostring" => {
                if let Some(arg) = args.first() {
                    let value = match self.evaluate_projection_expression(row, context, arg) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    return Some(match value {
                        Value::String(s) => Ok(Value::String(s)),
                        Value::Number(n) => Ok(Value::String(n.to_string())),
                        Value::Bool(b) => Ok(Value::String(b.to_string())),
                        Value::Null => Ok(Value::Null),
                        Value::Array(_) | Value::Object(_) => Ok(Value::String(value.to_string())),
                    });
                }
                Some(Ok(Value::Null))
            }
            "toboolean" => {
                if let Some(arg) = args.first() {
                    let value = match self.evaluate_projection_expression(row, context, arg) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    return Some(match value {
                        Value::Bool(b) => Ok(Value::Bool(b)),
                        Value::String(s) => {
                            let lower = s.to_lowercase();
                            if lower == "true" {
                                Ok(Value::Bool(true))
                            } else if lower == "false" {
                                Ok(Value::Bool(false))
                            } else {
                                Ok(Value::Null)
                            }
                        }
                        Value::Number(n) => Ok(Value::Bool(n.as_f64().unwrap_or(0.0) != 0.0)),
                        _ => Ok(Value::Null),
                    });
                }
                Some(Ok(Value::Null))
            }
            // phase6_opencypher-quickwins §1 — type-check predicates.
            "isinteger" => {
                let v = match args.first() {
                    Some(a) => match self.evaluate_projection_expression(row, context, a) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    },
                    None => return Some(Ok(Value::Null)),
                };
                Some(match v {
                    Value::Null => Ok(Value::Null),
                    Value::Number(n) => Ok(Value::Bool(n.is_i64() || n.is_u64())),
                    _ => Ok(Value::Bool(false)),
                })
            }
            "isfloat" => {
                let v = match args.first() {
                    Some(a) => match self.evaluate_projection_expression(row, context, a) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    },
                    None => return Some(Ok(Value::Null)),
                };
                Some(match v {
                    Value::Null => Ok(Value::Null),
                    Value::Number(n) => Ok(Value::Bool(n.is_f64())),
                    _ => Ok(Value::Bool(false)),
                })
            }
            "isstring" => {
                let v = match args.first() {
                    Some(a) => match self.evaluate_projection_expression(row, context, a) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    },
                    None => return Some(Ok(Value::Null)),
                };
                Some(match v {
                    Value::Null => Ok(Value::Null),
                    Value::String(_) => Ok(Value::Bool(true)),
                    _ => Ok(Value::Bool(false)),
                })
            }
            "isboolean" => {
                let v = match args.first() {
                    Some(a) => match self.evaluate_projection_expression(row, context, a) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    },
                    None => return Some(Ok(Value::Null)),
                };
                Some(match v {
                    Value::Null => Ok(Value::Null),
                    Value::Bool(_) => Ok(Value::Bool(true)),
                    _ => Ok(Value::Bool(false)),
                })
            }
            "islist" => {
                let v = match args.first() {
                    Some(a) => match self.evaluate_projection_expression(row, context, a) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    },
                    None => return Some(Ok(Value::Null)),
                };
                Some(match v {
                    Value::Null => Ok(Value::Null),
                    Value::Array(_) => Ok(Value::Bool(true)),
                    _ => Ok(Value::Bool(false)),
                })
            }
            "ismap" => {
                // A MAP is any Object that ISN'T one of Nexus's
                // serialised graph entities (node/relationship carry
                // `_nexus_id`). Plain user maps are Object values
                // without `_nexus_id`.
                let v = match args.first() {
                    Some(a) => match self.evaluate_projection_expression(row, context, a) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    },
                    None => return Some(Ok(Value::Null)),
                };
                Some(match v {
                    Value::Null => Ok(Value::Null),
                    Value::Object(obj) => Ok(Value::Bool(!obj.contains_key("_nexus_id"))),
                    _ => Ok(Value::Bool(false)),
                })
            }
            "isnode" => {
                let v = match args.first() {
                    Some(a) => match self.evaluate_projection_expression(row, context, a) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    },
                    None => return Some(Ok(Value::Null)),
                };
                Some(match v {
                    Value::Null => Ok(Value::Null),
                    Value::Object(obj) => Ok(Value::Bool(
                        obj.contains_key("_nexus_id") && !obj.contains_key("type"),
                    )),
                    _ => Ok(Value::Bool(false)),
                })
            }
            "isrelationship" => {
                let v = match args.first() {
                    Some(a) => match self.evaluate_projection_expression(row, context, a) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    },
                    None => return Some(Ok(Value::Null)),
                };
                Some(match v {
                    Value::Null => Ok(Value::Null),
                    Value::Object(obj) => Ok(Value::Bool(
                        obj.contains_key("_nexus_id") && obj.contains_key("type"),
                    )),
                    _ => Ok(Value::Bool(false)),
                })
            }
            "ispath" => {
                // Paths currently surface through the executor as
                // Arrays of alternating node/relationship Objects.
                let v = match args.first() {
                    Some(a) => match self.evaluate_projection_expression(row, context, a) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    },
                    None => return Some(Ok(Value::Null)),
                };
                Some(match v {
                    Value::Null => Ok(Value::Null),
                    Value::Array(items) if !items.is_empty() => {
                        let is_path = items.iter().all(
                            |el| matches!(el, Value::Object(o) if o.contains_key("_nexus_id")),
                        );
                        Ok(Value::Bool(is_path))
                    }
                    _ => Ok(Value::Bool(false)),
                })
            }
            // phase6_opencypher-quickwins §2 — list type-converter functions.
            "tointegerlist" => {
                let v = match args.first() {
                    Some(a) => match self.evaluate_projection_expression(row, context, a) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    },
                    None => return Some(Ok(Value::Null)),
                };
                Some(match v {
                    Value::Null => Ok(Value::Null),
                    Value::Array(items) => {
                        let out: Vec<Value> = items
                            .into_iter()
                            .map(|el| match el {
                                Value::Null => Value::Null,
                                Value::Number(n) => n
                                    .as_i64()
                                    .or_else(|| n.as_f64().map(|f| f as i64))
                                    .map(|i| Value::Number(i.into()))
                                    .unwrap_or(Value::Null),
                                Value::Bool(b) => Value::Number((if b { 1i64 } else { 0 }).into()),
                                Value::String(s) => s
                                    .parse::<i64>()
                                    .ok()
                                    .or_else(|| s.parse::<f64>().ok().map(|f| f as i64))
                                    .map(|i| Value::Number(i.into()))
                                    .unwrap_or(Value::Null),
                                _ => Value::Null,
                            })
                            .collect();
                        Ok(Value::Array(out))
                    }
                    other => Err(Error::TypeMismatch {
                        expected: "LIST".to_string(),
                        actual: super::type_name_of(&other).to_string(),
                    }),
                })
            }
            "tofloatlist" => {
                let v = match args.first() {
                    Some(a) => match self.evaluate_projection_expression(row, context, a) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    },
                    None => return Some(Ok(Value::Null)),
                };
                Some(match v {
                    Value::Null => Ok(Value::Null),
                    Value::Array(items) => {
                        let out: Vec<Value> = items
                            .into_iter()
                            .map(|el| match el {
                                Value::Null => Value::Null,
                                Value::Number(n) => n
                                    .as_f64()
                                    .and_then(serde_json::Number::from_f64)
                                    .map(Value::Number)
                                    .unwrap_or(Value::Null),
                                Value::Bool(b) => {
                                    serde_json::Number::from_f64(if b { 1.0 } else { 0.0 })
                                        .map(Value::Number)
                                        .unwrap_or(Value::Null)
                                }
                                Value::String(s) => s
                                    .parse::<f64>()
                                    .ok()
                                    .and_then(serde_json::Number::from_f64)
                                    .map(Value::Number)
                                    .unwrap_or(Value::Null),
                                _ => Value::Null,
                            })
                            .collect();
                        Ok(Value::Array(out))
                    }
                    other => Err(Error::TypeMismatch {
                        expected: "LIST".to_string(),
                        actual: super::type_name_of(&other).to_string(),
                    }),
                })
            }
            "tostringlist" => {
                let v = match args.first() {
                    Some(a) => match self.evaluate_projection_expression(row, context, a) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    },
                    None => return Some(Ok(Value::Null)),
                };
                Some(match v {
                    Value::Null => Ok(Value::Null),
                    Value::Array(items) => {
                        let out: Vec<Value> = items
                            .into_iter()
                            .map(|el| match el {
                                Value::Null => Value::Null,
                                Value::String(s) => Value::String(s),
                                Value::Number(n) => Value::String(n.to_string()),
                                Value::Bool(b) => Value::String(b.to_string()),
                                other => Value::String(other.to_string()),
                            })
                            .collect();
                        Ok(Value::Array(out))
                    }
                    other => Err(Error::TypeMismatch {
                        expected: "LIST".to_string(),
                        actual: super::type_name_of(&other).to_string(),
                    }),
                })
            }
            "tobooleanlist" => {
                let v = match args.first() {
                    Some(a) => match self.evaluate_projection_expression(row, context, a) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    },
                    None => return Some(Ok(Value::Null)),
                };
                Some(match v {
                    Value::Null => Ok(Value::Null),
                    Value::Array(items) => {
                        let out: Vec<Value> = items
                            .into_iter()
                            .map(|el| match el {
                                Value::Null => Value::Null,
                                Value::Bool(b) => Value::Bool(b),
                                Value::Number(n) => Value::Bool(n.as_f64().unwrap_or(0.0) != 0.0),
                                Value::String(s) => {
                                    let lo = s.to_lowercase();
                                    if lo == "true" {
                                        Value::Bool(true)
                                    } else if lo == "false" {
                                        Value::Bool(false)
                                    } else {
                                        Value::Null
                                    }
                                }
                                _ => Value::Null,
                            })
                            .collect();
                        Ok(Value::Array(out))
                    }
                    other => Err(Error::TypeMismatch {
                        expected: "LIST".to_string(),
                        actual: super::type_name_of(&other).to_string(),
                    }),
                })
            }
            _ => None,
        }
    }
}
