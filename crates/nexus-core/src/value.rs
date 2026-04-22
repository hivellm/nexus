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
    /// phase6_opencypher-advanced-types §4.2 — typed homogeneous list.
    /// Encoded with a single 1-byte element-type tag in the list
    /// header and inline per-element payloads (no per-element tag
    /// bytes). Untyped / heterogeneous lists keep using [`List`]
    /// above, which carries one tag per element.
    TypedList {
        /// Element-type code — see [`typed_list_elem`] constants.
        elem_type: u8,
        /// Items, every one of them compatible with `elem_type`.
        items: Arc<[NexusValue]>,
    },
    Map(Arc<Vec<(String, NexusValue)>>),
}

/// Element-type codes for [`NexusValue::TypedList`] (§4.2).
/// Kept as free constants rather than an enum so the on-disk
/// format stays a single byte and we can extend the range without
/// re-tagging.
pub mod typed_list_elem {
    /// Untyped / heterogeneous — each element falls back to a
    /// per-element tag byte in the payload.
    pub const ANY: u8 = 0x00;
    pub const INT: u8 = 0x01;
    pub const FLOAT: u8 = 0x02;
    pub const BOOL: u8 = 0x03;
    pub const STRING: u8 = 0x04;
    pub const BYTES: u8 = 0x05;
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
            NexusValue::TypedList { .. } => "LIST",
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
            NexusValue::TypedList { items, .. } => {
                // Typed lists surface as plain JSON arrays on the
                // wire — the element-type discipline lives in the
                // constraint catalog, not in the JSON shape.
                Value::Array(items.iter().map(|x| x.into_json()).collect())
            }
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
    /// phase6_opencypher-advanced-types §4.2 — typed LIST<T>.
    /// Payload: `[tag:u8=0x0C][elem_type:u8][count:u32 LE][items...]`.
    /// Scalar elements (INT/FLOAT/BOOL/STRING/BYTES) have **no**
    /// per-element tag — the one-byte header carries the type for
    /// every element. `ANY` (0x00) falls back to per-element tags
    /// and behaves identically to the untyped `LIST` tag above.
    pub const TYPED_LIST: u8 = 0x0C;
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
        NexusValue::TypedList { elem_type, items } => {
            out.push(tag::TYPED_LIST);
            out.push(*elem_type);
            let count: u32 = items.len().try_into().map_err(|_| {
                crate::Error::storage(
                    "TYPED_LIST length overflows u32 in binary encoder".to_string(),
                )
            })?;
            out.extend_from_slice(&count.to_le_bytes());
            for item in items.iter() {
                encode_typed_list_elem(*elem_type, item, out)?;
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

/// Encode one element of a `TypedList` (§4.2). For scalar element
/// types the payload is written inline — no per-element tag byte —
/// which is the compactness payoff of typing the list. `ANY` falls
/// back to the untyped `encode_into` path.
///
/// The caller (`encode_into`) has already emitted the list header
/// `[0x0C][elem_type:u8][count:u32]`; this function handles only
/// each item's body.
fn encode_typed_list_elem(elem_type: u8, v: &NexusValue, out: &mut Vec<u8>) -> crate::Result<()> {
    match elem_type {
        typed_list_elem::ANY => encode_into(v, out),
        typed_list_elem::INT => match v {
            NexusValue::Int(i) => {
                out.extend_from_slice(&i.to_le_bytes());
                Ok(())
            }
            other => typed_mismatch("INT", other),
        },
        typed_list_elem::FLOAT => match v {
            NexusValue::Float(f) => {
                out.extend_from_slice(&f.to_le_bytes());
                Ok(())
            }
            NexusValue::Int(i) => {
                // Integer coerces upward into the float slot so
                // typed LIST<FLOAT> accepts literal 1 the same way
                // Cypher's type coercion does.
                out.extend_from_slice(&(*i as f64).to_le_bytes());
                Ok(())
            }
            other => typed_mismatch("FLOAT", other),
        },
        typed_list_elem::BOOL => match v {
            NexusValue::Bool(b) => {
                out.push(if *b { 1 } else { 0 });
                Ok(())
            }
            other => typed_mismatch("BOOLEAN", other),
        },
        typed_list_elem::STRING => match v {
            NexusValue::String(s) => write_len_only(s.as_bytes(), out),
            other => typed_mismatch("STRING", other),
        },
        typed_list_elem::BYTES => match v {
            NexusValue::Bytes(b) => write_len_only(b.as_ref(), out),
            other => typed_mismatch("BYTES", other),
        },
        unknown => Err(crate::Error::storage(format!(
            "TYPED_LIST: unknown element-type code 0x{unknown:02x}"
        ))),
    }
}

fn write_len_only(payload: &[u8], out: &mut Vec<u8>) -> crate::Result<()> {
    if payload.len() > MAX_ELEMENT_BYTES {
        return Err(crate::Error::storage(format!(
            "ERR_BYTES_TOO_LARGE: payload {} exceeds {}-byte cap",
            payload.len(),
            MAX_ELEMENT_BYTES
        )));
    }
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(payload);
    Ok(())
}

fn typed_mismatch(expected: &str, got: &NexusValue) -> crate::Result<()> {
    Err(crate::Error::CypherExecution(format!(
        "ERR_CONSTRAINT_VIOLATED: TYPED_LIST expected {expected}, got {}",
        got.type_name()
    )))
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
        tag::TYPED_LIST => {
            let elem_type = read_u8(buf, cursor)?;
            let count = read_u32(buf, cursor)? as usize;
            let mut items = Vec::with_capacity(count);
            for _ in 0..count {
                items.push(decode_typed_list_elem(elem_type, buf, cursor)?);
            }
            Ok(NexusValue::TypedList {
                elem_type,
                items: Arc::from(items.into_boxed_slice()),
            })
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

fn decode_typed_list_elem(
    elem_type: u8,
    buf: &[u8],
    cursor: &mut usize,
) -> crate::Result<NexusValue> {
    match elem_type {
        typed_list_elem::ANY => decode_into(buf, cursor),
        typed_list_elem::INT => {
            let bytes = read_slice(buf, cursor, 8)?;
            let mut arr = [0u8; 8];
            arr.copy_from_slice(bytes);
            Ok(NexusValue::Int(i64::from_le_bytes(arr)))
        }
        typed_list_elem::FLOAT => {
            let bytes = read_slice(buf, cursor, 8)?;
            let mut arr = [0u8; 8];
            arr.copy_from_slice(bytes);
            Ok(NexusValue::Float(f64::from_le_bytes(arr)))
        }
        typed_list_elem::BOOL => {
            let b = read_u8(buf, cursor)?;
            Ok(NexusValue::Bool(b != 0))
        }
        typed_list_elem::STRING => {
            let payload = read_len_prefixed(buf, cursor)?;
            let s = std::str::from_utf8(payload).map_err(|e| {
                crate::Error::storage(format!("TYPED_LIST: invalid UTF-8 in STRING: {e}"))
            })?;
            Ok(NexusValue::String(Arc::from(s)))
        }
        typed_list_elem::BYTES => {
            let payload = read_len_prefixed(buf, cursor)?;
            Ok(NexusValue::Bytes(Arc::from(
                payload.to_vec().into_boxed_slice(),
            )))
        }
        unknown => Err(crate::Error::storage(format!(
            "TYPED_LIST: unknown element-type code 0x{unknown:02x}"
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

    // ─────────── phase6_opencypher-advanced-types §4.2 ───────────

    #[test]
    fn typed_list_int_roundtrips_with_inline_scalars() {
        let items: Vec<NexusValue> = (0..5).map(NexusValue::Int).collect();
        let v = NexusValue::TypedList {
            elem_type: typed_list_elem::INT,
            items: Arc::from(items.into_boxed_slice()),
        };
        let buf = encode(&v).unwrap();
        // Header: 0x0C [elem=0x01] [count=5 LE]
        assert_eq!(buf[0], 0x0c);
        assert_eq!(buf[1], typed_list_elem::INT);
        assert_eq!(&buf[2..6], &5u32.to_le_bytes());
        // Body is 5 × 8 = 40 bytes of i64 LE — no per-element tag
        // bytes. So the whole buffer is 6 + 40 = 46 bytes.
        assert_eq!(buf.len(), 46);

        let back = decode(&buf).unwrap();
        assert_eq!(back, v);
    }

    #[test]
    fn typed_list_float_accepts_integer_coercion() {
        let items = vec![NexusValue::Int(1), NexusValue::Float(2.5)];
        let v = NexusValue::TypedList {
            elem_type: typed_list_elem::FLOAT,
            items: Arc::from(items.into_boxed_slice()),
        };
        let buf = encode(&v).unwrap();
        // Decoder promotes every slot into Float because the inline
        // layout is u64 LE bits interpreted as f64.
        match decode(&buf).unwrap() {
            NexusValue::TypedList { elem_type, items } => {
                assert_eq!(elem_type, typed_list_elem::FLOAT);
                assert_eq!(items[0], NexusValue::Float(1.0));
                assert_eq!(items[1], NexusValue::Float(2.5));
            }
            other => panic!("expected TypedList, got {:?}", other),
        }
    }

    #[test]
    fn typed_list_string_roundtrip() {
        let items = vec![
            NexusValue::String(Arc::from("a")),
            NexusValue::String(Arc::from("bc")),
        ];
        let v = NexusValue::TypedList {
            elem_type: typed_list_elem::STRING,
            items: Arc::from(items.into_boxed_slice()),
        };
        let buf = encode(&v).unwrap();
        // No per-element tag. Each element: [len:u32 LE][utf8].
        // So layout: 0x0C 0x04 [count:u32=2]
        //            [len:u32=1] "a"
        //            [len:u32=2] "bc"
        // = 6 + (4+1) + (4+2) = 17 bytes
        assert_eq!(buf.len(), 17);
        assert_eq!(decode(&buf).unwrap(), v);
    }

    #[test]
    fn typed_list_bytes_roundtrip() {
        let items = vec![
            NexusValue::Bytes(Arc::from([0u8, 1].as_slice())),
            NexusValue::Bytes(Arc::from([0xffu8, 0xee, 0xdd].as_slice())),
        ];
        let v = NexusValue::TypedList {
            elem_type: typed_list_elem::BYTES,
            items: Arc::from(items.into_boxed_slice()),
        };
        let buf = encode(&v).unwrap();
        let back = decode(&buf).unwrap();
        assert_eq!(back, v);
    }

    #[test]
    fn typed_list_bool_uses_one_byte_per_element() {
        let items = vec![NexusValue::Bool(true), NexusValue::Bool(false)];
        let v = NexusValue::TypedList {
            elem_type: typed_list_elem::BOOL,
            items: Arc::from(items.into_boxed_slice()),
        };
        let buf = encode(&v).unwrap();
        // Header (6 bytes) + 2 × 1 byte = 8 bytes.
        assert_eq!(buf.len(), 8);
        assert_eq!(buf[6], 1);
        assert_eq!(buf[7], 0);
        assert_eq!(decode(&buf).unwrap(), v);
    }

    #[test]
    fn typed_list_rejects_wrong_element_type_on_encode() {
        let items = vec![NexusValue::Int(1), NexusValue::String(Arc::from("two"))];
        let v = NexusValue::TypedList {
            elem_type: typed_list_elem::INT,
            items: Arc::from(items.into_boxed_slice()),
        };
        let err = encode(&v).unwrap_err();
        assert!(err.to_string().contains("ERR_CONSTRAINT_VIOLATED"));
    }

    #[test]
    fn typed_list_any_falls_back_to_per_element_tags() {
        // ANY is the escape hatch for heterogeneous lists and lines up
        // with the untyped format — the difference is just the 0x0C
        // header so the typed-list tag machinery stays active.
        let items = vec![
            NexusValue::Int(1),
            NexusValue::String(Arc::from("x")),
            NexusValue::Bool(true),
        ];
        let v = NexusValue::TypedList {
            elem_type: typed_list_elem::ANY,
            items: Arc::from(items.into_boxed_slice()),
        };
        let buf = encode(&v).unwrap();
        assert_eq!(decode(&buf).unwrap(), v);
    }

    #[test]
    fn typed_list_unknown_elem_type_rejected_on_decode() {
        // Hand-crafted buffer: TYPED_LIST tag + bogus elem type +
        // count=1 so the decoder actually tries to parse an element.
        // Empty typed-lists can't detect the bogus code (nothing to
        // decode), which is fine — storage never materialises an
        // empty list with an unknown type code.
        let buf = [0x0c, 0xff, 1, 0, 0, 0];
        let err = decode(&buf).unwrap_err();
        assert!(err.to_string().contains("unknown element-type code"));
    }

    #[test]
    fn typed_list_surfaces_as_plain_json_array() {
        let items = vec![NexusValue::Int(1), NexusValue::Int(2)];
        let v = NexusValue::TypedList {
            elem_type: typed_list_elem::INT,
            items: Arc::from(items.into_boxed_slice()),
        };
        assert_eq!(v.into_json(), json!([1, 2]));
    }

    #[test]
    fn as_bytes_returns_slice_only_for_bytes() {
        let b = NexusValue::Bytes(Arc::from([0xaau8, 0xbb].as_slice()));
        assert_eq!(b.as_bytes(), Some(&[0xaa, 0xbb][..]));
        assert_eq!(NexusValue::Int(1).as_bytes(), None);
    }
}
