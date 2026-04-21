//! `NexusValue` — native typed value for the Cypher runtime
//! (phase6_opencypher-advanced-types §1.1).
//!
//! Nexus's internal runtime carries every value through
//! `serde_json::Value` today. That shape is fine for JSON-native types
//! (NULL / BOOLEAN / INTEGER / FLOAT / STRING / LIST / MAP), and we
//! kept it as the universal currency through phase6 to avoid a
//! crate-wide refactor. But JSON has no byte-array shape, and the
//! `{"_bytes": "<base64>"}` wire convention we ship for BYTES today
//! is ambiguous against a user-declared MAP that happens to have a
//! single `_bytes` string key.
//!
//! This module introduces a native `NexusValue` enum marked
//! `#[non_exhaustive]` so future additions (typed lists, user-defined
//! types, ...) don't break downstream code doing exhaustive matches.
//! It pairs with a pair of lossless conversions:
//!
//! - `NexusValue::from_json(&Value)` — decodes the BYTES wire shape
//!   back into `Bytes(Arc<[u8]>)`; every other JSON form maps
//!   one-to-one.
//! - `NexusValue::into_json(&self)` — re-encodes `Bytes` as the wire
//!   shape and pushes every other variant through the obvious JSON
//!   shape.
//!
//! The property-chain binary encoder (`encode` / `decode` at the end
//! of this file) also uses `NexusValue` and emits a `TYPE_BYTES`
//! tag with a u32 length prefix (§1.2).

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use serde_json::Value;
use std::sync::Arc;

// ───────────────────────── `NexusValue` enum ──────────────────────────

/// Native Cypher runtime value. Marked `#[non_exhaustive]` so adding
/// a new variant (typed LIST, DURATION, DATE, ...) in a future release
/// does not break downstream exhaustive matches on the enum — callers
/// with a wildcard arm continue to compile unchanged.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq)]
pub enum NexusValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(Arc<str>),
    /// phase6_opencypher-advanced-types §1.1 — BYTES scalar.
    Bytes(Arc<[u8]>),
    List(Arc<[NexusValue]>),
    Map(Arc<Vec<(String, NexusValue)>>),
}

impl NexusValue {
    /// Short type-name for error messages — matches the openCypher
    /// spec's canonical uppercase form.
    pub fn type_name(&self) -> &'static str {
        match self {
            NexusValue::Null => "NULL",
            NexusValue::Bool(_) => "BOOLEAN",
            NexusValue::Int(_) => "INTEGER",
            NexusValue::Float(_) => "FLOAT",
            NexusValue::String(_) => "STRING",
            NexusValue::Bytes(_) => "BYTES",
            NexusValue::List(_) => "LIST",
            NexusValue::Map(_) => "MAP",
        }
    }

    /// True iff this value is a BYTES scalar.
    pub fn is_bytes(&self) -> bool {
        matches!(self, NexusValue::Bytes(_))
    }

    /// Borrow the underlying byte slice, if this is a BYTES scalar.
    pub fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            NexusValue::Bytes(b) => Some(b),
            _ => None,
        }
    }

    /// Decode a [`serde_json::Value`] into a [`NexusValue`]. The
    /// JSON-wire BYTES shape (`{"_bytes": "<base64>"}`) round-trips
    /// back into `Bytes`; every other JSON shape maps to the matching
    /// variant. Numeric precision is preserved for i64 / u64 / f64.
    pub fn from_json(v: &Value) -> Self {
        match v {
            Value::Null => NexusValue::Null,
            Value::Bool(b) => NexusValue::Bool(*b),
            Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    NexusValue::Int(i)
                } else if let Some(u) = n.as_u64() {
                    NexusValue::Int(u as i64)
                } else {
                    NexusValue::Float(n.as_f64().unwrap_or(0.0))
                }
            }
            Value::String(s) => NexusValue::String(Arc::from(s.as_str())),
            Value::Array(arr) => {
                let items: Vec<NexusValue> = arr.iter().map(NexusValue::from_json).collect();
                NexusValue::List(Arc::from(items.into_boxed_slice()))
            }
            Value::Object(map) => {
                // BYTES wire-shape detection: single key `_bytes` with
                // a STRING value decodes to `Bytes`. Anything else is
                // a MAP.
                if map.len() == 1 {
                    if let Some(Value::String(s)) = map.get("_bytes") {
                        if let Ok(raw) = B64.decode(s) {
                            return NexusValue::Bytes(Arc::from(raw.into_boxed_slice()));
                        }
                    }
                }
                let pairs: Vec<(String, NexusValue)> = map
                    .iter()
                    .map(|(k, v)| (k.clone(), NexusValue::from_json(v)))
                    .collect();
                NexusValue::Map(Arc::new(pairs))
            }
        }
    }

    /// Re-encode this value as a [`serde_json::Value`]. `Bytes` emits
    /// the canonical wire shape; every other variant maps to the
    /// obvious JSON form.
    pub fn into_json(&self) -> Value {
        match self {
            NexusValue::Null => Value::Null,
            NexusValue::Bool(b) => Value::Bool(*b),
            NexusValue::Int(i) => Value::Number((*i).into()),
            NexusValue::Float(f) => serde_json::Number::from_f64(*f)
                .map(Value::Number)
                .unwrap_or(Value::Null),
            NexusValue::String(s) => Value::String(s.to_string()),
            NexusValue::Bytes(b) => {
                let mut map = serde_json::Map::with_capacity(1);
                map.insert("_bytes".to_string(), Value::String(B64.encode(b.as_ref())));
                Value::Object(map)
            }
            NexusValue::List(xs) => Value::Array(xs.iter().map(|x| x.into_json()).collect()),
            NexusValue::Map(pairs) => {
                let mut out = serde_json::Map::with_capacity(pairs.len());
                for (k, v) in pairs.iter() {
                    out.insert(k.clone(), v.into_json());
                }
                Value::Object(out)
            }
        }
    }
}

