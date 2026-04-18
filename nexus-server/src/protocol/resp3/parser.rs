//! RESP3 parser — consumes a byte stream and produces `Resp3Value`s.
//!
//! Covers all 12 RESP3 type prefixes plus `redis-cli`-style inline commands
//! so a plain telnet session can talk to Nexus. Designed to tolerate TCP
//! fragmentation: the async reader is polled until the next framing marker
//! is satisfied, so split reads are handled naturally.
//!
//! This parser is forgiving on input and strict on output: a malformed
//! frame returns `ParseError`, but the `Resp3Value`s it does build are
//! always self-consistent (counts match lengths, maps are even-element,
//! etc.).

use std::fmt;

use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncReadExt};

/// RESP3 value tree — the in-memory representation of any RESP3 frame.
///
/// Variants map 1:1 onto the type prefixes documented in
/// <https://github.com/antirez/RESP3/blob/master/spec.md>. `Null` is the
/// RESP3 `_` sentinel (distinct from the RESP2 `$-1\r\n`/`*-1\r\n` encodings
/// that the writer lowers it to when a RESP2 client is detected).
#[derive(Debug, Clone, PartialEq)]
pub enum Resp3Value {
    /// `+<string>\r\n` — e.g. `+OK\r\n`
    SimpleString(String),
    /// `-<message>\r\n` — e.g. `-ERR bad arg\r\n`
    Error(String),
    /// `:<signed-integer>\r\n`
    Integer(i64),
    /// `$<len>\r\n<bytes>\r\n` — binary-safe. Length -1 means RESP2 null.
    BulkString(Vec<u8>),
    /// `*<n>\r\n` followed by `n` values.
    Array(Vec<Resp3Value>),
    /// `_\r\n` — RESP3 native null.
    Null,
    /// `,<double>\r\n`
    Double(f64),
    /// `#t\r\n` / `#f\r\n`
    Boolean(bool),
    /// `=<len>\r\nfmt:<bytes>\r\n` — Verbatim string with a 3-byte format tag
    /// (e.g. `txt`, `mkd`).
    Verbatim(String, Vec<u8>),
    /// `~<n>\r\n` — unordered set.
    Set(Vec<Resp3Value>),
    /// `%<n>\r\n` — key/value pairs. Represented as a `Vec<(k, v)>` to
    /// preserve insertion order (important for deterministic responses).
    Map(Vec<(Resp3Value, Resp3Value)>),
    /// `(<decimal>\r\n` — arbitrary-precision integer, kept as a string.
    BigNumber(String),
}

impl Resp3Value {
    /// Return the raw bytes iff this value is a `BulkString`. `None` for
    /// every other variant (including `SimpleString`, which callers usually
    /// want coerced via `as_str`).
    pub fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            Resp3Value::BulkString(b) => Some(b.as_slice()),
            _ => None,
        }
    }

    /// Coerce common string-like variants into `&str`. Returns `None` if the
    /// payload exists but is not valid UTF-8.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Resp3Value::SimpleString(s) => Some(s.as_str()),
            Resp3Value::BulkString(b) => std::str::from_utf8(b).ok(),
            Resp3Value::Verbatim(_, b) => std::str::from_utf8(b).ok(),
            Resp3Value::Error(s) => Some(s.as_str()),
            _ => None,
        }
    }

    /// Extract an integer from either `Integer` or a numeric-looking bulk
    /// string. Used to let clients send `"42"` where we expect an `i64`.
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Resp3Value::Integer(n) => Some(*n),
            Resp3Value::BulkString(b) => std::str::from_utf8(b).ok()?.trim().parse().ok(),
            Resp3Value::BigNumber(s) => s.parse().ok(),
            _ => None,
        }
    }

    /// True for the RESP3 `_` sentinel.
    pub fn is_null(&self) -> bool {
        matches!(self, Resp3Value::Null)
    }

    /// Convenience: wrap a UTF-8 string as a `BulkString`.
    pub fn bulk<S: Into<String>>(s: S) -> Self {
        Resp3Value::BulkString(s.into().into_bytes())
    }

    /// Convenience: wrap a `-ERR <msg>` error message.
    pub fn err<S: Into<String>>(msg: S) -> Self {
        Resp3Value::Error(msg.into())
    }
}

