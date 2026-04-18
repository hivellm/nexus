//! RESP3 writer — serialises `Resp3Value` back to the wire.
//!
//! Every frame is written through a `BufWriter` so we get a single
//! `write_all` syscall per framed response. A `bytes_written` counter rides
//! on the side so the server can attribute wire bytes to a Prometheus
//! counter without reaching into tokio internals.
//!
//! When the connecting client negotiated RESP2 (via `HELLO 2`), RESP3-only
//! types degrade gracefully: `Null` becomes `$-1`, `Double` becomes a bulk
//! string, `Boolean` becomes `:0`/`:1`, `Map` becomes a flat `Array`, and
//! `Set` also becomes `Array`. `Verbatim` and `BigNumber` lower to bulk
//! strings.

use std::fmt::Write as _;

use tokio::io::{AsyncWrite, AsyncWriteExt, BufWriter};

use super::parser::Resp3Value;

/// Mirrors `ParseError` but for the write side.
#[derive(Debug)]
pub enum WriteError {
    Io(std::io::Error),
}

impl std::fmt::Display for WriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WriteError::Io(e) => write!(f, "RESP3 write I/O error: {e}"),
        }
    }
}

impl std::error::Error for WriteError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            WriteError::Io(e) => Some(e),
        }
    }
}

impl From<std::io::Error> for WriteError {
    fn from(e: std::io::Error) -> Self {
        WriteError::Io(e)
    }
}

/// Protocol variant a specific TCP connection has negotiated via HELLO.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProtocolVersion {
    /// Legacy RESP2 — default when the peer never sent HELLO. RESP3-only
    /// types are lowered to their closest RESP2 equivalent before framing.
    Resp2,
    /// Full RESP3. This is what we negotiate in response to `HELLO 3`.
    #[default]
    Resp3,
}

/// Framed writer over any `AsyncWrite`.
///
/// The `Send` bound is required because `encode` returns a
/// `Pin<Box<dyn Future<Output = _> + Send + '_>>` — RESP3 frames can nest
/// (Array of Maps of Arrays …) so the recursive boxed future needs to be
/// `Send` to cross `.await` points inside a multi-threaded tokio runtime.
pub struct Resp3Writer<W: AsyncWrite + Unpin + Send> {
    inner: BufWriter<W>,
    bytes_written: u64,
    protocol: ProtocolVersion,
}

impl<W: AsyncWrite + Unpin + Send> Resp3Writer<W> {
    /// Wrap an `AsyncWrite` in a buffered RESP3 writer. The default
    /// protocol is RESP3; downgrade via [`Self::set_protocol`] when the
    /// client issues `HELLO 2`.
    pub fn new(inner: W) -> Self {
        Self {
            inner: BufWriter::new(inner),
            bytes_written: 0,
            protocol: ProtocolVersion::Resp3,
        }
    }

    /// Return the number of bytes this writer has sent on the wire since it
    /// was created, *including* framing overhead. Used by the server loop
    /// to bump `nexus_resp3_bytes_written_total`.
    pub fn bytes_written(&self) -> u64 {
        self.bytes_written
    }

    /// Negotiate the protocol variant to emit. Cheap — just flips a flag.
    pub fn set_protocol(&mut self, protocol: ProtocolVersion) {
        self.protocol = protocol;
    }

    /// Write a full `Resp3Value` subtree to the stream. Does **not** flush.
    /// Every framing primitive goes through a single `write_all` to avoid
    /// a half-written frame from being observable after a mid-write error.
    pub async fn write(&mut self, value: &Resp3Value) -> Result<(), WriteError> {
        self.encode(value).await
    }

    /// Convenience: `+OK\r\n`.
    pub async fn write_ok(&mut self) -> Result<(), WriteError> {
        self.encode(&Resp3Value::SimpleString("OK".into())).await
    }

    /// Convenience: `-ERR <msg>\r\n`.
    pub async fn write_error<S: Into<String>>(&mut self, msg: S) -> Result<(), WriteError> {
        let text = msg.into();
        let body = if text.starts_with("ERR ")
            || text.starts_with("WRONGPASS")
            || text.starts_with("NOAUTH")
        {
            text
        } else {
            format!("ERR {text}")
        };
        self.encode(&Resp3Value::Error(body)).await
    }