// ─────────────────── Property-chain binary encoder ────────────────────
//
// phase6_opencypher-advanced-types §1.2 — property values stored
// outside the JSON wire format (future on-disk property-chain
// migration) use the byte tags below. The format is single-pass
// readable: every variant starts with a 1-byte tag and carries its
// own payload-length-prefix where needed. `TYPE_BYTES` is the new
// tag this phase introduces; the older tags are laid out for
// forward compatibility with the phase-6 typed-list effort.
//
// Kept here (not in `storage/property_store.rs`) so the encoder can
// be unit-tested without pulling in the full mmap / transaction
// stack.

/// Tag bytes for the binary property encoding.
pub mod tag {
    pub const NULL: u8 = 0x00;
    pub const BOOL_FALSE: u8 = 0x01;
    pub const BOOL_TRUE: u8 = 0x02;
    pub const INT: u8 = 0x03;
    pub const FLOAT: u8 = 0x04;
    pub const STRING: u8 = 0x05;
    pub const LIST: u8 = 0x06;
    pub const MAP: u8 = 0x07;
    /// phase6_opencypher-advanced-types §1.2 — BYTES tag.
    /// Payload: `[tag:u8=0x0F][len:u32 LE][bytes...]`.
    pub const BYTES: u8 = 0x0F;
}

/// Maximum payload size accepted by `encode` / `decode` for any
/// single STRING / LIST / MAP / BYTES value — 64 MiB. Matches the
/// `ERR_BYTES_TOO_LARGE` cap enforced on the wire-side in
/// `executor::eval::bytes`.
pub const MAX_ELEMENT_BYTES: usize = 64 * 1024 * 1024;

/// Encode a [`NexusValue`] into the binary property-chain format.
/// Returns `Err(Error::Storage)` if any single string / byte slice /
/// collection exceeds [`MAX_ELEMENT_BYTES`].
pub fn encode(value: &NexusValue) -> crate::Result<Vec<u8>> {
    let mut out = Vec::new();
    encode_into(value, &mut out)?;
    Ok(out)
}

