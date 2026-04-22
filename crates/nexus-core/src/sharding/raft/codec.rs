//! Wire codec for the TCP Raft transport.
//!
//! Frame format (documented in the raft-consensus spec §Wire Format):
//!
//! ```text
//! ┌──────────────┬──────────────┬──────────────┬──────────────┬──────────────┐
//! │ shard_id: u32 LE │ msg_type: u8 │ length: u32 LE │ payload: N bytes │ crc32: u32 LE │
//! └──────────────┴──────────────┴──────────────┴──────────────┴──────────────┘
//! ```
//!
//! * `shard_id`: the target shard the message belongs to. Lets the receiver
//!   multiplex a single TCP connection across many shard groups if it
//!   wants to.
//! * `msg_type`: a single-byte tag. Raft frames use `0x40`; values below
//!   `0x40` are reserved for the legacy [`crate::replication::protocol`]
//!   framing and values above `0x7F` for future uses.
//! * `length`: payload length in bytes, excluding the 9-byte header and the
//!   4-byte trailing CRC.
//! * `payload`: bincode-encoded [`super::types::RaftEnvelope`]. The
//!   envelope itself carries `shard_id` + `from` + `message`; we still
//!   ship `shard_id` in the header so the reader can drop frames for
//!   unknown shards without deserializing.
//! * `crc32`: CRC32 of the first 9 + N bytes (header + payload). Mismatched
//!   frames are dropped silently; the Raft layer already tolerates message
//!   loss.
//!
//! The codec is deliberately async-free. The TCP transport in
//! [`super::tcp_transport`] plugs it into `tokio::io::Async{Read,Write}`
//! when it needs to move bytes across a socket; the pure codec stays
//! trivially testable against in-memory buffers.

use crc32fast::Hasher;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::types::RaftEnvelope;
use crate::sharding::metadata::ShardId;

/// Single-byte tag for the Raft frame variant. Picked outside the range
/// used by [`crate::replication::protocol::ReplicationMessageType`] so a
/// transport that mixes the two (unintended but possible) cannot confuse
/// them.
pub const RAFT_FRAME_TYPE: u8 = 0x40;

/// Fixed header size: 4 (shard_id) + 1 (type) + 4 (length).
pub const HEADER_LEN: usize = 9;

/// Trailing CRC size.
pub const CRC_LEN: usize = 4;

/// Upper bound on a single frame's payload. Anything larger probably
/// indicates corruption; we refuse to allocate gigabyte-sized buffers
/// from an attacker's whim. 8 MiB is ample — openraft-style snapshots
/// get their own chunked InstallSnapshot path.
pub const MAX_FRAME_PAYLOAD: usize = 8 * 1024 * 1024;

/// Errors surfaced by the codec.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum CodecError {
    /// The buffer was too small to contain even the header + CRC.
    #[error("frame too short: {0} bytes")]
    TooShort(usize),
    /// The frame's `msg_type` byte wasn't [`RAFT_FRAME_TYPE`].
    #[error("unknown frame type 0x{0:02x}")]
    UnknownType(u8),
    /// Payload length exceeded [`MAX_FRAME_PAYLOAD`].
    #[error("frame payload length {0} exceeds max {MAX_FRAME_PAYLOAD}")]
    PayloadTooLarge(u32),
    /// CRC32 stored in the frame disagreed with the one computed over
    /// the header + payload bytes.
    #[error("CRC mismatch: expected 0x{expected:08x}, computed 0x{computed:08x}")]
    CrcMismatch { expected: u32, computed: u32 },
    /// Bincode refused to (de)serialize the envelope.
    #[error("bincode error: {0}")]
    Bincode(String),
    /// The header's shard_id field did not match the envelope's
    /// internal `shard_id`. Rejected so a truncated or misdirected
    /// frame cannot silently land in the wrong shard's state machine.
    #[error("header shard_id {header:?} does not match payload shard_id {payload:?}")]
    ShardMismatch { header: ShardId, payload: ShardId },
}

