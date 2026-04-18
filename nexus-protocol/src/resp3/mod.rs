//! RESP3 wire format — parser + writer.
//!
//! Split from `nexus-server` so SDKs and third-party tooling can parse
//! RESP3 frames without taking a dependency on the full server. Command
//! dispatch (which needs a running engine) lives in
//! `nexus-server::protocol::resp3`.

pub mod parser;
pub mod writer;

pub use parser::{ParseError, Resp3Value, parse_from_reader, parse_inline};
pub use writer::{ProtocolVersion, Resp3Writer, WriteError};