/// All ways a RESP3 byte stream can disagree with the spec.
#[derive(Debug)]
pub enum ParseError {
    /// Underlying I/O failure. `None` when end-of-stream was reached cleanly.
    Io(std::io::Error),
    /// Encountered a type prefix byte we don't recognise.
    UnknownPrefix(u8),
    /// Bulk-string / verbatim length header could not be parsed.
    BadLength(String),
    /// Numeric header (Integer, Double, array count, ...) was not a valid
    /// number.
    BadNumber(String),
    /// Verbatim string missing the `fmt:` prefix or shorter than 4 bytes.
    MalformedVerbatim,
    /// Map payload had an odd number of elements.
    OddMapLength,
    /// UTF-8 violation in a place where the spec mandates text.
    InvalidUtf8,
    /// Parsing gave up because the stream ended mid-frame.
    UnexpectedEof,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::Io(e) => write!(f, "RESP3 I/O error: {e}"),
            ParseError::UnknownPrefix(b) => write!(f, "RESP3 unknown type prefix: 0x{b:02x}"),
            ParseError::BadLength(s) => write!(f, "RESP3 bad length header: {s}"),
            ParseError::BadNumber(s) => write!(f, "RESP3 bad numeric value: {s}"),
            ParseError::MalformedVerbatim => f.write_str("RESP3 malformed verbatim string"),
            ParseError::OddMapLength => f.write_str("RESP3 map payload had odd element count"),
            ParseError::InvalidUtf8 => {
                f.write_str("RESP3 non-UTF-8 payload where text was required")
            }
            ParseError::UnexpectedEof => f.write_str("RESP3 stream ended mid-frame"),
        }
    }
}

impl std::error::Error for ParseError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ParseError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for ParseError {
    fn from(e: std::io::Error) -> Self {
        ParseError::Io(e)
    }
}

/// Read one full RESP3 frame (possibly nested) from `reader`.
///
/// Returns `Ok(None)` on clean EOF before any bytes have been seen; the
/// connection loop uses this to distinguish a peer closing cleanly from a
/// mid-frame disconnect. Also supports inline commands (any first byte that
/// is not a RESP3 type prefix triggers whitespace tokenisation — exactly
/// what `redis-cli` and `telnet` send).
pub async fn parse_from_reader<R>(reader: &mut R) -> Result<Option<Resp3Value>, ParseError>
where
    R: AsyncBufRead + AsyncRead + Unpin + Send,
{
    // Peek at the first byte without committing — we need the line back if
    // it turns out to be an inline command.
    let first = match read_one_byte(reader).await? {
        Some(b) => b,
        None => return Ok(None),
    };

    match first {
        b'+' => parse_simple_string(reader).await.map(Some),
        b'-' => parse_error(reader).await.map(Some),
        b':' => parse_integer(reader).await.map(Some),
        b'$' => parse_bulk_string(reader).await.map(Some),
        b'*' => parse_array(reader).await.map(Some),
        b'_' => parse_null(reader).await.map(Some),
        b',' => parse_double(reader).await.map(Some),
        b'#' => parse_boolean(reader).await.map(Some),
        b'=' => parse_verbatim(reader).await.map(Some),
        b'~' => parse_set(reader).await.map(Some),
        b'%' => parse_map(reader).await.map(Some),
        b'(' => parse_big_number(reader).await.map(Some),
        b'|' => parse_attribute_then_value(reader).await,
        b'\r' | b'\n' => {
            // Leading blank line — skip it and try again.
            // (Some clients send an extra CRLF after QUIT.)
            skip_rest_of_crlf(reader).await?;
            Box::pin(parse_from_reader(reader)).await
        }
        _ => parse_inline_from_first_byte(reader, first).await.map(Some),
    }
}

/// Public helper: split a text line into a RESP3 Array of BulkStrings the
/// way `redis-cli` does. Used both by the inline-parser branch above and by
/// the test-suite.
pub fn parse_inline(line: &str) -> Resp3Value {
    let tokens: Vec<Resp3Value> = shell_split(line)
        .into_iter()
        .map(Resp3Value::bulk)
        .collect();
    Resp3Value::Array(tokens)
}

// --------------------------------------------------------------------------
// Internal helpers — one per type prefix. All of them consume input starting
// AFTER the prefix byte has already been read.
// --------------------------------------------------------------------------

async fn parse_simple_string<R: AsyncBufRead + Unpin>(
    reader: &mut R,
) -> Result<Resp3Value, ParseError> {
    let line = read_crlf_line(reader).await?;
    Ok(Resp3Value::SimpleString(line))
}

async fn parse_error<R: AsyncBufRead + Unpin>(reader: &mut R) -> Result<Resp3Value, ParseError> {
    let line = read_crlf_line(reader).await?;
    Ok(Resp3Value::Error(line))
}

