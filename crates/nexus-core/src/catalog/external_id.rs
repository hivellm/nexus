//! External node identity types for the catalog.
//!
//! An [`ExternalId`] is a caller-supplied stable identifier that Nexus
//! stores alongside the internal `u64` node id.  The variant is encoded
//! as a 1-byte discriminator so the wire format can be extended without
//! rewriting the index:
//!
//! | Discriminator | Variant      | Payload                            |
//! |---------------|--------------|------------------------------------|
//! | `0x01`        | `Hash`       | 1-byte `HashKind` sub-disc + bytes |
//! | `0x02`        | `Uuid`       | 16 raw bytes                       |
//! | `0x03`        | `Str`        | 4-byte LE length + UTF-8 bytes     |
//! | `0x04`        | `Bytes`      | 4-byte LE length + raw bytes       |
//!
//! Hash sub-discriminators:
//!
//! | Sub-disc | HashKind | Length |
//! |----------|----------|--------|
//! | `0x01`   | Blake3   | 32     |
//! | `0x02`   | Sha256   | 32     |
//! | `0x03`   | Sha512   | 64     |

use std::fmt;
use std::str::FromStr;

use thiserror::Error;

// ──────────────────────────────────────────────────────────────────────────────
// Constants
// ──────────────────────────────────────────────────────────────────────────────

/// Maximum length of a `Str` variant in bytes.
pub const STR_MAX_BYTES: usize = 256;
/// Maximum length of a `Bytes` variant in bytes.
pub const BYTES_MAX_BYTES: usize = 64;

// Wire discriminators
const DISC_HASH: u8 = 0x01;
const DISC_UUID: u8 = 0x02;
const DISC_STR: u8 = 0x03;
const DISC_BYTES: u8 = 0x04;

// Hash sub-discriminators
const HASH_BLAKE3: u8 = 0x01;
const HASH_SHA256: u8 = 0x02;
const HASH_SHA512: u8 = 0x03;

// ──────────────────────────────────────────────────────────────────────────────
// Error type
// ──────────────────────────────────────────────────────────────────────────────

/// Errors produced during [`ExternalId`] construction or decoding.
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum ExternalIdError {
    /// Zero-length value.
    #[error("external id must not be empty")]
    Empty,

    /// Value exceeds the per-variant length cap.
    #[error("{kind} external id is too long: max {max} bytes, got {actual}")]
    TooLong {
        /// Variant description.
        kind: &'static str,
        /// Maximum allowed length.
        max: usize,
        /// Actual submitted length.
        actual: usize,
    },

    /// Hash byte slice does not match the declared hash algorithm's output size.
    #[error("{kind:?} hash has wrong length: expected {expected} bytes, got {actual}")]
    WrongHashLength {
        /// The hash algorithm.
        kind: HashKind,
        /// Expected byte count.
        expected: usize,
        /// Actual byte count.
        actual: usize,
    },

    /// Could not parse the textual representation.
    #[error("bad external id format: {0}")]
    BadFormat(String),
}

// ──────────────────────────────────────────────────────────────────────────────
// HashKind
// ──────────────────────────────────────────────────────────────────────────────

/// The hash algorithm associated with a [`ExternalId::Hash`] variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HashKind {
    /// BLAKE3, 32-byte output.
    Blake3,
    /// SHA-256, 32-byte output.
    Sha256,
    /// SHA-512, 64-byte output.
    Sha512,
}

impl HashKind {
    /// Expected raw byte length for this algorithm.
    pub fn byte_len(self) -> usize {
        match self {
            HashKind::Blake3 => 32,
            HashKind::Sha256 => 32,
            HashKind::Sha512 => 64,
        }
    }

    fn wire_byte(self) -> u8 {
        match self {
            HashKind::Blake3 => HASH_BLAKE3,
            HashKind::Sha256 => HASH_SHA256,
            HashKind::Sha512 => HASH_SHA512,
        }
    }

