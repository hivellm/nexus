//! Byte-array (`BYTES`) helpers for the Cypher executor.
//!
//! The Nexus runtime stores every value in `serde_json::Value`, so a native
//! `BYTES` type is represented by the single-key object
//! `{"_bytes": "<base64>"}`. This module owns the predicate/extractor/
//! constructor trio used across the `bytes`, `bytesFromBase64`,
//! `bytesToBase64`, `bytesToHex`, `bytesLength`, and `bytesSlice`
//! function implementations in `projection.rs`.
//!
//! Kept out of `helpers.rs` because it is self-contained and cheap to
//! unit-test in isolation.

use crate::{Error, Result};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use serde_json::{Map, Value};

/// 64 MiB cap per-property (matches spec §types-bytes "size-limit enforced").
pub(crate) const MAX_BYTES_PER_PROPERTY: usize = 64 * 1024 * 1024;

/// Rejects a base64 payload whose ENCODED length already guarantees a
/// decoded size over [`MAX_BYTES_PER_PROPERTY`], before any decode
/// buffer is allocated.
///
/// Base64 encodes 3 raw bytes as 4 characters, so `decoded_len <=
/// floor(encoded_len * 3 / 4)`. Every `B64.decode` call site in this
/// module previously decoded first and only checked the size of the
/// RESULT (via [`bytes_from_vec`]) — or, for [`bytes_value_to_vec`],
/// never checked at all — so a caller-supplied base64 string of
/// several GB made the decoder allocate a several-GB `Vec<u8>` before
/// any cap could reject it. Checking the cheap `s.len()` upfront (an
/// `O(1)` string-length read, no allocation) closes that gap: this
/// runs BEFORE `B64.decode`, not after.
pub(crate) fn reject_oversize_base64(s: &str) -> Result<()> {
    // Inverse of the encode-size formula with headroom for padding
    // (`+4`): any encoded string longer than this cannot possibly
    // decode to <= `MAX_BYTES_PER_PROPERTY` raw bytes.
    let max_encoded_len = MAX_BYTES_PER_PROPERTY * 4 / 3 + 4;
    if s.len() > max_encoded_len {
        return Err(Error::CypherExecution(format!(
            "ERR_BYTES_TOO_LARGE: base64 payload of {} characters exceeds the \
             {max_encoded_len}-character encoded-length cap (decodes to more than the \
             {MAX_BYTES_PER_PROPERTY}-byte per-property limit)",
            s.len()
        )));
    }
    Ok(())
}

/// True iff `v` is the `{"_bytes": "<base64>"}` wire shape that the
/// Nexus runtime treats as a BYTES scalar.
pub(crate) fn is_bytes_value(v: &Value) -> bool {
    if let Value::Object(map) = v {
        if map.len() == 1 {
            if let Some(Value::String(_)) = map.get("_bytes") {
                return true;
            }
        }
    }
    false
}

/// Decode a `{"_bytes": "<base64>"}` value into its raw byte slice.
///
/// Errors on a malformed base64 payload. Callers that want to propagate
/// NULL on NULL input should check with [`is_bytes_value`] or handle
/// `Value::Null` before calling.
pub(crate) fn bytes_value_to_vec(v: &Value) -> Result<Vec<u8>> {
    let s = match v {
        Value::Object(map) => {
            map.get("_bytes")
                .and_then(|x| x.as_str())
                .ok_or_else(|| Error::TypeMismatch {
                    expected: "BYTES".to_string(),
                    actual: "MAP".to_string(),
                })?
        }
        other => {
            return Err(Error::TypeMismatch {
                expected: "BYTES".to_string(),
                actual: match other {
                    Value::Null => "NULL",
                    Value::Bool(_) => "BOOLEAN",
                    Value::Number(_) => "NUMBER",
                    Value::String(_) => "STRING",
                    Value::Array(_) => "LIST",
                    Value::Object(_) => "MAP",
                }
                .to_string(),
            });
        }
    };
    reject_oversize_base64(s)?;
    B64.decode(s).map_err(|e| {
        Error::CypherExecution(format!("ERR_INVALID_BYTES: base64 decode failed: {e}"))
    })
}

/// Build a `{"_bytes": "<base64>"}` value from raw bytes with the
/// 64 MiB per-property cap enforced.
pub(crate) fn bytes_from_vec(raw: Vec<u8>) -> Result<Value> {
    if raw.len() > MAX_BYTES_PER_PROPERTY {
        return Err(Error::CypherExecution(format!(
            "ERR_BYTES_TOO_LARGE: {} bytes exceeds {}-byte per-property cap",
            raw.len(),
            MAX_BYTES_PER_PROPERTY
        )));
    }
    let encoded = B64.encode(&raw);
    let mut map = Map::with_capacity(1);
    map.insert("_bytes".to_string(), Value::String(encoded));
    Ok(Value::Object(map))
}

