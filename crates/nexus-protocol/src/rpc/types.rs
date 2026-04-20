//! RPC wire types shared by client and server.
//!
//! All frames encode with rmp-serde's default externally-tagged representation
//! so `NexusValue::Null` serializes as the string `"Null"` and payload variants
//! serialize as a single-key map `{"Variant": payload}`. This matches Synap's
//! `SynapValue` byte-for-byte, which keeps cross-project tooling (debuggers,
//! packet captures, Grafana tails) interoperable.

use serde::{Deserialize, Serialize};

/// A dynamically-typed value carried by RPC requests and responses.
///
/// This is deliberately a small value-type enum, not an `Any`: every SDK
/// should be able to reconstruct the full set of variants with a `match` and
/// a flat switch on the tag byte.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum NexusValue {
    /// SQL NULL / absent property. Distinct from an empty string or zero.
    Null,
    /// Boolean scalar.
    Bool(bool),
    /// Signed 64-bit integer.
    Int(i64),
    /// IEEE-754 double-precision float.
    Float(f64),
    /// Raw bytes with no base64 step — ideal for f32 embedding vectors.
    Bytes(Vec<u8>),
    /// UTF-8 string.
    Str(String),
    /// Heterogeneous array.
    Array(Vec<NexusValue>),
    /// Association list preserving insertion order; key type is a `NexusValue`
    /// so KNN filters and property maps can use non-string keys if needed.
    Map(Vec<(NexusValue, NexusValue)>),
}

impl NexusValue {
    /// Returns the inner string slice if this value is a [`NexusValue::Str`].
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Str(s) => Some(s.as_str()),
            _ => None,
        }
    }

    /// Returns the inner bytes if this value is [`NexusValue::Bytes`] or a
    /// [`NexusValue::Str`] (the UTF-8 bytes of the string).
    pub fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            Self::Bytes(b) => Some(b.as_slice()),
            Self::Str(s) => Some(s.as_bytes()),
            _ => None,
        }
    }

    /// Returns the inner integer. Does not coerce floats.
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Self::Int(i) => Some(*i),
            _ => None,
        }
    }

    /// Returns the inner float, coercing [`NexusValue::Int`] as a convenience
    /// so KNN embeddings encoded as `Array<Int>` still work.
    pub fn as_float(&self) -> Option<f64> {
        match self {
            Self::Float(f) => Some(*f),
            Self::Int(i) => Some(*i as f64),
            _ => None,
        }
    }

    /// Returns `true` iff this value is [`NexusValue::Null`].
    pub fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }
}

// ── From impls for ergonomic call sites ──────────────────────────────────────

impl From<String> for NexusValue {
    fn from(s: String) -> Self {
        Self::Str(s)
    }
}

impl From<&str> for NexusValue {
    fn from(s: &str) -> Self {
        Self::Str(s.to_owned())
    }
}

impl From<Vec<u8>> for NexusValue {
    fn from(b: Vec<u8>) -> Self {
        Self::Bytes(b)
    }
}

impl From<i64> for NexusValue {
    fn from(i: i64) -> Self {
        Self::Int(i)
    }
}

impl From<f64> for NexusValue {
    fn from(f: f64) -> Self {
        Self::Float(f)
    }
}

impl From<bool> for NexusValue {
    fn from(b: bool) -> Self {
        Self::Bool(b)
    }
}

// ── Wire frames ──────────────────────────────────────────────────────────────

/// A request from client to server.
///
/// `id` is caller-chosen and echoed back in the matching [`Response`]. Clients
/// SHOULD use monotonic ids per connection; the server imposes no ordering
/// guarantee — responses may arrive out of order when the dispatcher runs
/// requests concurrently.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub id: u32,
    pub command: String,
    pub args: Vec<NexusValue>,
}

/// A response from server to client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub id: u32,
    pub result: Result<NexusValue, String>,
}

impl Response {
    pub fn ok(id: u32, value: NexusValue) -> Self {
        Self {
            id,
            result: Ok(value),
        }
    }

