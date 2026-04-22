//! `apoc.convert.*` — type conversion utilities.

use super::{ApocResult, bad_arg, not_found};
use crate::{Error, Result};
use serde_json::{Map, Value, json};

pub fn list() -> &'static [&'static str] {
    &[
        "apoc.convert.toJson",
        "apoc.convert.fromJsonMap",
        "apoc.convert.fromJsonList",
        "apoc.convert.toMap",
        "apoc.convert.toList",
        "apoc.convert.toString",
        "apoc.convert.toInteger",
        "apoc.convert.toFloat",
        "apoc.convert.toBoolean",
        "apoc.convert.toStringList",
        "apoc.convert.toIntList",
        "apoc.convert.toFloatList",
        "apoc.convert.toBooleanList",
    ]
}

pub fn dispatch(proc: &str, args: Vec<Value>) -> Result<ApocResult> {
    match proc {
        "toJson" => to_json(args),
        "fromJsonMap" => from_json_map(args),
        "fromJsonList" => from_json_list(args),
        "toMap" => to_map(args),
        "toList" => to_list_proc(args),
        "toString" => to_string_proc(args),
        "toInteger" => to_integer(args),
        "toFloat" => to_float(args),
        "toBoolean" => to_boolean(args),
        "toStringList" => to_typed_list(args, |v| Ok(Value::String(display(v)))),
        "toIntList" => to_typed_list(args, |v| {
            v.as_i64()
                .map(|i| Value::Number(i.into()))
                .or_else(|| v.as_f64().map(|f| Value::Number((f as i64).into())))
                .or_else(|| match v {
                    Value::String(s) => s.parse::<i64>().ok().map(|i| Value::Number(i.into())),
                    _ => None,
                })
                .ok_or_else(|| bad_arg("apoc.convert.toIntList", "non-integer element"))
        }),
        "toFloatList" => to_typed_list(args, |v| {
            v.as_f64()
                .map(|f| {
                    serde_json::Number::from_f64(f)
                        .map(Value::Number)
                        .unwrap_or(Value::Null)
                })
                .or_else(|| match v {
                    Value::String(s) => s
                        .parse::<f64>()
                        .ok()
                        .and_then(|f| serde_json::Number::from_f64(f).map(Value::Number)),
                    _ => None,
                })
                .ok_or_else(|| bad_arg("apoc.convert.toFloatList", "non-float element"))
        }),
        "toBooleanList" => to_typed_list(args, |v| match v {
            Value::Bool(b) => Ok(Value::Bool(*b)),
            Value::String(s) => match s.to_ascii_lowercase().as_str() {
                "true" => Ok(Value::Bool(true)),
                "false" => Ok(Value::Bool(false)),
                _ => Err(bad_arg("apoc.convert.toBooleanList", "not a boolean")),
            },
            _ => Err(bad_arg("apoc.convert.toBooleanList", "not a boolean")),
        }),
        _ => Err(not_found(&format!("apoc.convert.{proc}"))),
    }
}

fn display(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => "null".to_string(),
        other => other.to_string(),
    }
}

fn to_json(args: Vec<Value>) -> Result<ApocResult> {
    let v = args.first().cloned().unwrap_or(Value::Null);
    let serialized = serde_json::to_string(&v)
        .map_err(|e| bad_arg("apoc.convert.toJson", &format!("serialisation failed: {e}")))?;
    Ok(ApocResult::scalar(Value::String(serialized)))
}

fn from_json_map(args: Vec<Value>) -> Result<ApocResult> {
    let s = args
        .first()
        .and_then(|v| v.as_str())
        .ok_or_else(|| bad_arg("apoc.convert.fromJsonMap", "arg 0 must be STRING"))?;
    let parsed: Value = serde_json::from_str(s)
        .map_err(|e| bad_arg("apoc.convert.fromJsonMap", &format!("bad JSON: {e}")))?;
    match parsed {
        Value::Object(_) => Ok(ApocResult::scalar(parsed)),
        _ => Err(bad_arg(
            "apoc.convert.fromJsonMap",
            "JSON root is not an object",
        )),
    }
}