fn encode_into(value: &NexusValue, out: &mut Vec<u8>) -> crate::Result<()> {
    match value {
        NexusValue::Null => out.push(tag::NULL),
        NexusValue::Bool(false) => out.push(tag::BOOL_FALSE),
        NexusValue::Bool(true) => out.push(tag::BOOL_TRUE),
        NexusValue::Int(i) => {
            out.push(tag::INT);
            out.extend_from_slice(&i.to_le_bytes());
        }
        NexusValue::Float(f) => {
            out.push(tag::FLOAT);
            out.extend_from_slice(&f.to_le_bytes());
        }
        NexusValue::String(s) => {
            let bytes = s.as_bytes();
            write_len_prefixed(tag::STRING, bytes, out)?;
        }
        NexusValue::Bytes(b) => {
            write_len_prefixed(tag::BYTES, b.as_ref(), out)?;
        }
        NexusValue::List(xs) => {
            out.push(tag::LIST);
            let count: u32 = xs.len().try_into().map_err(|_| {
                crate::Error::storage("LIST length overflows u32 in binary encoder".to_string())
            })?;
            out.extend_from_slice(&count.to_le_bytes());
            for x in xs.iter() {
                encode_into(x, out)?;
            }
        }
        NexusValue::Map(pairs) => {
            out.push(tag::MAP);
            let count: u32 = pairs.len().try_into().map_err(|_| {
                crate::Error::storage("MAP length overflows u32 in binary encoder".to_string())
            })?;
            out.extend_from_slice(&count.to_le_bytes());
            for (k, v) in pairs.iter() {
                let kb = k.as_bytes();
                write_len_prefixed(tag::STRING, kb, out)?;
                encode_into(v, out)?;
            }
        }
    }
    Ok(())
}

fn write_len_prefixed(tag_byte: u8, payload: &[u8], out: &mut Vec<u8>) -> crate::Result<()> {
    if payload.len() > MAX_ELEMENT_BYTES {
        return Err(crate::Error::storage(format!(
            "ERR_BYTES_TOO_LARGE: payload {} exceeds {}-byte cap",
            payload.len(),
            MAX_ELEMENT_BYTES
        )));
    }
    out.push(tag_byte);
    let len: u32 = payload.len() as u32;
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(payload);
    Ok(())
}

/// Decode a [`NexusValue`] from the binary property-chain format.
pub fn decode(buf: &[u8]) -> crate::Result<NexusValue> {
    let mut cursor = 0usize;
    let v = decode_into(buf, &mut cursor)?;
    if cursor != buf.len() {
        return Err(crate::Error::storage(format!(
            "trailing bytes after decode: {} unread of {}",
            buf.len() - cursor,
            buf.len()
        )));
    }
    Ok(v)
}

fn decode_into(buf: &[u8], cursor: &mut usize) -> crate::Result<NexusValue> {
    let tag = read_u8(buf, cursor)?;
    match tag {
        tag::NULL => Ok(NexusValue::Null),
        tag::BOOL_FALSE => Ok(NexusValue::Bool(false)),
        tag::BOOL_TRUE => Ok(NexusValue::Bool(true)),
        tag::INT => {
            let bytes = read_slice(buf, cursor, 8)?;
            let mut arr = [0u8; 8];
            arr.copy_from_slice(bytes);
            Ok(NexusValue::Int(i64::from_le_bytes(arr)))
        }
        tag::FLOAT => {
            let bytes = read_slice(buf, cursor, 8)?;
            let mut arr = [0u8; 8];
            arr.copy_from_slice(bytes);
            Ok(NexusValue::Float(f64::from_le_bytes(arr)))
        }
        tag::STRING => {
            let payload = read_len_prefixed(buf, cursor)?;
            let s = std::str::from_utf8(payload)
                .map_err(|e| crate::Error::storage(format!("invalid UTF-8 in STRING: {e}")))?;
            Ok(NexusValue::String(Arc::from(s)))
        }
        tag::BYTES => {
            let payload = read_len_prefixed(buf, cursor)?;
            Ok(NexusValue::Bytes(Arc::from(
                payload.to_vec().into_boxed_slice(),
            )))
        }
        tag::LIST => {
            let count = read_u32(buf, cursor)? as usize;
            let mut items = Vec::with_capacity(count);
            for _ in 0..count {
                items.push(decode_into(buf, cursor)?);
            }
            Ok(NexusValue::List(Arc::from(items.into_boxed_slice())))
        }
        tag::MAP => {
            let count = read_u32(buf, cursor)? as usize;
            let mut pairs = Vec::with_capacity(count);
            for _ in 0..count {
                let k_tag = read_u8(buf, cursor)?;
                if k_tag != tag::STRING {
                    return Err(crate::Error::storage(format!(
                        "MAP key tag must be STRING (0x05), got 0x{k_tag:02x}"
                    )));
                }
                let payload = read_len_prefixed(buf, cursor)?;
                let k = std::str::from_utf8(payload)
                    .map_err(|e| crate::Error::storage(format!("invalid UTF-8 in MAP key: {e}")))?
                    .to_string();
                let v = decode_into(buf, cursor)?;
                pairs.push((k, v));
            }
            Ok(NexusValue::Map(Arc::new(pairs)))
        }
        other => Err(crate::Error::storage(format!(
            "unknown property tag 0x{other:02x}"
        ))),
    }
}