    fn from_wire_byte(b: u8) -> Result<Self, ExternalIdError> {
        match b {
            HASH_BLAKE3 => Ok(HashKind::Blake3),
            HASH_SHA256 => Ok(HashKind::Sha256),
            HASH_SHA512 => Ok(HashKind::Sha512),
            other => Err(ExternalIdError::BadFormat(format!(
                "unknown hash sub-discriminator 0x{other:02x}"
            ))),
        }
    }

    fn prefix(self) -> &'static str {
        match self {
            HashKind::Blake3 => "blake3",
            HashKind::Sha256 => "sha256",
            HashKind::Sha512 => "sha512",
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// ExternalId
// ──────────────────────────────────────────────────────────────────────────────

/// A caller-supplied stable external identifier for a graph node.
///
/// Each variant carries a different type of natural key.  Construct via
/// the `try_*` constructors (which validate length/content) or via
/// [`FromStr`] (which parses the canonical prefixed-string form).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ExternalId {
    /// A cryptographic hash of the node's content.
    Hash {
        /// The hash algorithm.
        kind: HashKind,
        /// Raw hash bytes (length determined by `kind`).
        bytes: Vec<u8>,
    },
    /// A UUID in binary form (16 bytes, RFC 4122).
    Uuid([u8; 16]),
    /// An arbitrary UTF-8 string, capped at [`STR_MAX_BYTES`] bytes.
    Str(String),
    /// Opaque binary data, capped at [`BYTES_MAX_BYTES`] bytes.
    Bytes(Vec<u8>),
}

impl ExternalId {
    // ── Constructors ──────────────────────────────────────────────────────────

    /// Construct a `Hash` variant, validating that `bytes` has the correct
    /// length for the given `kind`.
    pub fn try_hash(kind: HashKind, bytes: Vec<u8>) -> Result<Self, ExternalIdError> {
        if bytes.is_empty() {
            return Err(ExternalIdError::Empty);
        }
        let expected = kind.byte_len();
        if bytes.len() != expected {
            return Err(ExternalIdError::WrongHashLength {
                kind,
                expected,
                actual: bytes.len(),
            });
        }
        Ok(Self::Hash { kind, bytes })
    }

    /// Construct a `Uuid` variant from a 16-byte array.
    pub fn try_uuid(bytes: [u8; 16]) -> Result<Self, ExternalIdError> {
        // All 16-byte arrays are valid (nil UUID is allowed)
        Ok(Self::Uuid(bytes))
    }

    /// Construct a `Str` variant, validating that `s` is non-empty and
    /// does not exceed [`STR_MAX_BYTES`] bytes.
    pub fn try_str(s: String) -> Result<Self, ExternalIdError> {
        if s.is_empty() {
            return Err(ExternalIdError::Empty);
        }
        let len = s.len();
        if len > STR_MAX_BYTES {
            return Err(ExternalIdError::TooLong {
                kind: "Str",
                max: STR_MAX_BYTES,
                actual: len,
            });
        }
        Ok(Self::Str(s))
    }

    /// Construct a `Bytes` variant, validating that `b` is non-empty and
    /// does not exceed [`BYTES_MAX_BYTES`] bytes.
    pub fn try_bytes(b: Vec<u8>) -> Result<Self, ExternalIdError> {
        if b.is_empty() {
            return Err(ExternalIdError::Empty);
        }
        let len = b.len();
        if len > BYTES_MAX_BYTES {
            return Err(ExternalIdError::TooLong {
                kind: "Bytes",
                max: BYTES_MAX_BYTES,
                actual: len,
            });
        }
        Ok(Self::Bytes(b))
    }

    // ── Wire encoding / decoding ──────────────────────────────────────────────