    /// Convenience: `-NOAUTH Authentication required.\r\n`.
    pub async fn write_noauth(&mut self) -> Result<(), WriteError> {
        self.encode(&Resp3Value::Error("NOAUTH Authentication required.".into()))
            .await
    }

    /// Convenience: `:<n>\r\n`.
    pub async fn write_integer(&mut self, n: i64) -> Result<(), WriteError> {
        self.encode(&Resp3Value::Integer(n)).await
    }

    /// Convenience: `$<len>\r\n<bytes>\r\n`.
    pub async fn write_bulk<B: AsRef<[u8]>>(&mut self, bytes: B) -> Result<(), WriteError> {
        self.encode(&Resp3Value::BulkString(bytes.as_ref().to_vec()))
            .await
    }

    /// Convenience: RESP3 `_\r\n` (or RESP2 `$-1\r\n`).
    pub async fn write_null(&mut self) -> Result<(), WriteError> {
        self.encode(&Resp3Value::Null).await
    }

    /// Convenience: `%<n>\r\n` (or RESP2 flat array).
    pub async fn write_map(
        &mut self,
        entries: Vec<(Resp3Value, Resp3Value)>,
    ) -> Result<(), WriteError> {
        self.encode(&Resp3Value::Map(entries)).await
    }

    /// Flush the internal buffer to the underlying stream.
    pub async fn flush(&mut self) -> Result<(), WriteError> {
        self.inner.flush().await?;
        Ok(())
    }

    // --------------------------------------------------------------------
    // Recursive encoder.
    // --------------------------------------------------------------------

    fn encode<'a>(
        &'a mut self,
        value: &'a Resp3Value,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), WriteError>> + Send + 'a>>
    {
        // Boxed for recursion (Array, Set, Map).
        Box::pin(async move {
            match value {
                Resp3Value::SimpleString(s) => self.raw(format!("+{s}\r\n").as_bytes()).await,
                Resp3Value::Error(s) => self.raw(format!("-{s}\r\n").as_bytes()).await,
                Resp3Value::Integer(n) => self.raw(format!(":{n}\r\n").as_bytes()).await,
                Resp3Value::BulkString(b) => {
                    let mut header = String::with_capacity(16);
                    let _ = write!(header, "${}\r\n", b.len());
                    self.raw(header.as_bytes()).await?;
                    self.raw(b).await?;
                    self.raw(b"\r\n").await
                }
                Resp3Value::Array(items) => {
                    let header = format!("*{}\r\n", items.len());
                    self.raw(header.as_bytes()).await?;
                    for item in items {
                        self.encode(item).await?;
                    }
                    Ok(())
                }
                Resp3Value::Null => match self.protocol {
                    ProtocolVersion::Resp3 => self.raw(b"_\r\n").await,
                    ProtocolVersion::Resp2 => self.raw(b"$-1\r\n").await,
                },
                Resp3Value::Double(n) => match self.protocol {
                    ProtocolVersion::Resp3 => self.raw(format_double(*n).as_bytes()).await,
                    ProtocolVersion::Resp2 => {
                        // Degrade to BulkString for RESP2.
                        let s = format!("{n}");
                        self.encode(&Resp3Value::bulk(s)).await
                    }
                },
                Resp3Value::Boolean(b) => match self.protocol {
                    ProtocolVersion::Resp3 => {
                        self.raw(if *b { b"#t\r\n" } else { b"#f\r\n" }).await
                    }
                    ProtocolVersion::Resp2 => {
                        self.encode(&Resp3Value::Integer(if *b { 1 } else { 0 }))
                            .await
                    }
                },
                Resp3Value::Verbatim(fmt_tag, body) => match self.protocol {
                    ProtocolVersion::Resp3 => {
                        // Body format: "<fmt>:<data>" — length includes the
                        // 3-byte format tag and the colon.
                        let total_len = 3 + 1 + body.len();
                        let header = format!("={total_len}\r\n{fmt_tag}:");
                        self.raw(header.as_bytes()).await?;
                        self.raw(body).await?;
                        self.raw(b"\r\n").await
                    }
                    ProtocolVersion::Resp2 => {
                        // Lower to BulkString (drops the format tag).
                        self.encode(&Resp3Value::BulkString(body.clone())).await
                    }
                },
                Resp3Value::Set(items) => match self.protocol {
                    ProtocolVersion::Resp3 => {
                        let header = format!("~{}\r\n", items.len());
                        self.raw(header.as_bytes()).await?;
                        for item in items {
                            self.encode(item).await?;
                        }
                        Ok(())
                    }
                    ProtocolVersion::Resp2 => self.encode(&Resp3Value::Array(items.clone())).await,
                },
                Resp3Value::Map(entries) => match self.protocol {
                    ProtocolVersion::Resp3 => {
                        let header = format!("%{}\r\n", entries.len());
                        self.raw(header.as_bytes()).await?;
                        for (k, v) in entries {
                            self.encode(k).await?;
                            self.encode(v).await?;
                        }
                        Ok(())
                    }
                    ProtocolVersion::Resp2 => {
                        let mut flat = Vec::with_capacity(entries.len() * 2);
                        for (k, v) in entries {
                            flat.push(k.clone());
                            flat.push(v.clone());
                        }
                        self.encode(&Resp3Value::Array(flat)).await
                    }
                },
                Resp3Value::BigNumber(s) => match self.protocol {
                    ProtocolVersion::Resp3 => self.raw(format!("({s}\r\n").as_bytes()).await,
                    ProtocolVersion::Resp2 => self.encode(&Resp3Value::bulk(s.clone())).await,
                },
            }
        })
    }