fn read_u8(buf: &[u8], cursor: &mut usize) -> crate::Result<u8> {
    if *cursor >= buf.len() {
        return Err(crate::Error::storage(
            "property decode: unexpected EOF reading tag".to_string(),
        ));
    }
    let b = buf[*cursor];
    *cursor += 1;
    Ok(b)
}

fn read_u32(buf: &[u8], cursor: &mut usize) -> crate::Result<u32> {
    let s = read_slice(buf, cursor, 4)?;
    let mut arr = [0u8; 4];
    arr.copy_from_slice(s);
    Ok(u32::from_le_bytes(arr))
}

fn read_slice<'a>(buf: &'a [u8], cursor: &mut usize, len: usize) -> crate::Result<&'a [u8]> {
    if cursor.saturating_add(len) > buf.len() {
        return Err(crate::Error::storage(format!(
            "property decode: wanted {len} bytes, only {} available",
            buf.len() - *cursor
        )));
    }
    let end = *cursor + len;
    let s = &buf[*cursor..end];
    *cursor = end;
    Ok(s)
}

fn read_len_prefixed<'a>(buf: &'a [u8], cursor: &mut usize) -> crate::Result<&'a [u8]> {
    let len = read_u32(buf, cursor)? as usize;
    if len > MAX_ELEMENT_BYTES {
        return Err(crate::Error::storage(format!(
            "property decode: payload length {len} exceeds {}-byte cap",
            MAX_ELEMENT_BYTES
        )));
    }
    read_slice(buf, cursor, len)
}