    pub fn err(id: u32, msg: impl Into<String>) -> Self {
        Self {
            id,
            result: Err(msg.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nexus_value_roundtrip_all_variants() {
        let variants: Vec<NexusValue> = vec![
            NexusValue::Null,
            NexusValue::Bool(true),
            NexusValue::Bool(false),
            NexusValue::Int(i64::MIN),
            NexusValue::Int(0),
            NexusValue::Int(i64::MAX),
            NexusValue::Float(1.5_f64),
            NexusValue::Float(f64::NEG_INFINITY),
            NexusValue::Float(f64::INFINITY),
            NexusValue::Bytes(vec![0, 1, 2, 255]),
            NexusValue::Bytes(vec![]),
            NexusValue::Str("hello".into()),
            NexusValue::Str(String::new()),
            NexusValue::Array(vec![NexusValue::Int(1), NexusValue::Str("two".into())]),
            NexusValue::Map(vec![(NexusValue::Str("k".into()), NexusValue::Int(99))]),
        ];

        for v in variants {
            let encoded = rmp_serde::to_vec(&v).expect("encode");
            let decoded: NexusValue = rmp_serde::from_slice(&encoded).expect("decode");
            assert_eq!(v, decoded);
        }
    }

    #[test]
    fn nexus_value_nan_roundtrip_is_nan() {
        // NaN does not compare equal, but the bit pattern must survive.
        let enc = rmp_serde::to_vec(&NexusValue::Float(f64::NAN)).unwrap();
        let dec: NexusValue = rmp_serde::from_slice(&enc).unwrap();
        match dec {
            NexusValue::Float(f) => assert!(f.is_nan()),
            other => panic!("expected Float(NaN), got {other:?}"),
        }
    }

    #[test]
    fn request_response_serde() {
        let req = Request {
            id: 42,
            command: "CYPHER".into(),
            args: vec![
                NexusValue::Str("RETURN 1".into()),
                NexusValue::Bytes(b"params".to_vec()),
            ],
        };
        let enc = rmp_serde::to_vec(&req).unwrap();
        let dec: Request = rmp_serde::from_slice(&enc).unwrap();
        assert_eq!(dec.id, 42);
        assert_eq!(dec.command, "CYPHER");
        assert_eq!(dec.args.len(), 2);

        let resp = Response::ok(42, NexusValue::Str("OK".into()));
        let enc = rmp_serde::to_vec(&resp).unwrap();
        let dec: Response = rmp_serde::from_slice(&enc).unwrap();
        assert_eq!(dec.id, 42);
        assert!(dec.result.is_ok());

        let err = Response::err(7, "boom");
        let enc = rmp_serde::to_vec(&err).unwrap();
        let dec: Response = rmp_serde::from_slice(&enc).unwrap();
        assert_eq!(dec.id, 7);
        assert_eq!(dec.result.err().as_deref(), Some("boom"));
    }

    #[test]
    fn accessor_helpers() {
        assert_eq!(NexusValue::Str("x".into()).as_str(), Some("x"));
        assert_eq!(NexusValue::Int(42).as_int(), Some(42));
        assert_eq!(NexusValue::Int(3).as_float(), Some(3.0));
        assert_eq!(NexusValue::Float(2.5).as_float(), Some(2.5));
        assert_eq!(
            NexusValue::Bytes(vec![1, 2, 3]).as_bytes(),
            Some(&[1u8, 2, 3][..])
        );
        assert_eq!(NexusValue::Str("abc".into()).as_bytes(), Some(&b"abc"[..]));
        assert!(NexusValue::Null.is_null());
        assert!(!NexusValue::Int(0).is_null());
        assert_eq!(NexusValue::Bool(true).as_str(), None);
        assert_eq!(NexusValue::Bool(true).as_int(), None);
    }

    #[test]
    fn from_impls_cover_common_scalars() {
        let _: NexusValue = String::from("s").into();
        let _: NexusValue = "s".into();
        let _: NexusValue = vec![0u8, 1, 2].into();
        let _: NexusValue = 7i64.into();
        let _: NexusValue = 1.5f64.into();
        let _: NexusValue = true.into();
    }
}