/// Encode a [`RaftEnvelope`] into a framed byte vector ready to send.
///
/// Guarantees a single contiguous allocation and no intermediate copies
/// of the payload beyond bincode's own serialization buffer.
pub fn encode_frame(env: &RaftEnvelope) -> Result<Vec<u8>, CodecError> {
    let payload = bincode::serialize(env).map_err(|e| CodecError::Bincode(e.to_string()))?;
    if payload.len() > MAX_FRAME_PAYLOAD {
        return Err(CodecError::PayloadTooLarge(payload.len() as u32));
    }
    let mut buf = Vec::with_capacity(HEADER_LEN + payload.len() + CRC_LEN);
    buf.extend_from_slice(&env.shard_id.as_u32().to_le_bytes());
    buf.push(RAFT_FRAME_TYPE);
    buf.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    buf.extend_from_slice(&payload);
    let mut h = Hasher::new();
    h.update(&buf);
    buf.extend_from_slice(&h.finalize().to_le_bytes());
    Ok(buf)
}

/// Parsed header — produced by [`decode_header`], consumed by
/// [`decode_frame_body`]. Separating the two halves lets the async
/// transport read the fixed-size header first, allocate the right
/// payload buffer, and only then read the rest — instead of reading one
/// byte at a time or an oversized chunk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameHeader {
    /// Shard id field, validated to be consistent with the envelope's
    /// own shard_id in [`decode_frame_body`].
    pub shard_id: ShardId,
    /// Payload length (excludes header + CRC).
    pub payload_len: u32,
}

impl FrameHeader {
    /// Total bytes the caller still needs to read: `payload_len` + 4 CRC
    /// bytes.
    #[inline]
    #[must_use]
    pub fn remaining(self) -> usize {
        self.payload_len as usize + CRC_LEN
    }
}

/// Parse the 9-byte fixed header. Returns `Err` on unknown type or
/// oversized payload length (no allocation performed).
pub fn decode_header(bytes: &[u8]) -> Result<FrameHeader, CodecError> {
    if bytes.len() < HEADER_LEN {
        return Err(CodecError::TooShort(bytes.len()));
    }
    let shard_id = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    let msg_type = bytes[4];
    if msg_type != RAFT_FRAME_TYPE {
        return Err(CodecError::UnknownType(msg_type));
    }
    let payload_len = u32::from_le_bytes([bytes[5], bytes[6], bytes[7], bytes[8]]);
    if payload_len as usize > MAX_FRAME_PAYLOAD {
        return Err(CodecError::PayloadTooLarge(payload_len));
    }
    Ok(FrameHeader {
        shard_id: ShardId::new(shard_id),
        payload_len,
    })
}

/// Parse the payload + CRC suffix against the already-decoded header.
/// `header_bytes` is the raw header the caller already consumed (9 bytes
/// exactly); `rest` is `payload_len + 4` bytes immediately after.
/// Re-computes the CRC over header + payload and rejects mismatches.
pub fn decode_frame_body(
    header: FrameHeader,
    header_bytes: &[u8; HEADER_LEN],
    rest: &[u8],
) -> Result<RaftEnvelope, CodecError> {
    let payload_len = header.payload_len as usize;
    if rest.len() < payload_len + CRC_LEN {
        return Err(CodecError::TooShort(rest.len()));
    }
    let payload = &rest[..payload_len];
    let crc_bytes = &rest[payload_len..payload_len + CRC_LEN];
    let expected = u32::from_le_bytes([crc_bytes[0], crc_bytes[1], crc_bytes[2], crc_bytes[3]]);
    let mut h = Hasher::new();
    h.update(header_bytes);
    h.update(payload);
    let computed = h.finalize();
    if expected != computed {
        return Err(CodecError::CrcMismatch { expected, computed });
    }
    let env: RaftEnvelope =
        bincode::deserialize(payload).map_err(|e| CodecError::Bincode(e.to_string()))?;
    if env.shard_id != header.shard_id {
        return Err(CodecError::ShardMismatch {
            header: header.shard_id,
            payload: env.shard_id,
        });
    }
    Ok(env)
}