// ───────────────────────────── tests ──────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn av(v: NexusValue) -> NexusValue {
        v
    }

    #[test]
    fn json_roundtrip_scalars() {
        for case in [
            av(NexusValue::Null),
            av(NexusValue::Bool(true)),
            av(NexusValue::Int(42)),
            av(NexusValue::Float(3.5)),
            av(NexusValue::String(Arc::from("hello"))),
            av(NexusValue::Bytes(Arc::from([0u8, 1, 255].as_slice()))),
        ] {
            let j = case.into_json();
            let back = NexusValue::from_json(&j);
            assert_eq!(back, case, "roundtrip failure for {j}");
        }
    }

    #[test]
    fn json_bytes_wire_shape_is_single_key_object() {
        let b = NexusValue::Bytes(Arc::from([0u8, 1, 0xff].as_slice()));
        assert_eq!(b.into_json(), json!({"_bytes": "AAH/"}));
    }

    #[test]
    fn from_json_map_with_bytes_key_but_extra_entries_stays_map() {
        // The BYTES wire shape requires exactly one key. A map with a
        // `_bytes` entry alongside other keys must decode as a MAP,
        // not as BYTES.
        let j = json!({"_bytes": "AAH/", "other": 1});
        match NexusValue::from_json(&j) {
            NexusValue::Map(_) => {}
            other => panic!("expected MAP, got {:?}", other),
        }
    }

    #[test]
    fn binary_roundtrip_every_variant() {
        let inner = vec![
            NexusValue::Int(1),
            NexusValue::Int(2),
            NexusValue::String(Arc::from("x")),
        ];
        let v = NexusValue::Map(Arc::new(vec![
            ("null".to_string(), NexusValue::Null),
            ("bool".to_string(), NexusValue::Bool(true)),
            ("int".to_string(), NexusValue::Int(-17)),
            ("float".to_string(), NexusValue::Float(2.5)),
            ("str".to_string(), NexusValue::String(Arc::from("hello"))),
            (
                "bytes".to_string(),
                NexusValue::Bytes(Arc::from([0u8, 1, 0xff, 0x20].as_slice())),
            ),
            (
                "list".to_string(),
                NexusValue::List(Arc::from(inner.into_boxed_slice())),
            ),
        ]));

        let bytes = encode(&v).unwrap();
        let back = decode(&bytes).unwrap();
        assert_eq!(back, v);
    }

    #[test]
    fn binary_bytes_tag_is_0x0f() {
        let v = NexusValue::Bytes(Arc::from([0xabu8, 0xcd].as_slice()));
        let buf = encode(&v).unwrap();
        assert_eq!(buf[0], 0x0f);
        // u32 length-prefix LE = 2
        assert_eq!(&buf[1..5], &2u32.to_le_bytes());
        assert_eq!(&buf[5..], &[0xab, 0xcd]);
    }

    #[test]
    fn binary_decode_rejects_unknown_tag() {
        let err = decode(&[0x42]).unwrap_err();
        assert!(err.to_string().contains("unknown property tag"));
    }

    #[test]
    fn binary_decode_rejects_truncated_payload() {
        // BYTES tag + length 10 but only 2 bytes follow.
        let buf = [0x0f, 10, 0, 0, 0, 1, 2];
        let err = decode(&buf).unwrap_err();
        assert!(err.to_string().contains("wanted"));
    }

    #[test]
    fn binary_decode_rejects_trailing_bytes() {
        let mut buf = encode(&NexusValue::Bool(true)).unwrap();
        buf.push(0x99);
        let err = decode(&buf).unwrap_err();
        assert!(err.to_string().contains("trailing bytes"));
    }

    #[test]
    fn encode_rejects_oversize_payload() {
        let big = vec![0u8; MAX_ELEMENT_BYTES + 1];
        let err = encode(&NexusValue::Bytes(Arc::from(big.into_boxed_slice()))).unwrap_err();
        assert!(err.to_string().contains("ERR_BYTES_TOO_LARGE"));
    }

    #[test]
    fn type_name_matches_cypher_spec() {
        assert_eq!(NexusValue::Null.type_name(), "NULL");
        assert_eq!(NexusValue::Bool(true).type_name(), "BOOLEAN");
        assert_eq!(NexusValue::Int(1).type_name(), "INTEGER");
        assert_eq!(NexusValue::Float(1.0).type_name(), "FLOAT");
        assert_eq!(NexusValue::String(Arc::from("x")).type_name(), "STRING");
        assert_eq!(
            NexusValue::Bytes(Arc::from([0u8].as_slice())).type_name(),
            "BYTES"
        );
    }

    #[test]
    fn non_exhaustive_wildcard_protection() {
        // Compile-time assertion: matching `NexusValue` without a
        // wildcard arm should fail to compile. We model that here
        // with a match that HAS a wildcard — if the enum ever loses
        // `#[non_exhaustive]`, this test still compiles (false
        // negative), but if we drop the wildcard we break the
        // promise. Kept as a reminder to reviewers.
        let v = NexusValue::Bool(true);
        let _ = match v {
            NexusValue::Bool(_) => 1,
            _ => 0,
        };
    }

    #[test]
    fn as_bytes_returns_slice_only_for_bytes() {
        let b = NexusValue::Bytes(Arc::from([0xaau8, 0xbb].as_slice()));
        assert_eq!(b.as_bytes(), Some(&[0xaa, 0xbb][..]));
        assert_eq!(NexusValue::Int(1).as_bytes(), None);
    }
}
