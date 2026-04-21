//! `apoc.map.*` — map manipulation (phase6 apoc §3).

use super::{ApocResult, as_list, bad_arg, not_found};
use crate::{Error, Result};
use serde_json::{Map, Value, json};

pub fn list() -> &'static [&'static str] {
    &[
        "apoc.map.merge",
        "apoc.map.mergeList",
        "apoc.map.fromPairs",
        "apoc.map.fromLists",
        "apoc.map.fromValues",
        "apoc.map.setKey",
        "apoc.map.removeKey",
        "apoc.map.removeKeys",
        "apoc.map.clean",
        "apoc.map.flatten",
        "apoc.map.unflatten",
        "apoc.map.values",
        "apoc.map.fromNodes",
        "apoc.map.groupBy",
        "apoc.map.groupByMulti",
        "apoc.map.updateTree",
        "apoc.map.submap",
        "apoc.map.get",
        "apoc.map.getOrDefault",
        "apoc.map.fromEntries",
    ]
}

pub fn dispatch(proc: &str, args: Vec<Value>) -> Result<ApocResult> {
    match proc {
        "merge" => merge(args),
        "mergeList" => merge_list(args),
        "fromPairs" | "fromEntries" => from_pairs(args),
        "fromLists" => from_lists(args),
        "fromValues" => from_values(args),
        "setKey" => set_key(args),
        "removeKey" => remove_key(args),
        "removeKeys" => remove_keys(args),
        "clean" => clean(args),
        "flatten" => flatten(args),
        "unflatten" => unflatten(args),
        "values" => values(args),
        "fromNodes" => from_nodes(args),
        "groupBy" => group_by(args),
        "groupByMulti" => group_by_multi(args),
        "updateTree" => update_tree(args),
        "submap" => submap(args),
        "get" | "getOrDefault" => get(args),
        _ => Err(not_found(&format!("apoc.map.{proc}"))),
    }
}

fn as_map(v: &Value) -> Option<Map<String, Value>> {
    match v {
        Value::Object(m) => Some(m.clone()),
        Value::Null => Some(Map::new()),
        _ => None,
    }
}

fn require_map(proc: &str, v: &Value) -> Result<Map<String, Value>> {
    as_map(v).ok_or_else(|| bad_arg(proc, "expected a MAP"))
}

fn merge(args: Vec<Value>) -> Result<ApocResult> {
    let a = args
        .first()
        .map(|v| require_map("apoc.map.merge", v))
        .transpose()?
        .unwrap_or_default();
    let b = args
        .get(1)
        .map(|v| require_map("apoc.map.merge", v))
        .transpose()?
        .unwrap_or_default();
    let mut out = a;
    for (k, v) in b {
        out.insert(k, v);
    }
    Ok(ApocResult::scalar(Value::Object(out)))
}

fn merge_list(args: Vec<Value>) -> Result<ApocResult> {
    let xs = args.first().map(as_list).unwrap_or_default();
    let mut out = Map::new();
    for v in xs {
        let m = require_map("apoc.map.mergeList", &v)?;
        for (k, val) in m {
            out.insert(k, val);
        }
    }
    Ok(ApocResult::scalar(Value::Object(out)))
}