    async fn raw(&mut self, bytes: &[u8]) -> Result<(), WriteError> {
        self.inner.write_all(bytes).await?;
        self.bytes_written += bytes.len() as u64;
        Ok(())
    }
}

/// Format a RESP3 Double. The spec mandates `inf`/`-inf`/`nan` for the
/// specials and a bare decimal representation otherwise (no leading `+`).
fn format_double(n: f64) -> String {
    if n.is_nan() {
        ",nan\r\n".to_string()
    } else if n.is_infinite() {
        if n.is_sign_negative() {
            ",-inf\r\n".to_string()
        } else {
            ",inf\r\n".to_string()
        }
    } else {
        // Rust's default `{}` for f64 already produces a lossless decimal
        // representation; good enough for a wire encoding.
        format!(",{n}\r\n")
    }
}

// --------------------------------------------------------------------------
// Tests.
// --------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    async fn encode(value: Resp3Value) -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut w = Resp3Writer::new(&mut buf);
            w.write(&value).await.unwrap();
            w.flush().await.unwrap();
        }
        buf
    }

    async fn encode_resp2(value: Resp3Value) -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut w = Resp3Writer::new(&mut buf);
            w.set_protocol(ProtocolVersion::Resp2);
            w.write(&value).await.unwrap();
            w.flush().await.unwrap();
        }
        buf
    }

    #[tokio::test]
    async fn simple_string_frame() {
        assert_eq!(
            encode(Resp3Value::SimpleString("OK".into())).await,
            b"+OK\r\n"
        );
    }

    #[tokio::test]
    async fn error_frame() {
        assert_eq!(
            encode(Resp3Value::Error("ERR bad".into())).await,
            b"-ERR bad\r\n"
        );
    }

    #[tokio::test]
    async fn integer_frame() {
        assert_eq!(encode(Resp3Value::Integer(42)).await, b":42\r\n");
        assert_eq!(encode(Resp3Value::Integer(-7)).await, b":-7\r\n");
    }

    #[tokio::test]
    async fn bulk_string_frame_is_binary_safe() {
        let bytes = vec![0x00, 0xff, b'h', b'i'];
        let out = encode(Resp3Value::BulkString(bytes.clone())).await;
        assert_eq!(&out[..5], b"$4\r\n\x00");
        assert_eq!(&out[5..], b"\xffhi\r\n");
    }

    #[tokio::test]
    async fn array_frame() {
        let out = encode(Resp3Value::Array(vec![
            Resp3Value::Integer(1),
            Resp3Value::bulk("hi"),
        ]))
        .await;
        assert_eq!(out, b"*2\r\n:1\r\n$2\r\nhi\r\n");
    }

    #[tokio::test]
    async fn null_resp3_vs_resp2() {
        assert_eq!(encode(Resp3Value::Null).await, b"_\r\n");
        assert_eq!(encode_resp2(Resp3Value::Null).await, b"$-1\r\n");
    }

    #[tokio::test]
    async fn double_regular_and_specials() {
        assert_eq!(encode(Resp3Value::Double(3.14)).await, b",3.14\r\n");
        assert_eq!(encode(Resp3Value::Double(f64::INFINITY)).await, b",inf\r\n");
        assert_eq!(
            encode(Resp3Value::Double(f64::NEG_INFINITY)).await,
            b",-inf\r\n"
        );
        assert_eq!(encode(Resp3Value::Double(f64::NAN)).await, b",nan\r\n");
    }

    #[tokio::test]
    async fn boolean_resp3_vs_resp2() {
        assert_eq!(encode(Resp3Value::Boolean(true)).await, b"#t\r\n");
        assert_eq!(encode(Resp3Value::Boolean(false)).await, b"#f\r\n");
        assert_eq!(encode_resp2(Resp3Value::Boolean(true)).await, b":1\r\n");
        assert_eq!(encode_resp2(Resp3Value::Boolean(false)).await, b":0\r\n");
    }

    #[tokio::test]
    async fn verbatim_frame() {
        let out = encode(Resp3Value::Verbatim("txt".into(), b"hi user".to_vec())).await;
        assert_eq!(out, b"=11\r\ntxt:hi user\r\n");
    }

    #[tokio::test]
    async fn verbatim_resp2_lowers_to_bulk() {
        let out = encode_resp2(Resp3Value::Verbatim("txt".into(), b"hi".to_vec())).await;
        assert_eq!(out, b"$2\r\nhi\r\n");
    }

    #[tokio::test]
    async fn set_frame() {
        let out = encode(Resp3Value::Set(vec![
            Resp3Value::Integer(1),
            Resp3Value::Integer(2),
        ]))
        .await;
        assert_eq!(out, b"~2\r\n:1\r\n:2\r\n");
    }

    #[tokio::test]
    async fn set_resp2_is_flat_array() {
        let out = encode_resp2(Resp3Value::Set(vec![
            Resp3Value::Integer(1),
            Resp3Value::Integer(2),
        ]))
        .await;
        assert_eq!(out, b"*2\r\n:1\r\n:2\r\n");
    }

    #[tokio::test]
    async fn map_frame() {
        let out = encode(Resp3Value::Map(vec![(
            Resp3Value::bulk("k"),
            Resp3Value::Integer(7),
        )]))
        .await;
        assert_eq!(out, b"%1\r\n$1\r\nk\r\n:7\r\n");
    }

    #[tokio::test]
    async fn map_resp2_is_flat_array() {
        let out = encode_resp2(Resp3Value::Map(vec![(
            Resp3Value::bulk("k"),
            Resp3Value::Integer(7),
        )]))
        .await;
        assert_eq!(out, b"*2\r\n$1\r\nk\r\n:7\r\n");
    }

    #[tokio::test]
    async fn big_number_frame() {
        assert_eq!(
            encode(Resp3Value::BigNumber(
                "123456789012345678901234567890".into()
            ))
            .await,
            b"(123456789012345678901234567890\r\n"
        );
    }

    #[tokio::test]
    async fn bytes_written_counter_sums_frames() {
        let mut buf = Vec::new();
        let mut w = Resp3Writer::new(&mut buf);
        w.write_ok().await.unwrap();
        let after_ok = w.bytes_written();
        w.write_integer(42).await.unwrap();
        let after_int = w.bytes_written();
        w.flush().await.unwrap();
        assert_eq!(after_ok, 5); // "+OK\r\n"
        assert_eq!(after_int, 5 + 5); // + ":42\r\n"
    }

    #[tokio::test]
    async fn write_error_adds_err_prefix_when_missing() {
        let mut buf = Vec::new();
        {
            let mut w = Resp3Writer::new(&mut buf);
            w.write_error("bad request").await.unwrap();
            w.flush().await.unwrap();
        }
        assert_eq!(buf, b"-ERR bad request\r\n");
    }

    #[tokio::test]
    async fn write_error_preserves_wrongpass_noauth_prefix() {
        let mut buf = Vec::new();
        {
            let mut w = Resp3Writer::new(&mut buf);
            w.write_error("WRONGPASS invalid credentials")
                .await
                .unwrap();
            w.flush().await.unwrap();
        }
        assert_eq!(buf, b"-WRONGPASS invalid credentials\r\n");
    }

    #[tokio::test]
    async fn write_noauth_helper() {
        let mut buf = Vec::new();
        {
            let mut w = Resp3Writer::new(&mut buf);
            w.write_noauth().await.unwrap();
            w.flush().await.unwrap();
        }
        assert_eq!(buf, b"-NOAUTH Authentication required.\r\n");
    }
}