    /// Encode this `ExternalId` to a compact byte sequence suitable for use
    /// as an LMDB key.
    ///
    /// Format: `[discriminator] [payload…]`
    pub fn to_bytes(&self) -> Vec<u8> {
        match self {
            ExternalId::Hash { kind, bytes } => {
                let mut out = Vec::with_capacity(2 + bytes.len());
                out.push(DISC_HASH);
                out.push(kind.wire_byte());
                out.extend_from_slice(bytes);
                out
            }
            ExternalId::Uuid(b) => {
                let mut out = Vec::with_capacity(17);
                out.push(DISC_UUID);
                out.extend_from_slice(b);
                out
            }
            ExternalId::Str(s) => {
                let sb = s.as_bytes();
                let mut out = Vec::with_capacity(5 + sb.len());
                out.push(DISC_STR);
                out.extend_from_slice(&(sb.len() as u32).to_le_bytes());
                out.extend_from_slice(sb);
                out
            }
            ExternalId::Bytes(b) => {
                let mut out = Vec::with_capacity(5 + b.len());
                out.push(DISC_BYTES);
                out.extend_from_slice(&(b.len() as u32).to_le_bytes());
                out.extend_from_slice(b);
                out
            }
        }
    }

    /// Decode an `ExternalId` from the byte sequence produced by
    /// [`ExternalId::to_bytes`].
    pub fn from_bytes(data: &[u8]) -> Result<Self, ExternalIdError> {
        if data.is_empty() {
            return Err(ExternalIdError::Empty);
        }
        match data[0] {
            DISC_HASH => {
                if data.len() < 2 {
                    return Err(ExternalIdError::BadFormat(
                        "Hash variant: missing sub-discriminator".into(),
                    ));
                }
                let kind = HashKind::from_wire_byte(data[1])?;
                let payload = data[2..].to_vec();
                Self::try_hash(kind, payload)
            }
            DISC_UUID => {
                if data.len() != 17 {
                    return Err(ExternalIdError::BadFormat(format!(
                        "Uuid variant: expected 17 bytes, got {}",
                        data.len()
                    )));
                }
                let mut arr = [0u8; 16];
                arr.copy_from_slice(&data[1..17]);
                Ok(Self::Uuid(arr))
            }
            DISC_STR => {
                if data.len() < 5 {
                    return Err(ExternalIdError::BadFormat("Str variant: too short".into()));
                }
                let len = u32::from_le_bytes([data[1], data[2], data[3], data[4]]) as usize;
                if data.len() != 5 + len {
                    return Err(ExternalIdError::BadFormat(format!(
                        "Str variant: length mismatch (expected {}, got {})",
                        5 + len,
                        data.len()
                    )));
                }
                let s = String::from_utf8(data[5..5 + len].to_vec()).map_err(|e| {
                    ExternalIdError::BadFormat(format!("Str variant: invalid UTF-8: {e}"))
                })?;
                Self::try_str(s)
            }
            DISC_BYTES => {
                if data.len() < 5 {
                    return Err(ExternalIdError::BadFormat(
                        "Bytes variant: too short".into(),
                    ));
                }
                let len = u32::from_le_bytes([data[1], data[2], data[3], data[4]]) as usize;
                if data.len() != 5 + len {
                    return Err(ExternalIdError::BadFormat(format!(
                        "Bytes variant: length mismatch (expected {}, got {})",
                        5 + len,
                        data.len()
                    )));
                }
                Self::try_bytes(data[5..5 + len].to_vec())
            }
            other => Err(ExternalIdError::BadFormat(format!(
                "unknown ExternalId discriminator 0x{other:02x}"
            ))),
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Display (canonical prefixed-string form)
// ──────────────────────────────────────────────────────────────────────────────

impl fmt::Display for ExternalId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExternalId::Hash { kind, bytes } => {
                write!(f, "{}:{}", kind.prefix(), hex::encode(bytes))
            }
            ExternalId::Uuid(b) => {
                // Format as 8-4-4-4-12 canonical UUID string.
                write!(
                    f,
                    "uuid:{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
                    u32::from_be_bytes([b[0], b[1], b[2], b[3]]),
                    u16::from_be_bytes([b[4], b[5]]),
                    u16::from_be_bytes([b[6], b[7]]),
                    u16::from_be_bytes([b[8], b[9]]),
                    // Last 6 bytes as u64 upper-aligned
                    ((b[10] as u64) << 40)
                        | ((b[11] as u64) << 32)
                        | ((b[12] as u64) << 24)
                        | ((b[13] as u64) << 16)
                        | ((b[14] as u64) << 8)
                        | b[15] as u64,
                )
            }
            ExternalId::Str(s) => write!(f, "str:{s}"),
            ExternalId::Bytes(b) => write!(f, "bytes:{}", hex::encode(b)),
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// FromStr (parse canonical prefixed-string form)
// ──────────────────────────────────────────────────────────────────────────────

impl FromStr for ExternalId {
    type Err = ExternalIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            return Err(ExternalIdError::Empty);
        }

        // Try hash prefixes first
        for (prefix, kind) in &[
            ("blake3:", HashKind::Blake3),
            ("sha256:", HashKind::Sha256),
            ("sha512:", HashKind::Sha512),
        ] {
            if let Some(hex_part) = s.strip_prefix(prefix) {
                let raw = hex::decode(hex_part).map_err(|e| {
                    ExternalIdError::BadFormat(format!("invalid hex in {prefix}: {e}"))
                })?;
                return Self::try_hash(*kind, raw);
            }
        }

        if let Some(uuid_part) = s.strip_prefix("uuid:") {
            // Parse canonical 8-4-4-4-12 form (hyphens optional but expected)
            let clean: String = uuid_part.chars().filter(|c| *c != '-').collect();
            if clean.len() != 32 {
                return Err(ExternalIdError::BadFormat(format!(
                    "uuid must be 32 hex digits (got {})",
                    clean.len()
                )));
            }
            let raw = hex::decode(&clean)
                .map_err(|e| ExternalIdError::BadFormat(format!("invalid uuid hex: {e}")))?;
            let mut arr = [0u8; 16];
            arr.copy_from_slice(&raw);
            return Self::try_uuid(arr);
        }

        if let Some(str_part) = s.strip_prefix("str:") {
            return Self::try_str(str_part.to_string());
        }

        if let Some(bytes_part) = s.strip_prefix("bytes:") {
            let raw = hex::decode(bytes_part)
                .map_err(|e| ExternalIdError::BadFormat(format!("invalid hex in bytes: {e}")))?;
            return Self::try_bytes(raw);
        }

        Err(ExternalIdError::BadFormat(format!(
            "unrecognised external-id prefix in {s:?}; \
             expected blake3:, sha256:, sha512:, uuid:, str:, or bytes:"
        )))
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Round-trip helpers ────────────────────────────────────────────────────

    fn round_trip_display(id: &ExternalId) {
        let s = id.to_string();
        let parsed: ExternalId = s.parse().expect("round-trip failed");
        assert_eq!(id, &parsed, "display round-trip mismatch for {s:?}");
    }

    fn round_trip_bytes(id: &ExternalId) {
        let encoded = id.to_bytes();
        let decoded = ExternalId::from_bytes(&encoded).expect("byte round-trip failed");
        assert_eq!(id, &decoded, "byte round-trip mismatch");
    }

    // ── Hash variant ──────────────────────────────────────────────────────────

    #[test]
    fn test_hash_blake3_round_trip() {
        let bytes = vec![0xABu8; 32];
        let id = ExternalId::try_hash(HashKind::Blake3, bytes).unwrap();
        round_trip_display(&id);
        round_trip_bytes(&id);
    }

    #[test]
    fn test_hash_sha256_round_trip() {
        let bytes = vec![0x01u8; 32];
        let id = ExternalId::try_hash(HashKind::Sha256, bytes).unwrap();
        round_trip_display(&id);
        round_trip_bytes(&id);
    }

    #[test]
    fn test_hash_sha512_round_trip() {
        let bytes = vec![0xFFu8; 64];
        let id = ExternalId::try_hash(HashKind::Sha512, bytes).unwrap();
        round_trip_display(&id);
        round_trip_bytes(&id);
    }

    #[test]
    fn test_hash_wire_discriminator() {
        let bytes = vec![0x00u8; 32];
        let id = ExternalId::try_hash(HashKind::Sha256, bytes).unwrap();
        let encoded = id.to_bytes();
        assert_eq!(encoded[0], 0x01, "Hash discriminator must be 0x01");
        assert_eq!(encoded[1], 0x02, "Sha256 sub-disc must be 0x02");
    }

    #[test]
    fn test_hash_wrong_length_rejected() {
        let err = ExternalId::try_hash(HashKind::Sha256, vec![0u8; 31]).unwrap_err();
        assert!(
            matches!(
                err,
                ExternalIdError::WrongHashLength {
                    kind: HashKind::Sha256,
                    expected: 32,
                    actual: 31
                }
            ),
            "unexpected error: {err:?}"
        );
    }

    #[test]
    fn test_hash_sha512_wrong_length_rejected() {
        let err = ExternalId::try_hash(HashKind::Sha512, vec![0u8; 32]).unwrap_err();
        assert!(
            matches!(
                err,
                ExternalIdError::WrongHashLength {
                    kind: HashKind::Sha512,
                    expected: 64,
                    actual: 32
                }
            ),
            "unexpected error: {err:?}"
        );
    }

    #[test]
    fn test_hash_empty_rejected() {
        let err = ExternalId::try_hash(HashKind::Blake3, vec![]).unwrap_err();
        assert_eq!(err, ExternalIdError::Empty);
    }

    // ── UUID variant ──────────────────────────────────────────────────────────

    #[test]
    fn test_uuid_round_trip() {
        let arr = [
            0x12, 0x3e, 0x45, 0x67, 0xe8, 0x9b, 0x12, 0xd3, 0xa4, 0x56, 0x42, 0x66, 0x14, 0x17,
            0x40, 0x00,
        ];
        let id = ExternalId::try_uuid(arr).unwrap();
        round_trip_display(&id);
        round_trip_bytes(&id);
    }

    #[test]
    fn test_uuid_wire_discriminator() {
        let id = ExternalId::try_uuid([0u8; 16]).unwrap();
        let encoded = id.to_bytes();
        assert_eq!(encoded[0], 0x02, "Uuid discriminator must be 0x02");
        assert_eq!(encoded.len(), 17);
    }

    #[test]
    fn test_uuid_display_format() {
        // nil UUID should display as 00000000-0000-0000-0000-000000000000
        let id = ExternalId::try_uuid([0u8; 16]).unwrap();
        assert_eq!(id.to_string(), "uuid:00000000-0000-0000-0000-000000000000");
    }

    // ── Str variant ───────────────────────────────────────────────────────────

    #[test]
    fn test_str_round_trip() {
        let id = ExternalId::try_str("hello/world".to_string()).unwrap();
        round_trip_display(&id);
        round_trip_bytes(&id);
    }

    #[test]
    fn test_str_wire_discriminator() {
        let id = ExternalId::try_str("x".to_string()).unwrap();
        let encoded = id.to_bytes();
        assert_eq!(encoded[0], 0x03, "Str discriminator must be 0x03");
    }

    #[test]
    fn test_str_too_long_rejected() {
        let s = "x".repeat(257);
        let err = ExternalId::try_str(s).unwrap_err();
        assert!(
            matches!(
                err,
                ExternalIdError::TooLong {
                    kind: "Str",
                    max: 256,
                    actual: 257
                }
            ),
            "unexpected error: {err:?}"
        );
    }

    #[test]
    fn test_str_empty_rejected() {
        let err = ExternalId::try_str(String::new()).unwrap_err();
        assert_eq!(err, ExternalIdError::Empty);
    }

    #[test]
    fn test_str_max_len_accepted() {
        let s = "a".repeat(256);
        assert!(ExternalId::try_str(s).is_ok());
    }

    // ── Bytes variant ─────────────────────────────────────────────────────────

    #[test]
    fn test_bytes_round_trip() {
        let id = ExternalId::try_bytes(vec![1, 2, 3, 4]).unwrap();
        round_trip_display(&id);
        round_trip_bytes(&id);
    }

    #[test]
    fn test_bytes_wire_discriminator() {
        let id = ExternalId::try_bytes(vec![0xFF]).unwrap();
        let encoded = id.to_bytes();
        assert_eq!(encoded[0], 0x04, "Bytes discriminator must be 0x04");
    }

    #[test]
    fn test_bytes_too_long_rejected() {
        let b = vec![0u8; 65];
        let err = ExternalId::try_bytes(b).unwrap_err();
        assert!(
            matches!(
                err,
                ExternalIdError::TooLong {
                    kind: "Bytes",
                    max: 64,
                    actual: 65
                }
            ),
            "unexpected error: {err:?}"
        );
    }

    #[test]
    fn test_bytes_empty_rejected() {
        let err = ExternalId::try_bytes(vec![]).unwrap_err();
        assert_eq!(err, ExternalIdError::Empty);
    }

    #[test]
    fn test_bytes_max_len_accepted() {
        let b = vec![0u8; 64];
        assert!(ExternalId::try_bytes(b).is_ok());
    }

    // ── FromStr ───────────────────────────────────────────────────────────────

    #[test]
    fn test_parse_blake3() {
        let hex = "a".repeat(64); // 32 bytes
        let s = format!("blake3:{hex}");
        let id: ExternalId = s.parse().unwrap();
        assert!(matches!(
            id,
            ExternalId::Hash {
                kind: HashKind::Blake3,
                ..
            }
        ));
    }

    #[test]
    fn test_parse_sha256() {
        let hex = "b".repeat(64);
        let s = format!("sha256:{hex}");
        let id: ExternalId = s.parse().unwrap();
        assert!(matches!(
            id,
            ExternalId::Hash {
                kind: HashKind::Sha256,
                ..
            }
        ));
    }

    #[test]
    fn test_parse_sha512() {
        let hex = "c".repeat(128); // 64 bytes
        let s = format!("sha512:{hex}");
        let id: ExternalId = s.parse().unwrap();
        assert!(matches!(
            id,
            ExternalId::Hash {
                kind: HashKind::Sha512,
                ..
            }
        ));
    }

    #[test]
    fn test_parse_uuid() {
        let s = "uuid:550e8400-e29b-41d4-a716-446655440000";
        let id: ExternalId = s.parse().unwrap();
        assert!(matches!(id, ExternalId::Uuid(_)));
    }

    #[test]
    fn test_parse_str() {
        let s = "str:hello world";
        let id: ExternalId = s.parse().unwrap();
        assert_eq!(id, ExternalId::Str("hello world".to_string()));
    }

    #[test]
    fn test_parse_bytes_hex() {
        let s = "bytes:deadbeef";
        let id: ExternalId = s.parse().unwrap();
        assert_eq!(id, ExternalId::Bytes(vec![0xDE, 0xAD, 0xBE, 0xEF]));
    }

    #[test]
    fn test_parse_unknown_prefix_error() {
        let err: ExternalIdError = "custom:xyz".parse::<ExternalId>().unwrap_err();
        assert!(matches!(err, ExternalIdError::BadFormat(_)));
    }

    #[test]
    fn test_parse_empty_error() {
        let err: ExternalIdError = "".parse::<ExternalId>().unwrap_err();
        assert_eq!(err, ExternalIdError::Empty);
    }

    // ── Table-driven coverage across all variants ─────────────────────────────

    #[test]
    fn test_all_variants_display_and_byte_round_trip() {
        let cases: Vec<ExternalId> = vec![
            ExternalId::try_hash(HashKind::Blake3, vec![0u8; 32]).unwrap(),
            ExternalId::try_hash(HashKind::Sha256, vec![1u8; 32]).unwrap(),
            ExternalId::try_hash(HashKind::Sha512, vec![2u8; 64]).unwrap(),
            ExternalId::try_uuid([3u8; 16]).unwrap(),
            ExternalId::try_str("test-key".to_string()).unwrap(),
            ExternalId::try_bytes(vec![4, 5, 6]).unwrap(),
        ];

        for case in &cases {
            round_trip_display(case);
            round_trip_bytes(case);
        }
    }
}
