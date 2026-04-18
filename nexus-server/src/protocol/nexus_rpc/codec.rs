//! NexusRPC length-prefixed MessagePack codec.
//!
//! Wire format:
//! ```text
//! ┌───────────────────┬──────────────────────────┐
//! │  length: u32 (LE) │  body: MessagePack bytes  │
//! └───────────────────┴──────────────────────────┘
//!     4 bytes              length bytes
//! ```
//!
//! Both [`Request`] and [`Response`] frames share this format. The codec is
//! the only entry point the accept loop needs — everything above it works on
//! typed frames.

use serde::{Deserialize, Serialize};
use std::io;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use super::DEFAULT_MAX_FRAME_BYTES;
use super::types::{Request, Response};

/// Errors produced by the sync decoder.
#[derive(Debug, thiserror::Error)]
pub enum DecodeError {
    /// The length prefix declared a body larger than the caller's cap.
    #[error("frame body {body} bytes exceeds limit {max} bytes")]
    FrameTooLarge { body: usize, max: usize },
    /// The body parsed as a well-formed length-prefix frame but the
    /// MessagePack payload was malformed.
    #[error("decode error: {0}")]
    Rmp(#[from] rmp_serde::decode::Error),
}

// ── Sync frame helpers ──────────────────────────────────────────────────────

/// Encode any serializable message into a length-prefixed MessagePack frame.
pub fn encode_frame<T: Serialize>(msg: &T) -> Result<Vec<u8>, rmp_serde::encode::Error> {
    let body = rmp_serde::to_vec(msg)?;
    let len = body.len() as u32;
    let mut frame = Vec::with_capacity(4 + body.len());
    frame.extend_from_slice(&len.to_le_bytes());
    frame.extend_from_slice(&body);
    Ok(frame)
}

/// Decode one frame from a byte slice using the default 64 MiB cap.
///
/// Returns `Ok(None)` when the buffer does not yet contain a complete frame
/// (the caller should read more bytes and retry). Returns
/// [`DecodeError::FrameTooLarge`] if the length prefix alone already exceeds
/// the default cap — the body is never allocated in that case.
pub fn decode_frame<T: for<'de> Deserialize<'de>>(
    buf: &[u8],
) -> Result<Option<(T, usize)>, DecodeError> {
    decode_frame_with_limit(buf, DEFAULT_MAX_FRAME_BYTES)
}

/// Decode one frame from a byte slice, rejecting bodies larger than `max`.
pub fn decode_frame_with_limit<T: for<'de> Deserialize<'de>>(
    buf: &[u8],
    max: usize,
) -> Result<Option<(T, usize)>, DecodeError> {
    if buf.len() < 4 {
        return Ok(None);
    }
    let len = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;
    if len > max {
        return Err(DecodeError::FrameTooLarge { body: len, max });
    }
    let total = 4 + len;
    if buf.len() < total {
        return Ok(None);
    }
    let value = rmp_serde::from_slice(&buf[4..total])?;
    Ok(Some((value, total)))
}

// ── Async frame helpers ──────────────────────────────────────────────────────

/// Read one [`Request`] frame from an async reader using the default cap.
pub async fn read_request<R: AsyncRead + Unpin>(reader: &mut R) -> io::Result<Request> {
    read_frame(reader, DEFAULT_MAX_FRAME_BYTES).await
}

/// Read one [`Request`] frame with a caller-supplied cap (use this on the
/// server hot path so operators can tune `rpc.max_frame_bytes`).
pub async fn read_request_with_limit<R: AsyncRead + Unpin>(
    reader: &mut R,
    max: usize,
) -> io::Result<Request> {
    read_frame(reader, max).await
}

/// Read one [`Response`] frame from an async reader using the default cap.
pub async fn read_response<R: AsyncRead + Unpin>(reader: &mut R) -> io::Result<Response> {
    read_frame(reader, DEFAULT_MAX_FRAME_BYTES).await
}

/// Write a [`Request`] frame to an async writer.
pub async fn write_request<W: AsyncWrite + Unpin>(writer: &mut W, req: &Request) -> io::Result<()> {
    write_frame(writer, req).await
}

/// Write a [`Response`] frame to an async writer.
pub async fn write_response<W: AsyncWrite + Unpin>(
    writer: &mut W,
    resp: &Response,
) -> io::Result<()> {
    write_frame(writer, resp).await
}

async fn read_frame<T: for<'de> Deserialize<'de>, R: AsyncRead + Unpin>(
    reader: &mut R,
    max: usize,
) -> io::Result<T> {
    let mut len_buf = [0u8; 4];
    reader.read_exact(&mut len_buf).await?;
    let len = u32::from_le_bytes(len_buf) as usize;
    if len > max {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("frame body {len} bytes exceeds limit {max} bytes"),
        ));
    }
    let mut body = vec![0u8; len];
    reader.read_exact(&mut body).await?;
    rmp_serde::from_slice(&body)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))
}

async fn write_frame<T: Serialize, W: AsyncWrite + Unpin>(
    writer: &mut W,
    msg: &T,
) -> io::Result<()> {
    let frame =
        encode_frame(msg).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
    writer.write_all(&frame).await
}

#[cfg(test)]
mod tests {
    use super::super::types::NexusValue;
    use super::*;
    use tokio::io::BufReader;

    fn sample_request() -> Request {
        Request {
            id: 1,
            command: "CYPHER".into(),
            args: vec![
                NexusValue::Str("RETURN 1".into()),
                NexusValue::Bytes(vec![1, 2, 3]),
            ],
        }
    }