fn from_json_list(args: Vec<Value>) -> Result<ApocResult> {
    let s = args
        .first()
        .and_then(|v| v.as_str())
        .ok_or_else(|| bad_arg("apoc.convert.fromJsonList", "arg 0 must be STRING"))?;
    let parsed: Value = serde_json::from_str(s)
        .map_err(|e| bad_arg("apoc.convert.fromJsonList", &format!("bad JSON: {e}")))?;
    match parsed {
        Value::Array(_) => Ok(ApocResult::scalar(parsed)),
        _ => Err(bad_arg(
            "apoc.convert.fromJsonList",
            "JSON root is not an array",
        )),
    }
}

fn to_map(args: Vec<Value>) -> Result<ApocResult> {
    // toMap takes a list of [key, value] pairs or a STRING of JSON.
    match args.first() {
        Some(Value::Array(pairs)) => {
            let mut m = Map::new();
            for p in pairs {
                if let Value::Array(kv) = p {
                    if kv.len() == 2 {
                        let key = match &kv[0] {
                            Value::String(s) => s.clone(),
                            other => other.to_string(),
                        };
                        m.insert(key, kv[1].clone());
                    }
                }
            }
            Ok(ApocResult::scalar(Value::Object(m)))
        }
        Some(Value::String(s)) => {
            let parsed: Value = serde_json::from_str(s)
                .map_err(|e| bad_arg("apoc.convert.toMap", &format!("bad JSON: {e}")))?;
            match parsed {
                Value::Object(_) => Ok(ApocResult::scalar(parsed)),
                _ => Err(bad_arg("apoc.convert.toMap", "JSON root is not an object")),
            }
        }
        Some(Value::Object(m)) => Ok(ApocResult::scalar(Value::Object(m.clone()))),
        Some(Value::Null) | None => Ok(ApocResult::scalar(Value::Object(Map::new()))),
        Some(other) => Err(bad_arg(
            "apoc.convert.toMap",
            &format!("cannot convert {other} to MAP"),
        )),
    }
}

fn to_list_proc(args: Vec<Value>) -> Result<ApocResult> {
    let v = args.first().cloned().unwrap_or(Value::Null);
    let out = match v {
        Value::Array(a) => a,
        Value::Null => Vec::new(),
        other => vec![other],
    };
    Ok(ApocResult::scalar(Value::Array(out)))
}

fn to_string_proc(args: Vec<Value>) -> Result<ApocResult> {
    let v = args.first().cloned().unwrap_or(Value::Null);
    Ok(ApocResult::scalar(Value::String(display(&v))))
}

fn to_integer(args: Vec<Value>) -> Result<ApocResult> {
    let v = args.first().cloned().unwrap_or(Value::Null);
    let out = match v {
        Value::Null => Value::Null,
        Value::Bool(b) => Value::Number((b as i64).into()),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Number(i.into())
            } else if let Some(f) = n.as_f64() {
                Value::Number((f as i64).into())
            } else {
                Value::Null
            }
        }
        Value::String(s) => s
            .trim()
            .parse::<i64>()
            .map(|i| Value::Number(i.into()))
            .unwrap_or(Value::Null),
        _ => Value::Null,
    };
    Ok(ApocResult::scalar(out))
}

fn to_float(args: Vec<Value>) -> Result<ApocResult> {
    let v = args.first().cloned().unwrap_or(Value::Null);
    let out = match v {
        Value::Null => Value::Null,
        Value::Bool(b) => serde_json::Number::from_f64(if b { 1.0 } else { 0.0 })
            .map(Value::Number)
            .unwrap_or(Value::Null),
        Value::Number(n) => n
            .as_f64()
            .and_then(|f| serde_json::Number::from_f64(f).map(Value::Number))
            .unwrap_or(Value::Null),
        Value::String(s) => s
            .trim()
            .parse::<f64>()
            .ok()
            .and_then(|f| serde_json::Number::from_f64(f).map(Value::Number))
            .unwrap_or(Value::Null),
        _ => Value::Null,
    };
    Ok(ApocResult::scalar(out))
}