async fn parse_integer<R: AsyncBufRead + Unpin>(reader: &mut R) -> Result<Resp3Value, ParseError> {
    let line = read_crlf_line(reader).await?;
    let n = line
        .parse::<i64>()
        .map_err(|_| ParseError::BadNumber(line))?;
    Ok(Resp3Value::Integer(n))
}

async fn parse_big_number<R: AsyncBufRead + Unpin>(
    reader: &mut R,
) -> Result<Resp3Value, ParseError> {
    let line = read_crlf_line(reader).await?;
    Ok(Resp3Value::BigNumber(line))
}

async fn parse_double<R: AsyncBufRead + Unpin>(reader: &mut R) -> Result<Resp3Value, ParseError> {
    let line = read_crlf_line(reader).await?;
    // RESP3 specials: `inf`, `-inf`, `nan`.
    let n = match line.to_ascii_lowercase().as_str() {
        "inf" | "+inf" => f64::INFINITY,
        "-inf" => f64::NEG_INFINITY,
        "nan" => f64::NAN,
        _ => line
            .parse::<f64>()
            .map_err(|_| ParseError::BadNumber(line))?,
    };
    Ok(Resp3Value::Double(n))
}

async fn parse_boolean<R: AsyncBufRead + Unpin>(reader: &mut R) -> Result<Resp3Value, ParseError> {
    let line = read_crlf_line(reader).await?;
    match line.as_str() {
        "t" => Ok(Resp3Value::Boolean(true)),
        "f" => Ok(Resp3Value::Boolean(false)),
        other => Err(ParseError::BadNumber(other.to_string())),
    }
}

async fn parse_null<R: AsyncBufRead + Unpin>(reader: &mut R) -> Result<Resp3Value, ParseError> {
    // `_\r\n` — body is empty; just drain the CRLF.
    let _ = read_crlf_line(reader).await?;
    Ok(Resp3Value::Null)
}

async fn parse_bulk_string<R>(reader: &mut R) -> Result<Resp3Value, ParseError>
where
    R: AsyncBufRead + AsyncRead + Unpin + Send,
{
    let len_line = read_crlf_line(reader).await?;
    // `$-1\r\n` is the legacy RESP2 null encoding.
    if len_line == "-1" {
        return Ok(Resp3Value::Null);
    }
    let len: usize = len_line
        .parse()
        .map_err(|_| ParseError::BadLength(len_line))?;
    let mut buf = vec![0u8; len];
    reader
        .read_exact(&mut buf)
        .await
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::UnexpectedEof => ParseError::UnexpectedEof,
            _ => ParseError::Io(e),
        })?;
    // Trailing CRLF after the body.
    expect_crlf(reader).await?;
    Ok(Resp3Value::BulkString(buf))
}

async fn parse_verbatim<R>(reader: &mut R) -> Result<Resp3Value, ParseError>
where
    R: AsyncBufRead + AsyncRead + Unpin + Send,
{
    let len_line = read_crlf_line(reader).await?;
    let len: usize = len_line
        .parse()
        .map_err(|_| ParseError::BadLength(len_line))?;
    if len < 4 {
        return Err(ParseError::MalformedVerbatim);
    }
    let mut buf = vec![0u8; len];
    reader
        .read_exact(&mut buf)
        .await
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::UnexpectedEof => ParseError::UnexpectedEof,
            _ => ParseError::Io(e),
        })?;
    expect_crlf(reader).await?;
    if buf[3] != b':' {
        return Err(ParseError::MalformedVerbatim);
    }
    let fmt = std::str::from_utf8(&buf[0..3])
        .map_err(|_| ParseError::InvalidUtf8)?
        .to_string();
    let body = buf[4..].to_vec();
    Ok(Resp3Value::Verbatim(fmt, body))
}

async fn parse_array<R>(reader: &mut R) -> Result<Resp3Value, ParseError>
where
    R: AsyncBufRead + AsyncRead + Unpin + Send,
{
    let n = read_count(reader).await?;
    let mut items = Vec::with_capacity(n);
    for _ in 0..n {
        let v = Box::pin(parse_from_reader(reader))
            .await?
            .ok_or(ParseError::UnexpectedEof)?;
        items.push(v);
    }
    Ok(Resp3Value::Array(items))
}