/// One-shot decode from a flat buffer. Convenience for tests.
pub fn decode_frame(bytes: &[u8]) -> Result<RaftEnvelope, CodecError> {
    if bytes.len() < HEADER_LEN + CRC_LEN {
        return Err(CodecError::TooShort(bytes.len()));
    }
    let mut header_arr = [0u8; HEADER_LEN];
    header_arr.copy_from_slice(&bytes[..HEADER_LEN]);
    let header = decode_header(&header_arr)?;
    decode_frame_body(header, &header_arr, &bytes[HEADER_LEN..])
}

// ---------------------------------------------------------------------------
// AsyncRead / AsyncWrite adapters
// ---------------------------------------------------------------------------

/// Write a framed envelope onto an async writer.
pub async fn write_frame<W>(writer: &mut W, env: &RaftEnvelope) -> Result<(), FrameIoError>
where
    W: tokio::io::AsyncWrite + Unpin + ?Sized,
{
    use tokio::io::AsyncWriteExt;
    let buf = encode_frame(env)?;
    writer.write_all(&buf).await?;
    writer.flush().await?;
    Ok(())
}

/// Read one framed envelope from an async reader.
pub async fn read_frame<R>(reader: &mut R) -> Result<RaftEnvelope, FrameIoError>
where
    R: tokio::io::AsyncRead + Unpin + ?Sized,
{
    use tokio::io::AsyncReadExt;
    let mut header_buf = [0u8; HEADER_LEN];
    reader.read_exact(&mut header_buf).await?;
    let header = decode_header(&header_buf)?;
    let mut rest = vec![0u8; header.remaining()];
    reader.read_exact(&mut rest).await?;
    Ok(decode_frame_body(header, &header_buf, &rest)?)
}

/// Errors from the async read/write wrappers.
#[derive(Debug, Error)]
pub enum FrameIoError {
    /// Underlying I/O failed (connection closed, timeout, …).
    #[error("I/O error: {0}")]
    Io(std::io::Error),
    /// The bytes flowing through decoded to a malformed frame. The
    /// transport layer treats these the same as dropped packets —
    /// logged, then the connection is recycled.
    #[error("codec error: {0}")]
    Codec(CodecError),
}

impl From<CodecError> for FrameIoError {
    fn from(err: CodecError) -> Self {
        Self::Codec(err)
    }
}

impl From<std::io::Error> for FrameIoError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

// Private helper so the internal framing constants can be `pub(crate)`
// tested without leaking via the public API.
#[derive(Debug, Serialize, Deserialize)]
#[allow(dead_code)]
struct _CompileProbe<T> {
    _t: std::marker::PhantomData<T>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sharding::metadata::NodeId;
    use crate::sharding::raft::types::{LogIndex, RaftMessage, Term, VoteRequest};

    fn sample_env(shard: u32) -> RaftEnvelope {
        RaftEnvelope {
            shard_id: ShardId::new(shard),
            from: NodeId::new("node-a").unwrap(),
            message: RaftMessage::RequestVote(VoteRequest {
                term: Term(7),
                candidate: NodeId::new("node-a").unwrap(),
                last_log_index: LogIndex(42),
                last_log_term: Term(6),
            }),
        }
    }

    #[test]
    fn roundtrip_preserves_envelope() {
        let env = sample_env(2);
        let bytes = encode_frame(&env).unwrap();
        let back = decode_frame(&bytes).unwrap();
        assert_eq!(env, back);
    }

    #[test]
    fn header_fields_match_wire_layout() {
        let env = sample_env(0x01020304);
        let bytes = encode_frame(&env).unwrap();
        // shard_id LE at bytes 0..4.
        assert_eq!(bytes[0..4], [0x04, 0x03, 0x02, 0x01]);
        // msg_type at byte 4.
        assert_eq!(bytes[4], RAFT_FRAME_TYPE);
        // length at bytes 5..9 (LE).
        let declared = u32::from_le_bytes([bytes[5], bytes[6], bytes[7], bytes[8]]);
        assert_eq!(declared as usize, bytes.len() - HEADER_LEN - CRC_LEN);
    }