/// Parameter-side coercion: accept the canonical object shape, or a
/// bare base64 STRING when the caller has declared the parameter as
/// `BYTES` via the `bytes_params` hint. Rejects anything else with
/// `ERR_INVALID_BYTES`.
pub(crate) fn coerce_param_to_bytes(v: &Value) -> Result<Value> {
    if is_bytes_value(v) {
        return Ok(v.clone());
    }
    if let Value::String(s) = v {
        reject_oversize_base64(s)?;
        let raw = B64.decode(s).map_err(|e| {
            Error::CypherExecution(format!(
                "ERR_INVALID_BYTES: parameter is not valid base64: {e}"
            ))
        })?;
        return bytes_from_vec(raw);
    }
    Err(Error::CypherExecution(
        "ERR_INVALID_BYTES: expected {_bytes: string} or base64 STRING".to_string(),
    ))
}

/// Hex encoder used by `bytesToHex`. Lowercase, no separators — matches
/// the Neo4j `apoc.util.md5` convention reused for BYTES.
pub(crate) fn to_hex(raw: &[u8]) -> String {
    let mut out = String::with_capacity(raw.len() * 2);
    for b in raw {
        out.push(char::from_digit((b >> 4) as u32, 16).unwrap_or('0'));
        out.push(char::from_digit((b & 0x0f) as u32, 16).unwrap_or('0'));
    }
    out
}

/// `bytesSlice(b, start, len)` semantics: clamp `start` into `[0, L]`,
/// clamp `start + len` into `[start, L]`, where `L = raw.len()`. Matches
/// the Cypher `substring` clamping rules so users see familiar
/// behaviour across STRING and BYTES.
pub(crate) fn slice(raw: &[u8], start: i64, len: i64) -> Vec<u8> {
    let total = raw.len() as i64;
    let start = start.clamp(0, total) as usize;
    let len = len.max(0);
    let end = ((start as i64).saturating_add(len)).min(total) as usize;
    raw[start..end].to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn is_bytes_value_accepts_canonical_shape() {
        assert!(is_bytes_value(&json!({"_bytes": "AAH/"})));
    }

    #[test]
    fn is_bytes_value_rejects_extra_keys() {
        assert!(!is_bytes_value(&json!({"_bytes": "AAH/", "x": 1})));
        assert!(!is_bytes_value(&json!({"other": "AAH/"})));
        assert!(!is_bytes_value(&json!([1, 2, 3])));
    }

    #[test]
    fn bytes_from_vec_roundtrips_through_bytes_value_to_vec() {
        let raw = vec![0x00u8, 0x01, 0xff, 0x20];
        let wire = bytes_from_vec(raw.clone()).unwrap();
        assert_eq!(bytes_value_to_vec(&wire).unwrap(), raw);
    }

    #[test]
    fn bytes_from_vec_rejects_oversize_payload() {
        let too_big = vec![0u8; MAX_BYTES_PER_PROPERTY + 1];
        let err = bytes_from_vec(too_big).unwrap_err();
        assert!(format!("{err}").contains("ERR_BYTES_TOO_LARGE"));
    }

    #[test]
    fn reject_oversize_base64_rejects_before_decode_and_accepts_valid() {
        // A base64 string longer than the encoded cap must be rejected on
        // its length alone — BEFORE any decode buffer is allocated.
        let over_cap = "A".repeat(MAX_BYTES_PER_PROPERTY * 4 / 3 + 5);
        let err = reject_oversize_base64(&over_cap).unwrap_err();
        assert!(
            format!("{err}").contains("ERR_BYTES_TOO_LARGE"),
            "over-cap base64 must be rejected on encoded length; got {err}"
        );
        // A normal, in-bounds base64 payload passes the pre-check.
        reject_oversize_base64("SGVsbG8gd29ybGQ=").expect("valid small base64 must pass");
    }

    #[test]
    fn coerce_param_accepts_base64_string() {
        let p = coerce_param_to_bytes(&json!("AAH/")).unwrap();
        assert_eq!(bytes_value_to_vec(&p).unwrap(), vec![0x00, 0x01, 0xff]);
    }

    #[test]
    fn coerce_param_rejects_number() {
        assert!(coerce_param_to_bytes(&json!(42)).is_err());
    }

    #[test]
    fn to_hex_matches_spec_example() {
        assert_eq!(to_hex(&[0x61, 0x62, 0x63]), "616263");
    }

    #[test]
    fn slice_clamps_bounds() {
        let raw = vec![0, 1, 2, 3, 4];
        assert_eq!(slice(&raw, 1, 3), vec![1, 2, 3]);
        assert_eq!(slice(&raw, 0, 10), raw.clone());
        assert_eq!(slice(&raw, 10, 1), Vec::<u8>::new());
        assert_eq!(slice(&raw, -1, 2), vec![0, 1]);
        assert_eq!(slice(&raw, 2, -1), Vec::<u8>::new());
    }
}