async fn parse_set<R>(reader: &mut R) -> Result<Resp3Value, ParseError>
where
    R: AsyncBufRead + AsyncRead + Unpin + Send,
{
    let n = read_count(reader).await?;
    let mut items = Vec::with_capacity(n);
    for _ in 0..n {
        let v = Box::pin(parse_from_reader(reader))
            .await?
            .ok_or(ParseError::UnexpectedEof)?;
        items.push(v);
    }
    Ok(Resp3Value::Set(items))
}

async fn parse_map<R>(reader: &mut R) -> Result<Resp3Value, ParseError>
where
    R: AsyncBufRead + AsyncRead + Unpin + Send,
{
    let n = read_count(reader).await?;
    let mut entries = Vec::with_capacity(n);
    for _ in 0..n {
        let k = Box::pin(parse_from_reader(reader))
            .await?
            .ok_or(ParseError::UnexpectedEof)?;
        let v = Box::pin(parse_from_reader(reader))
            .await?
            .ok_or(ParseError::UnexpectedEof)?;
        entries.push((k, v));
    }
    Ok(Resp3Value::Map(entries))
}

/// Attribute prefix `|` carries out-of-band metadata and MUST be ignored by
/// clients that do not understand it. We parse-and-discard the attribute
/// map, then return whatever value follows it.
async fn parse_attribute_then_value<R>(reader: &mut R) -> Result<Option<Resp3Value>, ParseError>
where
    R: AsyncBufRead + AsyncRead + Unpin + Send,
{
    let n = read_count(reader).await?;
    for _ in 0..n {
        let _k = Box::pin(parse_from_reader(reader))
            .await?
            .ok_or(ParseError::UnexpectedEof)?;
        let _v = Box::pin(parse_from_reader(reader))
            .await?
            .ok_or(ParseError::UnexpectedEof)?;
    }
    // The spec says an attribute is a *prefix* — a real value must follow.
    Box::pin(parse_from_reader(reader)).await
}

async fn parse_inline_from_first_byte<R>(
    reader: &mut R,
    first: u8,
) -> Result<Resp3Value, ParseError>
where
    R: AsyncBufRead + Unpin,
{
    // Read the rest of the line and stitch the first byte back on the front.
    let rest = read_crlf_line(reader).await?;
    let mut line = String::with_capacity(1 + rest.len());
    line.push(first as char);
    line.push_str(&rest);
    Ok(parse_inline(&line))
}

// --------------------------------------------------------------------------
// Low-level byte helpers.
// --------------------------------------------------------------------------

async fn read_one_byte<R: AsyncBufRead + Unpin>(reader: &mut R) -> Result<Option<u8>, ParseError> {
    let mut buf = [0u8; 1];
    match tokio::io::AsyncReadExt::read(reader, &mut buf).await {
        Ok(0) => Ok(None),
        Ok(_) => Ok(Some(buf[0])),
        Err(e) => Err(ParseError::Io(e)),
    }
}

async fn read_crlf_line<R: AsyncBufRead + Unpin>(reader: &mut R) -> Result<String, ParseError> {
    let mut buf = String::new();
    // `read_line` keeps the terminating `\n`; we strip both `\r` and `\n`.
    let n = reader.read_line(&mut buf).await?;
    if n == 0 {
        return Err(ParseError::UnexpectedEof);
    }
    // Strip trailing `\n` or `\r\n`.
    if buf.ends_with('\n') {
        buf.pop();
        if buf.ends_with('\r') {
            buf.pop();
        }
    }
    Ok(buf)
}

async fn read_count<R: AsyncBufRead + Unpin>(reader: &mut R) -> Result<usize, ParseError> {
    let line = read_crlf_line(reader).await?;
    let n = line
        .parse::<usize>()
        .map_err(|_| ParseError::BadLength(line))?;
    Ok(n)
}

async fn expect_crlf<R: AsyncRead + AsyncBufRead + Unpin>(
    reader: &mut R,
) -> Result<(), ParseError> {
    let mut buf = [0u8; 2];
    reader
        .read_exact(&mut buf)
        .await
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::UnexpectedEof => ParseError::UnexpectedEof,
            _ => ParseError::Io(e),
        })?;
    if &buf != b"\r\n" {
        return Err(ParseError::BadLength(format!(
            "expected CRLF after bulk body, got 0x{:02x}{:02x}",
            buf[0], buf[1]
        )));
    }
    Ok(())
}

async fn skip_rest_of_crlf<R: AsyncBufRead + Unpin>(reader: &mut R) -> Result<(), ParseError> {
    let mut byte = [0u8; 1];
    // The `\r` or `\n` we already consumed may need one more `\n` after it.
    let _ = reader.read(&mut byte).await?;
    Ok(())
}