fn from_pairs(args: Vec<Value>) -> Result<ApocResult> {
    let xs = args.first().map(as_list).unwrap_or_default();
    let mut out = Map::new();
    for pair in xs {
        let arr = match pair {
            Value::Array(a) => a,
            _ => {
                return Err(bad_arg(
                    "apoc.map.fromPairs",
                    "pair must be a 2-element LIST",
                ));
            }
        };
        if arr.len() != 2 {
            return Err(bad_arg(
                "apoc.map.fromPairs",
                "pair must be a 2-element LIST",
            ));
        }
        let key = match &arr[0] {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        out.insert(key, arr[1].clone());
    }
    Ok(ApocResult::scalar(Value::Object(out)))
}

fn from_lists(args: Vec<Value>) -> Result<ApocResult> {
    let keys = args.first().map(as_list).unwrap_or_default();
    let values = args.get(1).map(as_list).unwrap_or_default();
    let mut out = Map::new();
    for (k, v) in keys.into_iter().zip(values) {
        let key = match k {
            Value::String(s) => s,
            other => other.to_string(),
        };
        out.insert(key, v);
    }
    Ok(ApocResult::scalar(Value::Object(out)))
}

fn from_values(args: Vec<Value>) -> Result<ApocResult> {
    // fromValues([k1, v1, k2, v2, ...])
    let xs = args.first().map(as_list).unwrap_or_default();
    if xs.len() % 2 != 0 {
        return Err(bad_arg(
            "apoc.map.fromValues",
            "expected an even-length LIST",
        ));
    }
    let mut out = Map::new();
    let mut iter = xs.into_iter();
    while let (Some(k), Some(v)) = (iter.next(), iter.next()) {
        let key = match k {
            Value::String(s) => s,
            other => other.to_string(),
        };
        out.insert(key, v);
    }
    Ok(ApocResult::scalar(Value::Object(out)))
}

fn set_key(args: Vec<Value>) -> Result<ApocResult> {
    let mut m = args
        .first()
        .map(|v| require_map("apoc.map.setKey", v))
        .transpose()?
        .unwrap_or_default();
    let key = args
        .get(1)
        .and_then(|v| v.as_str())
        .ok_or_else(|| bad_arg("apoc.map.setKey", "second arg must be a STRING key"))?
        .to_string();
    let value = args.get(2).cloned().unwrap_or(Value::Null);
    m.insert(key, value);
    Ok(ApocResult::scalar(Value::Object(m)))
}

fn remove_key(args: Vec<Value>) -> Result<ApocResult> {
    let mut m = args
        .first()
        .map(|v| require_map("apoc.map.removeKey", v))
        .transpose()?
        .unwrap_or_default();
    let key = args
        .get(1)
        .and_then(|v| v.as_str())
        .ok_or_else(|| bad_arg("apoc.map.removeKey", "second arg must be a STRING key"))?;
    m.remove(key);
    Ok(ApocResult::scalar(Value::Object(m)))
}

fn remove_keys(args: Vec<Value>) -> Result<ApocResult> {
    let mut m = args
        .first()
        .map(|v| require_map("apoc.map.removeKeys", v))
        .transpose()?
        .unwrap_or_default();
    let keys = args.get(1).map(as_list).unwrap_or_default();
    for k in keys {
        if let Value::String(s) = k {
            m.remove(&s);
        }
    }
    Ok(ApocResult::scalar(Value::Object(m)))
}

fn clean(args: Vec<Value>) -> Result<ApocResult> {
    // clean(map, removeKeys, removeValues)
    let m = args
        .first()
        .map(|v| require_map("apoc.map.clean", v))
        .transpose()?
        .unwrap_or_default();
    let remove_keys: Vec<String> = args
        .get(1)
        .map(as_list)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|v| match v {
            Value::String(s) => Some(s),
            _ => None,
        })
        .collect();
    let remove_values = args.get(2).map(as_list).unwrap_or_default();
    let mut out = Map::new();
    for (k, v) in m {
        if remove_keys.iter().any(|rk| rk == &k) {
            continue;
        }
        if remove_values.iter().any(|rv| rv == &v) {
            continue;
        }
        out.insert(k, v);
    }
    Ok(ApocResult::scalar(Value::Object(out)))
}

fn flatten(args: Vec<Value>) -> Result<ApocResult> {
    let m = args
        .first()
        .map(|v| require_map("apoc.map.flatten", v))
        .transpose()?
        .unwrap_or_default();
    let delim = args
        .get(1)
        .and_then(|v| v.as_str())
        .unwrap_or(".")
        .to_string();
    let mut out = Map::new();
    flatten_into(&m, String::new(), &delim, &mut out);
    Ok(ApocResult::scalar(Value::Object(out)))
}

fn flatten_into(m: &Map<String, Value>, prefix: String, delim: &str, out: &mut Map<String, Value>) {
    for (k, v) in m {
        let key = if prefix.is_empty() {
            k.clone()
        } else {
            format!("{prefix}{delim}{k}")
        };
        match v {
            Value::Object(inner) => flatten_into(inner, key, delim, out),
            other => {
                out.insert(key, other.clone());
            }
        }
    }
}

fn unflatten(args: Vec<Value>) -> Result<ApocResult> {
    let m = args
        .first()
        .map(|v| require_map("apoc.map.unflatten", v))
        .transpose()?
        .unwrap_or_default();
    let delim = args
        .get(1)
        .and_then(|v| v.as_str())
        .unwrap_or(".")
        .to_string();
    let mut root = Map::new();
    for (flat_key, v) in m {
        let parts: Vec<&str> = flat_key.split(delim.as_str()).collect();
        insert_nested(&mut root, &parts, v);
    }
    Ok(ApocResult::scalar(Value::Object(root)))
}