    #[test]
    fn encode_decode_roundtrip_request() {
        let req = sample_request();
        let frame = encode_frame(&req).unwrap();
        let len = u32::from_le_bytes([frame[0], frame[1], frame[2], frame[3]]) as usize;
        assert_eq!(len + 4, frame.len());

        let (decoded, consumed): (Request, usize) = decode_frame(&frame).unwrap().unwrap();
        assert_eq!(consumed, frame.len());
        assert_eq!(decoded.id, req.id);
        assert_eq!(decoded.command, req.command);
        assert_eq!(decoded.args.len(), 2);
    }

    #[test]
    fn encode_decode_roundtrip_response() {
        let resp = Response::ok(9, NexusValue::Int(123));
        let frame = encode_frame(&resp).unwrap();
        let (decoded, _): (Response, usize) = decode_frame(&frame).unwrap().unwrap();
        assert_eq!(decoded.id, 9);
        assert_eq!(decoded.result.ok(), Some(NexusValue::Int(123)));
    }

    #[test]
    fn decode_returns_none_on_partial_header() {
        let r: Result<Option<(Request, usize)>, _> = decode_frame(&[0, 0]);
        assert!(r.unwrap().is_none());
        let r: Result<Option<(Request, usize)>, _> = decode_frame(&[]);
        assert!(r.unwrap().is_none());
    }

    #[test]
    fn decode_returns_none_on_partial_body() {
        let req = Request {
            id: 99,
            command: "PING".into(),
            args: vec![],
        };
        let mut frame = encode_frame(&req).unwrap();
        frame.truncate(frame.len() - 1);
        let r: Result<Option<(Request, usize)>, _> = decode_frame(&frame);
        assert!(r.unwrap().is_none());
    }

    #[test]
    fn decode_consumes_exactly_one_frame_from_stream() {
        // Two frames concatenated — decode_frame must only eat the first.
        let a = encode_frame(&Request {
            id: 1,
            command: "A".into(),
            args: vec![],
        })
        .unwrap();
        let b = encode_frame(&Request {
            id: 2,
            command: "B".into(),
            args: vec![],
        })
        .unwrap();
        let mut stream = Vec::new();
        stream.extend_from_slice(&a);
        stream.extend_from_slice(&b);

        let (r1, consumed): (Request, usize) = decode_frame(&stream).unwrap().unwrap();
        assert_eq!(r1.id, 1);
        assert_eq!(consumed, a.len());

        let (r2, _): (Request, usize) = decode_frame(&stream[consumed..]).unwrap().unwrap();
        assert_eq!(r2.id, 2);
    }

    #[test]
    fn decode_frame_rejects_oversized_bodies() {
        // Craft a length prefix that exceeds a tiny limit; the body bytes
        // that follow never even need to exist — the length check must fire
        // before allocation.
        let big_len: u32 = 100;
        let mut frame = big_len.to_le_bytes().to_vec();
        frame.extend_from_slice(&[0u8; 10]); // body is truncated; limit hits first

        let err = decode_frame_with_limit::<Request>(&frame, 32).unwrap_err();
        match err {
            DecodeError::FrameTooLarge { body, max } => {
                assert_eq!(body, 100);
                assert_eq!(max, 32);
            }
            other => panic!("expected FrameTooLarge, got {other:?}"),
        }
    }

    #[test]
    fn decode_frame_returns_rmp_error_on_garbage_body() {
        // 4-byte length prefix of 3, followed by 3 bytes of invalid MessagePack.
        let frame = [3u8, 0, 0, 0, 0xc1, 0xc1, 0xc1]; // 0xc1 is a reserved byte
        let err = decode_frame::<Request>(&frame).unwrap_err();
        matches!(err, DecodeError::Rmp(_));
    }

    #[tokio::test]
    async fn async_write_read_roundtrip_request() {
        let req = sample_request();
        let mut buf = Vec::new();
        write_request(&mut buf, &req).await.unwrap();

        let mut cursor = BufReader::new(std::io::Cursor::new(buf));
        let decoded = read_request(&mut cursor).await.unwrap();
        assert_eq!(decoded.id, req.id);
        assert_eq!(decoded.command, req.command);
    }

    #[tokio::test]
    async fn async_write_read_roundtrip_response() {
        let resp = Response::err(3, "boom");
        let mut buf = Vec::new();
        write_response(&mut buf, &resp).await.unwrap();

        let mut cursor = BufReader::new(std::io::Cursor::new(buf));
        let decoded = read_response(&mut cursor).await.unwrap();
        assert_eq!(decoded.id, 3);
        assert_eq!(decoded.result.err().as_deref(), Some("boom"));
    }

    #[tokio::test]
    async fn async_read_rejects_oversized_frame() {
        // Declare a 10 KiB body but cap the reader at 1 KiB.
        let mut frame: Vec<u8> = 10_000u32.to_le_bytes().to_vec();
        frame.extend_from_slice(&[0u8; 64]); // no need to fill — limit check fires first
        let mut cursor = BufReader::new(std::io::Cursor::new(frame));
        let err = read_request_with_limit(&mut cursor, 1024)
            .await
            .unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        assert!(err.to_string().contains("exceeds limit"));
    }

    #[tokio::test]
    async fn async_read_errors_on_garbage_body() {
        // 3-byte body of invalid MessagePack.
        let mut frame = 3u32.to_le_bytes().to_vec();
        frame.extend_from_slice(&[0xc1, 0xc1, 0xc1]);
        let mut cursor = BufReader::new(std::io::Cursor::new(frame));
        let err = read_request(&mut cursor).await.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }
}