    #[test]
    fn decode_rejects_short_buffer() {
        let err = decode_frame(&[0u8; 4]).unwrap_err();
        assert!(matches!(err, CodecError::TooShort(_)));
    }

    #[test]
    fn decode_rejects_unknown_type() {
        let env = sample_env(0);
        let mut bytes = encode_frame(&env).unwrap();
        bytes[4] = 0x99; // corrupt type
        // CRC will also disagree but UnknownType fires first in decode_header.
        let err = decode_header(&bytes).unwrap_err();
        assert_eq!(err, CodecError::UnknownType(0x99));
    }

    #[test]
    fn decode_detects_crc_corruption() {
        let env = sample_env(0);
        let mut bytes = encode_frame(&env).unwrap();
        // Flip one bit in the payload; CRC should catch it.
        let tamper = HEADER_LEN + 1;
        bytes[tamper] ^= 0x01;
        let err = decode_frame(&bytes).unwrap_err();
        assert!(matches!(err, CodecError::CrcMismatch { .. }));
    }

    #[test]
    fn decode_detects_shard_mismatch_between_header_and_payload() {
        let env = sample_env(3);
        let mut bytes = encode_frame(&env).unwrap();
        // Rewrite header shard_id to something else but keep CRC valid
        // by recomputing it.
        bytes[0..4].copy_from_slice(&7u32.to_le_bytes());
        let crc_off = bytes.len() - CRC_LEN;
        let mut h = Hasher::new();
        h.update(&bytes[..crc_off]);
        let crc = h.finalize();
        bytes[crc_off..].copy_from_slice(&crc.to_le_bytes());
        let err = decode_frame(&bytes).unwrap_err();
        assert!(
            matches!(err, CodecError::ShardMismatch { .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn payload_too_large_is_rejected() {
        // Construct a header declaring a payload larger than the cap.
        let mut header = [0u8; HEADER_LEN];
        header[4] = RAFT_FRAME_TYPE;
        let oversized = (MAX_FRAME_PAYLOAD as u32) + 1;
        header[5..9].copy_from_slice(&oversized.to_le_bytes());
        let err = decode_header(&header).unwrap_err();
        assert!(matches!(err, CodecError::PayloadTooLarge(_)));
    }

    #[test]
    fn header_remaining_accounts_for_crc() {
        let h = FrameHeader {
            shard_id: ShardId::new(0),
            payload_len: 100,
        };
        assert_eq!(h.remaining(), 104);
    }

    #[test]
    fn different_shards_produce_different_headers() {
        let a = encode_frame(&sample_env(0)).unwrap();
        let b = encode_frame(&sample_env(1)).unwrap();
        assert_ne!(a[0..4], b[0..4]);
    }

    #[test]
    fn async_roundtrip_through_inmemory_pipe() {
        use tokio::io::duplex;
        use tokio::runtime::Runtime;

        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let (mut client, mut server) = duplex(1024);
            let env = sample_env(5);
            let env_clone = env.clone();
            tokio::spawn(async move {
                write_frame(&mut client, &env_clone).await.unwrap();
            });
            let got = read_frame(&mut server).await.unwrap();
            assert_eq!(got, env);
        });
    }

    #[test]
    fn async_read_surfaces_io_error_on_eof() {
        use tokio::io::duplex;
        use tokio::runtime::Runtime;

        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let (client, mut server) = duplex(64);
            drop(client); // simulate peer disconnect
            let err = read_frame(&mut server).await.unwrap_err();
            assert!(matches!(err, FrameIoError::Io(_)));
        });
    }

    #[test]
    fn async_read_surfaces_codec_error_on_bad_header() {
        use tokio::io::{AsyncWriteExt, duplex};
        use tokio::runtime::Runtime;

        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let (mut client, mut server) = duplex(64);
            // Send a header with wrong message type.
            let mut bad = [0u8; HEADER_LEN];
            bad[4] = 0x99;
            client.write_all(&bad).await.unwrap();
            client.flush().await.unwrap();
            let err = read_frame(&mut server).await.unwrap_err();
            assert!(matches!(
                err,
                FrameIoError::Codec(CodecError::UnknownType(0x99))
            ));
        });
    }
}