fn insert_nested(m: &mut Map<String, Value>, path: &[&str], v: Value) {
    if path.is_empty() {
        return;
    }
    if path.len() == 1 {
        m.insert(path[0].to_string(), v);
        return;
    }
    let head = path[0];
    let tail = &path[1..];
    let entry = m
        .entry(head.to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    if let Value::Object(inner) = entry {
        insert_nested(inner, tail, v);
    }
}

fn values(args: Vec<Value>) -> Result<ApocResult> {
    let m = args
        .first()
        .map(|v| require_map("apoc.map.values", v))
        .transpose()?
        .unwrap_or_default();
    let keys = args.get(1).map(as_list);
    let out: Vec<Value> = match keys {
        Some(ks) => ks
            .into_iter()
            .filter_map(|k| match k {
                Value::String(s) => m.get(&s).cloned(),
                _ => None,
            })
            .collect(),
        None => m.values().cloned().collect(),
    };
    Ok(ApocResult::scalar(Value::Array(out)))
}

fn from_nodes(_args: Vec<Value>) -> Result<ApocResult> {
    // Nodes don't round-trip through the JSON runtime without engine
    // access, and this procedure's real contract is
    // `apoc.map.fromNodes(label, keyProperty)` which needs a live
    // engine context. Emit the same `ERR_PROC_NOT_FOUND`-equivalent
    // as other engine-dependent procedures rather than silently
    // returning empty.
    Err(Error::CypherExecution(
        "apoc.map.fromNodes: requires engine context — call from the engine procedure dispatcher"
            .to_string(),
    ))
}

fn group_by(args: Vec<Value>) -> Result<ApocResult> {
    let xs = args.first().map(as_list).unwrap_or_default();
    let key_name = args
        .get(1)
        .and_then(|v| v.as_str())
        .ok_or_else(|| bad_arg("apoc.map.groupBy", "second arg must be a STRING key"))?
        .to_string();
    let mut out = Map::new();
    for item in xs {
        if let Value::Object(m) = &item {
            if let Some(k) = m.get(&key_name) {
                let key = match k {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                out.insert(key, item);
            }
        }
    }
    Ok(ApocResult::scalar(Value::Object(out)))
}

fn group_by_multi(args: Vec<Value>) -> Result<ApocResult> {
    let xs = args.first().map(as_list).unwrap_or_default();
    let key_name = args
        .get(1)
        .and_then(|v| v.as_str())
        .ok_or_else(|| bad_arg("apoc.map.groupByMulti", "second arg must be a STRING key"))?
        .to_string();
    let mut out: Map<String, Value> = Map::new();
    for item in xs {
        if let Value::Object(m) = &item {
            if let Some(k) = m.get(&key_name) {
                let key = match k {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                out.entry(key)
                    .and_modify(|e| {
                        if let Value::Array(list) = e {
                            list.push(item.clone());
                        }
                    })
                    .or_insert_with(|| Value::Array(vec![item.clone()]));
            }
        }
    }
    Ok(ApocResult::scalar(Value::Object(out)))
}

fn update_tree(args: Vec<Value>) -> Result<ApocResult> {
    // updateTree(tree, pathKey, updates[])
    let mut tree = args
        .first()
        .map(|v| require_map("apoc.map.updateTree", v))
        .transpose()?
        .unwrap_or_default();
    let path_key = args
        .get(1)
        .and_then(|v| v.as_str())
        .unwrap_or("id")
        .to_string();
    let updates = args.get(2).map(as_list).unwrap_or_default();
    for upd in updates {
        if let Value::Object(mut m) = upd {
            if let Some(id) = m.remove(&path_key) {
                let key = match id {
                    Value::String(s) => s,
                    other => other.to_string(),
                };
                // Merge into tree[key].
                let existing = tree.get(&key).cloned().unwrap_or(Value::Object(Map::new()));
                if let Value::Object(mut existing_map) = existing {
                    for (k, v) in m {
                        existing_map.insert(k, v);
                    }
                    tree.insert(key, Value::Object(existing_map));
                }
            }
        }
    }
    Ok(ApocResult::scalar(Value::Object(tree)))
}

fn submap(args: Vec<Value>) -> Result<ApocResult> {
    let m = args
        .first()
        .map(|v| require_map("apoc.map.submap", v))
        .transpose()?
        .unwrap_or_default();
    let keys: Vec<String> = args
        .get(1)
        .map(as_list)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|v| match v {
            Value::String(s) => Some(s),
            _ => None,
        })
        .collect();
    let mut out = Map::new();
    for k in keys {
        if let Some(v) = m.get(&k) {
            out.insert(k, v.clone());
        }
    }
    Ok(ApocResult::scalar(Value::Object(out)))
}

fn get(args: Vec<Value>) -> Result<ApocResult> {
    let m = args
        .first()
        .map(|v| require_map("apoc.map.get", v))
        .transpose()?
        .unwrap_or_default();
    let key = args
        .get(1)
        .and_then(|v| v.as_str())
        .ok_or_else(|| bad_arg("apoc.map.get", "second arg must be a STRING key"))?;
    let default = args.get(2).cloned().unwrap_or(Value::Null);
    Ok(ApocResult::scalar(m.get(key).cloned().unwrap_or(default)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(proc: &str, args: Vec<Value>) -> Value {
        dispatch(proc, args).unwrap().rows[0][0].clone()
    }

    #[test]
    fn merge_overwrites_right_wins() {
        assert_eq!(
            call(
                "merge",
                vec![json!({"a": 1, "b": 2}), json!({"b": 3, "c": 4})]
            ),
            json!({"a": 1, "b": 3, "c": 4})
        );
    }

    #[test]
    fn merge_list_folds_multiple() {
        assert_eq!(
            call("mergeList", vec![json!([{"a": 1}, {"b": 2}, {"a": 9}])]),
            json!({"a": 9, "b": 2})
        );
    }

    #[test]
    fn from_pairs_typical() {
        assert_eq!(
            call("fromPairs", vec![json!([["a", 1], ["b", 2]])]),
            json!({"a": 1, "b": 2})
        );
    }

    #[test]
    fn from_pairs_bad_pair_rejected() {
        let err = dispatch("fromPairs", vec![json!([["a"]])]).unwrap_err();
        assert!(err.to_string().contains("ERR_INVALID_ARG_VALUE"));
    }

    #[test]
    fn from_lists_pairs_by_position() {
        assert_eq!(
            call("fromLists", vec![json!(["a", "b"]), json!([1, 2])]),
            json!({"a": 1, "b": 2})
        );
    }

    #[test]
    fn from_values_even_length_only() {
        assert_eq!(
            call("fromValues", vec![json!(["a", 1, "b", 2])]),
            json!({"a": 1, "b": 2})
        );
        assert!(dispatch("fromValues", vec![json!(["a", 1, "b"])]).is_err());
    }

    #[test]
    fn set_key_overwrites() {
        assert_eq!(
            call("setKey", vec![json!({"a": 1}), json!("b"), json!(2)]),
            json!({"a": 1, "b": 2})
        );
    }

    #[test]
    fn remove_key_drops_entry() {
        assert_eq!(
            call("removeKey", vec![json!({"a": 1, "b": 2}), json!("a")]),
            json!({"b": 2})
        );
    }

    #[test]
    fn remove_keys_multi() {
        assert_eq!(
            call(
                "removeKeys",
                vec![json!({"a": 1, "b": 2, "c": 3}), json!(["a", "b"])]
            ),
            json!({"c": 3})
        );
    }

    #[test]
    fn clean_drops_null_and_sentinel() {
        assert_eq!(
            call(
                "clean",
                vec![
                    json!({"a": 1, "b": null, "c": "drop"}),
                    json!([]),
                    json!([null, "drop"])
                ]
            ),
            json!({"a": 1})
        );
    }

    #[test]
    fn flatten_nested_to_dotted() {
        assert_eq!(
            call("flatten", vec![json!({"a": {"b": {"c": 1}}, "d": 2})]),
            json!({"a.b.c": 1, "d": 2})
        );
    }

    #[test]
    fn unflatten_reverses_flatten() {
        let flat = call("flatten", vec![json!({"a": {"b": 1}, "c": 2})]);
        assert_eq!(
            call("unflatten", vec![flat]),
            json!({"a": {"b": 1}, "c": 2})
        );
    }

    #[test]
    fn values_returns_map_values() {
        let out = call("values", vec![json!({"a": 1, "b": 2})]);
        let mut arr: Vec<i64> = out
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_i64().unwrap())
            .collect();
        arr.sort_unstable();
        assert_eq!(arr, vec![1, 2]);
    }

    #[test]
    fn values_with_key_list_preserves_order() {
        assert_eq!(
            call(
                "values",
                vec![json!({"a": 1, "b": 2, "c": 3}), json!(["b", "a"])]
            ),
            json!([2, 1])
        );
    }

    #[test]
    fn group_by_last_wins() {
        assert_eq!(
            call(
                "groupBy",
                vec![
                    json!([{"g": "a", "v": 1}, {"g": "b", "v": 2}, {"g": "a", "v": 3}]),
                    json!("g")
                ]
            ),
            json!({"a": {"g": "a", "v": 3}, "b": {"g": "b", "v": 2}})
        );
    }

    #[test]
    fn group_by_multi_collects_all() {
        let out = call(
            "groupByMulti",
            vec![json!([{"g": "a", "v": 1}, {"g": "a", "v": 2}]), json!("g")],
        );
        assert_eq!(out, json!({"a": [{"g": "a", "v": 1}, {"g": "a", "v": 2}]}));
    }

    #[test]
    fn submap_extracts_selected_keys() {
        assert_eq!(
            call(
                "submap",
                vec![json!({"a": 1, "b": 2, "c": 3}), json!(["a", "c"])]
            ),
            json!({"a": 1, "c": 3})
        );
    }

    #[test]
    fn get_returns_default_when_missing() {
        assert_eq!(
            call("get", vec![json!({"a": 1}), json!("b"), json!(99)]),
            json!(99)
        );
    }
}
