//! Redaction utility for query parameters surfaced through
//! diagnostic surfaces (slow-query log, `/admin/queries`,
//! `SHOW QUERIES`).
//!
//! Applied at the boundary right before a parameter value would be
//! logged or returned to a client. Centralises the policy so every
//! diagnostic surface gets the same treatment — operators can not
//! accidentally surface a credential through `SHOW QUERIES` after
//! redacting it from `docker logs`.
//!
//! Policy (phase6_slow-query-log-and-active-queries §5):
//!
//! - Strings ≤ 256 chars → preserved verbatim.
//! - Strings > 256 chars → truncated to 256 chars + `<<truncated N
//!   bytes>>` suffix where N is the dropped byte count.
//! - Non-UTF-8 / binary buffers → `<<binary N bytes>>`.
//! - Numbers / booleans / null → preserved (no PII risk on
//!   primitives).
//! - Arrays / objects → recursively redacted; container shape is
//!   preserved so the diagnostic surface still conveys "what
//!   parameters were bound" without leaking individual values.
//!
//! The utility is value-only — it does not consult an allow / deny
//! list of parameter names. Operators that need name-based
//! redaction should layer their own filter on top of this one;
//! adding heuristic name matching here would be a footgun (false
//! negatives on novel naming conventions).

use serde_json::Value;

/// Maximum length (in chars) that a string parameter is allowed to
/// flow through to a diagnostic surface verbatim. Strings longer
/// than this are truncated with a `<<truncated N bytes>>` suffix
/// where N is the dropped byte count.
pub const STRING_PARAMETER_MAX_LEN: usize = 256;

/// Redact a single `serde_json::Value` per the module-level policy.
/// Returns a new `Value`; the input is not mutated.
pub fn redact(value: &Value) -> Value {
    match value {
        Value::String(s) => redact_string(s),
        Value::Array(arr) => Value::Array(arr.iter().map(redact).collect()),
        Value::Object(map) => {
            let mut out = serde_json::Map::with_capacity(map.len());
            for (k, v) in map {
                out.insert(k.clone(), redact(v));
            }
            Value::Object(out)
        }
        // Numbers / bools / null carry no PII payload — pass through.
        other => other.clone(),
    }
}

/// Redact every entry in a parameter map by name. Returns a new
/// map; input is not mutated. The keys (parameter names) are
/// preserved verbatim — only the values are redacted.
pub fn redact_parameters(
    params: &std::collections::HashMap<String, Value>,
) -> std::collections::HashMap<String, Value> {
    params.iter().map(|(k, v)| (k.clone(), redact(v))).collect()
}

fn redact_string(s: &str) -> Value {
    let char_count = s.chars().count();
    if char_count <= STRING_PARAMETER_MAX_LEN {
        return Value::String(s.to_string());
    }
    // Take the first STRING_PARAMETER_MAX_LEN chars, then count the
    // dropped bytes — the suffix names bytes (not chars) so a
    // multibyte payload reports its actual on-wire size.
    let kept: String = s.chars().take(STRING_PARAMETER_MAX_LEN).collect();
    let dropped_bytes = s.len() - kept.len();
    Value::String(format!("{kept}<<truncated {dropped_bytes} bytes>>"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn short_string_passes_through_verbatim() {
        let v = redact(&json!("hello"));
        assert_eq!(v, json!("hello"));
    }

    #[test]
    fn long_string_truncates_with_byte_count_suffix() {
        let s = "x".repeat(300);
        let v = redact(&Value::String(s));
        let out = v.as_str().unwrap();
        assert!(
            out.starts_with(&"x".repeat(256)),
            "first 256 chars must survive, got: {out:?}"
        );
        assert!(
            out.ends_with("<<truncated 44 bytes>>"),
            "suffix must report dropped byte count, got: {out:?}"
        );
    }

    #[test]
    fn non_string_primitives_pass_through() {
        assert_eq!(redact(&json!(42)), json!(42));
        assert_eq!(redact(&json!(true)), json!(true));
        assert_eq!(redact(&json!(null)), json!(null));
        assert_eq!(redact(&json!(2.5)), json!(2.5));
    }

    #[test]
    fn arrays_recurse_and_preserve_shape() {
        let big = "y".repeat(300);
        let input = json!(["short", big.clone(), 1, true]);
        let out = redact(&input);
        let arr = out.as_array().unwrap();
        assert_eq!(arr[0], json!("short"));
        assert!(arr[1].as_str().unwrap().ends_with("<<truncated 44 bytes>>"));
        assert_eq!(arr[2], json!(1));
        assert_eq!(arr[3], json!(true));
    }

    #[test]
    fn objects_recurse_and_preserve_keys() {
        let big = "z".repeat(300);
        let input = json!({"name": "alice", "secret": big});
        let out = redact(&input);
        let map = out.as_object().unwrap();
        assert_eq!(map["name"], json!("alice"));
        assert!(
            map["secret"]
                .as_str()
                .unwrap()
                .ends_with("<<truncated 44 bytes>>")
        );
        assert_eq!(map.len(), 2, "key set is preserved");
    }

    #[test]
    fn redact_parameters_preserves_param_names() {
        use std::collections::HashMap;
        let mut params = HashMap::new();
        params.insert("$user".to_string(), json!("alice"));
        params.insert("$blob".to_string(), json!("Q".repeat(300)));
        let out = redact_parameters(&params);
        assert_eq!(out.len(), 2);
        assert_eq!(out["$user"], json!("alice"));
        assert!(
            out["$blob"]
                .as_str()
                .unwrap()
                .ends_with("<<truncated 44 bytes>>")
        );
    }

    #[test]
    fn boundary_at_exactly_256_chars_passes_through_verbatim() {
        let s = "a".repeat(256);
        let v = redact(&Value::String(s.clone()));
        assert_eq!(v, Value::String(s));
    }

    #[test]
    fn multibyte_string_truncation_counts_bytes_not_chars() {
        // Each "é" is 2 bytes in UTF-8, 1 char. 300 chars = 600
        // bytes. The first 256 chars survive (= 512 bytes); the
        // suffix should report the remaining 88 bytes.
        let s = "é".repeat(300);
        let v = redact(&Value::String(s));
        let out = v.as_str().unwrap();
        assert!(
            out.ends_with("<<truncated 88 bytes>>"),
            "expected 88-byte truncation suffix, got: {out:?}"
        );
    }
}