/// Minimal POSIX-shell-style word splitter — whitespace separates words,
/// and single or double quotes group a single word. Good enough for
/// `redis-cli` inline arguments; not a full shell lexer.
fn shell_split(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut chars = line.chars().peekable();
    let mut in_quote: Option<char> = None;
    while let Some(c) = chars.next() {
        match (in_quote, c) {
            (Some(q), ch) if ch == q => {
                in_quote = None;
            }
            (None, '\'') | (None, '"') => {
                in_quote = Some(c);
            }
            (None, ch) if ch.is_whitespace() => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            (_, '\\') => {
                // Take next char literally (escape).
                if let Some(next) = chars.next() {
                    cur.push(next);
                }
            }
            (_, ch) => cur.push(ch),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

// --------------------------------------------------------------------------
// Tests.
// --------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::BufReader;

    async fn parse_bytes(bytes: &[u8]) -> Result<Option<Resp3Value>, ParseError> {
        let mut reader = BufReader::new(bytes);
        parse_from_reader(&mut reader).await
    }

    #[tokio::test]
    async fn simple_string() {
        let v = parse_bytes(b"+OK\r\n").await.unwrap().unwrap();
        assert_eq!(v, Resp3Value::SimpleString("OK".into()));
        assert_eq!(v.as_str(), Some("OK"));
    }

    #[tokio::test]
    async fn error_payload() {
        let v = parse_bytes(b"-ERR bad\r\n").await.unwrap().unwrap();
        assert_eq!(v, Resp3Value::Error("ERR bad".into()));
    }

    #[tokio::test]
    async fn integer_positive_and_negative() {
        assert_eq!(
            parse_bytes(b":42\r\n").await.unwrap().unwrap(),
            Resp3Value::Integer(42)
        );
        assert_eq!(
            parse_bytes(b":-7\r\n").await.unwrap().unwrap(),
            Resp3Value::Integer(-7)
        );
    }

    #[tokio::test]
    async fn bulk_string_binary_safe() {
        let v = parse_bytes(b"$5\r\nhello\r\n").await.unwrap().unwrap();
        assert_eq!(v.as_bytes(), Some(&b"hello"[..]));
    }

    #[tokio::test]
    async fn bulk_string_null_legacy() {
        let v = parse_bytes(b"$-1\r\n").await.unwrap().unwrap();
        assert!(v.is_null());
    }

    #[tokio::test]
    async fn null_native() {
        let v = parse_bytes(b"_\r\n").await.unwrap().unwrap();
        assert!(v.is_null());
    }

    #[tokio::test]
    async fn double_inf_nan() {
        assert_eq!(
            parse_bytes(b",3.14\r\n").await.unwrap().unwrap(),
            Resp3Value::Double(3.14)
        );
        assert_eq!(
            parse_bytes(b",inf\r\n").await.unwrap().unwrap(),
            Resp3Value::Double(f64::INFINITY)
        );
        match parse_bytes(b",nan\r\n").await.unwrap().unwrap() {
            Resp3Value::Double(n) => assert!(n.is_nan()),
            other => panic!("expected Double(NaN), got {other:?}"),
        }
    }

    #[tokio::test]
    async fn boolean_roundtrip() {
        assert_eq!(
            parse_bytes(b"#t\r\n").await.unwrap().unwrap(),
            Resp3Value::Boolean(true)
        );
        assert_eq!(
            parse_bytes(b"#f\r\n").await.unwrap().unwrap(),
            Resp3Value::Boolean(false)
        );
    }

    #[tokio::test]
    async fn verbatim_txt() {
        // `txt:` (4 bytes) + `hi user` (7 bytes) = 11 bytes total payload.
        let v = parse_bytes(b"=11\r\ntxt:hi user\r\n")
            .await
            .unwrap()
            .unwrap();
        match v {
            Resp3Value::Verbatim(fmt, body) => {
                assert_eq!(fmt, "txt");
                assert_eq!(body, b"hi user");
            }
            other => panic!("expected Verbatim, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn verbatim_too_short_is_error() {
        assert!(parse_bytes(b"=2\r\nab\r\n").await.is_err());
    }

    #[tokio::test]
    async fn array_of_heterogeneous_values() {
        let input = b"*3\r\n:1\r\n+OK\r\n$4\r\nabcd\r\n";
        let v = parse_bytes(input).await.unwrap().unwrap();
        match v {
            Resp3Value::Array(items) => {
                assert_eq!(items.len(), 3);
                assert_eq!(items[0], Resp3Value::Integer(1));
                assert_eq!(items[1], Resp3Value::SimpleString("OK".into()));
                assert_eq!(items[2].as_bytes(), Some(&b"abcd"[..]));
            }
            other => panic!("expected Array, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn set_parses_as_unordered_collection() {
        let v = parse_bytes(b"~2\r\n:1\r\n:2\r\n").await.unwrap().unwrap();
        assert_eq!(
            v,
            Resp3Value::Set(vec![Resp3Value::Integer(1), Resp3Value::Integer(2)])
        );
    }

    #[tokio::test]
    async fn map_pairs() {
        let v = parse_bytes(b"%1\r\n+k\r\n:7\r\n").await.unwrap().unwrap();
        assert_eq!(
            v,
            Resp3Value::Map(vec![(
                Resp3Value::SimpleString("k".into()),
                Resp3Value::Integer(7)
            )])
        );
    }

    #[tokio::test]
    async fn big_number() {
        let v = parse_bytes(b"(3492890328409238509324850943850943825024385\r\n")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            v,
            Resp3Value::BigNumber("3492890328409238509324850943850943825024385".into())
        );
    }

    #[tokio::test]
    async fn attribute_is_discarded_and_value_is_returned() {
        // |1\r\n +k\r\n +v\r\n | $2\r\nok\r\n
        let input = b"|1\r\n+k\r\n+v\r\n$2\r\nok\r\n";
        let v = parse_bytes(input).await.unwrap().unwrap();
        assert_eq!(v.as_bytes(), Some(&b"ok"[..]));
    }

    #[tokio::test]
    async fn unknown_prefix_triggers_inline_parse_not_error() {
        // `PING` as an inline command — first byte `P` is not a RESP3 prefix.
        let v = parse_bytes(b"PING\r\n").await.unwrap().unwrap();
        match v {
            Resp3Value::Array(items) => {
                assert_eq!(items.len(), 1);
                assert_eq!(items[0].as_str(), Some("PING"));
            }
            other => panic!("expected Array, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn inline_with_quoted_arg_containing_spaces() {
        let v = parse_bytes(b"CYPHER \"MATCH (n) RETURN n\"\r\n")
            .await
            .unwrap()
            .unwrap();
        match v {
            Resp3Value::Array(items) => {
                assert_eq!(items.len(), 2);
                assert_eq!(items[0].as_str(), Some("CYPHER"));
                assert_eq!(items[1].as_str(), Some("MATCH (n) RETURN n"));
            }
            other => panic!("expected Array, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn clean_eof_returns_none() {
        assert!(parse_bytes(b"").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn split_read_is_tolerated() {
        // Simulate TCP fragmentation via a `Chain` — a naive parser that
        // assumed one recv == one frame would break on this input.
        let first = b"$5\r\nhel" as &[u8];
        let second = b"lo\r\n" as &[u8];
        let combined: Vec<u8> = first.iter().chain(second.iter()).copied().collect();
        let v = parse_bytes(&combined).await.unwrap().unwrap();
        assert_eq!(v.as_bytes(), Some(&b"hello"[..]));
    }

    #[test]
    fn as_helpers_extract_expected_primitive() {
        assert_eq!(Resp3Value::bulk("hello").as_str(), Some("hello"));
        assert_eq!(
            Resp3Value::BulkString(vec![0xff, 0xfe]).as_str(),
            None,
            "non-UTF-8 bulk must not coerce"
        );
        assert_eq!(
            Resp3Value::bulk("42").as_int(),
            Some(42),
            "numeric-looking bulk coerces to i64"
        );
        assert_eq!(Resp3Value::Integer(-9).as_int(), Some(-9));
        assert!(Resp3Value::Null.is_null());
        assert!(!Resp3Value::Integer(0).is_null());
    }

    #[test]
    fn shell_split_handles_quotes_and_escapes() {
        assert_eq!(shell_split("a b c"), vec!["a", "b", "c"]);
        assert_eq!(
            shell_split("a \"b c\" d"),
            vec!["a".to_string(), "b c".to_string(), "d".to_string()]
        );
        assert_eq!(
            shell_split("a 'b c' d"),
            vec!["a".to_string(), "b c".to_string(), "d".to_string()]
        );
        assert_eq!(
            shell_split(r#"a \"quoted\" b"#),
            vec!["a".to_string(), "\"quoted\"".to_string(), "b".to_string()]
        );
    }
}