fn to_boolean(args: Vec<Value>) -> Result<ApocResult> {
    let v = args.first().cloned().unwrap_or(Value::Null);
    let out = match v {
        Value::Null => Value::Null,
        Value::Bool(b) => Value::Bool(b),
        Value::Number(n) => Value::Bool(n.as_f64().unwrap_or(0.0) != 0.0),
        Value::String(s) => match s.trim().to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" | "y" => Value::Bool(true),
            "false" | "0" | "no" | "n" | "" => Value::Bool(false),
            _ => Value::Null,
        },
        _ => Value::Null,
    };
    Ok(ApocResult::scalar(out))
}

fn to_typed_list<F>(args: Vec<Value>, map_elem: F) -> Result<ApocResult>
where
    F: Fn(&Value) -> Result<Value>,
{
    let xs = super::as_list(&args.first().cloned().unwrap_or(Value::Null));
    let mut out: Vec<Value> = Vec::with_capacity(xs.len());
    for v in &xs {
        out.push(map_elem(v)?);
    }
    Ok(ApocResult::scalar(Value::Array(out)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(proc: &str, args: Vec<Value>) -> Value {
        dispatch(proc, args).unwrap().rows[0][0].clone()
    }

    #[test]
    fn to_json_serialises() {
        assert_eq!(
            call("toJson", vec![json!({"a": 1, "b": [2, 3]})]),
            Value::String("{\"a\":1,\"b\":[2,3]}".to_string())
        );
    }

    #[test]
    fn from_json_map_parses() {
        assert_eq!(
            call("fromJsonMap", vec![json!(r#"{"a": 1}"#)]),
            json!({"a": 1})
        );
    }

    #[test]
    fn from_json_list_parses() {
        assert_eq!(
            call("fromJsonList", vec![json!("[1, 2, 3]")]),
            json!([1, 2, 3])
        );
    }

    #[test]
    fn from_json_map_rejects_array_root() {
        assert!(dispatch("fromJsonMap", vec![json!("[]")]).is_err());
    }

    #[test]
    fn to_integer_parses_string() {
        assert_eq!(call("toInteger", vec![json!("42")]), json!(42));
        assert_eq!(call("toInteger", vec![json!(3.7)]), json!(3));
        assert_eq!(call("toInteger", vec![json!("nope")]), Value::Null);
    }

    #[test]
    fn to_float_parses() {
        assert_eq!(call("toFloat", vec![json!("2.5")]), json!(2.5));
        assert_eq!(call("toFloat", vec![json!(2)]), json!(2.0));
    }

    #[test]
    fn to_boolean_recognises_yes_no() {
        assert_eq!(call("toBoolean", vec![json!("YES")]), json!(true));
        assert_eq!(call("toBoolean", vec![json!("no")]), json!(false));
        assert_eq!(call("toBoolean", vec![json!(1)]), json!(true));
        assert_eq!(call("toBoolean", vec![json!(0)]), json!(false));
    }

    #[test]
    fn to_string_null_yields_literal_null() {
        assert_eq!(call("toString", vec![json!(null)]), json!("null"));
    }

    #[test]
    fn to_int_list_rejects_non_numeric() {
        assert!(dispatch("toIntList", vec![json!([1, "x"])]).is_err());
    }

    #[test]
    fn to_map_from_pairs() {
        assert_eq!(
            call("toMap", vec![json!([["a", 1], ["b", 2]])]),
            json!({"a": 1, "b": 2})
        );
    }

    #[test]
    fn to_map_from_json_string() {
        assert_eq!(call("toMap", vec![json!(r#"{"a": 1}"#)]), json!({"a": 1}));
    }

    #[test]
    fn to_list_wraps_scalars() {
        assert_eq!(call("toList", vec![json!(7)]), json!([7]));
        assert_eq!(call("toList", vec![json!([1, 2])]), json!([1, 2]));
        assert_eq!(call("toList", vec![json!(null)]), json!([]));
    }
}
