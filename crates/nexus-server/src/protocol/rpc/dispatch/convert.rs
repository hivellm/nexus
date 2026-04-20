//! Bidirectional conversion between [`NexusValue`] and `serde_json::Value`.
//!
//! Every dispatch handler that crosses the boundary between the binary
//! wire type and the engine's internal `serde_json` representation goes
//! through these helpers so error shapes and UTF-8 / finite-number checks
//! stay consistent.

use crate::protocol::rpc::NexusValue;

/// Convert a [`NexusValue`] into the corresponding `serde_json::Value`.
///
/// Bytes are interpreted as UTF-8 text; non-UTF-8 bytes are rejected so a
/// value can never silently turn into an encoding-dependent string.
/// Non-finite [`f64`] values (NaN, +/-inf) are rejected because JSON has
/// no spelling for them.
pub fn nexus_to_json(value: NexusValue) -> Result<serde_json::Value, String> {
    match value {
        NexusValue::Null => Ok(serde_json::Value::Null),
        NexusValue::Bool(b) => Ok(serde_json::Value::Bool(b)),
        NexusValue::Int(i) => Ok(serde_json::Value::Number(i.into())),
        NexusValue::Float(f) => {
            let n = serde_json::Number::from_f64(f)
                .ok_or_else(|| "ERR non-finite Float cannot be represented in JSON".to_string())?;
            Ok(serde_json::Value::Number(n))
        }
        NexusValue::Bytes(b) => {
            let s = String::from_utf8(b)
                .map_err(|_| "ERR Bytes value must be valid UTF-8".to_string())?;
            Ok(serde_json::Value::String(s))
        }
        NexusValue::Str(s) => Ok(serde_json::Value::String(s)),
        NexusValue::Array(items) => items
            .into_iter()
            .map(nexus_to_json)
            .collect::<Result<Vec<_>, _>>()
            .map(serde_json::Value::Array),
        NexusValue::Map(pairs) => {
            let mut map = serde_json::Map::with_capacity(pairs.len());
            for (k, v) in pairs {
                let key = k
                    .as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| "ERR map keys must be strings".to_string())?;
                map.insert(key, nexus_to_json(v)?);
            }
            Ok(serde_json::Value::Object(map))
        }
    }
}

/// Convert a `serde_json::Value` into the matching [`NexusValue`].
/// Integer-fitting numbers become [`NexusValue::Int`]; the rest of the
/// variants map 1:1. Numbers larger than `i64::MAX` survive as strings
/// so no precision is silently lost.
pub fn json_to_nexus(value: serde_json::Value) -> NexusValue {
    match value {
        serde_json::Value::Null => NexusValue::Null,
        serde_json::Value::Bool(b) => NexusValue::Bool(b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                NexusValue::Int(i)
            } else if n.as_u64().is_some() {
                // u64 > i64::MAX — preserve precision as a string rather
                // than lossily widening to f64.
                NexusValue::Str(n.to_string())
            } else if let Some(f) = n.as_f64() {
                NexusValue::Float(f)
            } else {
                NexusValue::Str(n.to_string())
            }
        }
        serde_json::Value::String(s) => NexusValue::Str(s),
        serde_json::Value::Array(items) => {
            NexusValue::Array(items.into_iter().map(json_to_nexus).collect())
        }
        serde_json::Value::Object(obj) => NexusValue::Map(
            obj.into_iter()
                .map(|(k, v)| (NexusValue::Str(k), json_to_nexus(v)))
                .collect(),
        ),
    }
}

/// Convert a map argument (slice of `(key, value)` pairs) into a
/// `serde_json::Value::Object`. Keys must be strings; values round-trip
/// through [`nexus_to_json`].
pub fn pairs_to_json_object(
    pairs: &[(NexusValue, NexusValue)],
) -> Result<serde_json::Value, String> {
    let mut map = serde_json::Map::with_capacity(pairs.len());
    for (k, v) in pairs {
        let key = k
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| "ERR map keys must be strings".to_string())?;
        map.insert(key, nexus_to_json(v.clone())?);
    }
    Ok(serde_json::Value::Object(map))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nexus_to_json_scalars_round_trip() {
        assert_eq!(
            nexus_to_json(NexusValue::Null).unwrap(),
            serde_json::Value::Null
        );
        assert_eq!(
            nexus_to_json(NexusValue::Bool(false)).unwrap(),
            serde_json::Value::Bool(false)
        );
        assert_eq!(
            nexus_to_json(NexusValue::Int(-11)).unwrap(),
            serde_json::json!(-11)
        );
        assert_eq!(
            nexus_to_json(NexusValue::Float(3.25)).unwrap(),
            serde_json::json!(3.25)
        );
        assert_eq!(
            nexus_to_json(NexusValue::Str("abc".into())).unwrap(),
            serde_json::json!("abc")
        );
        assert_eq!(
            nexus_to_json(NexusValue::Bytes(b"xyz".to_vec())).unwrap(),
            serde_json::json!("xyz")
        );
    }

    #[test]
    fn nexus_to_json_non_finite_float_rejected() {
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let err = nexus_to_json(NexusValue::Float(bad)).unwrap_err();
            assert!(err.contains("non-finite"), "got: {err}");
        }
    }

    #[test]
    fn nexus_to_json_bytes_not_utf8_rejected() {
        let err = nexus_to_json(NexusValue::Bytes(vec![0xFF, 0xFE])).unwrap_err();
        assert!(err.contains("UTF-8"));
    }

    #[test]
    fn nested_round_trip() {
        let v = NexusValue::Map(vec![(
            NexusValue::Str("arr".into()),
            NexusValue::Array(vec![
                NexusValue::Int(1),
                NexusValue::Str("two".into()),
                NexusValue::Null,
            ]),
        )]);
        let as_json = nexus_to_json(v.clone()).unwrap();
        assert_eq!(as_json, serde_json::json!({ "arr": [1, "two", null] }));
        let back = json_to_nexus(as_json);
        assert_eq!(back, v);
    }

    #[test]
    fn large_integer_survives_as_string() {
        let big = serde_json::json!(u64::MAX);
        match json_to_nexus(big) {
            NexusValue::Str(s) => assert_eq!(s, u64::MAX.to_string()),
            other => panic!("expected Str fallback, got {other:?}"),
        }
    }

    #[test]
    fn pairs_to_json_object_maps_keys_and_values() {
        let pairs = vec![
            (NexusValue::Str("k".into()), NexusValue::Int(1)),
            (NexusValue::Str("s".into()), NexusValue::Str("v".into())),
        ];
        let obj = pairs_to_json_object(&pairs).unwrap();
        assert_eq!(obj, serde_json::json!({ "k": 1, "s": "v" }));
    }

    #[test]
    fn pairs_to_json_object_rejects_non_string_key() {
        let pairs = vec![(NexusValue::Int(1), NexusValue::Int(1))];
        let err = pairs_to_json_object(&pairs).unwrap_err();
        assert!(err.contains("keys must be strings"));
    }
}
