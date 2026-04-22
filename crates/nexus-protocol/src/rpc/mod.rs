//! Native binary RPC protocol shared between `nexus-server` and the Rust SDK.
//!
//! A length-prefixed MessagePack framing designed to replace HTTP+JSON as
//! the preferred SDK transport for high-throughput Cypher traffic, bulk
//! ingest, and KNN queries where the JSON tax is measurable.
//!
//! ## Wire format
//!
//! ```text
//! ┌───────────────────┬──────────────────────────┐
//! │  length: u32 (LE) │  body: MessagePack bytes  │
//! └───────────────────┴──────────────────────────┘
//!     4 bytes              length bytes
//! ```
//!
//! A single TCP connection multiplexes many concurrent requests: each
//! [`Request`] carries a caller-chosen `id: u32` which the server echoes
//! back on the matching [`Response`]. The id [`PUSH_ID`] (`u32::MAX`) is
//! reserved for server-initiated push frames.
//!
//! See `docs/specs/rpc-wire-format.md` for the authoritative specification
//! and `docs/specs/api-protocols.md` for the transport matrix.
//!
//! ## Why this lives in `nexus-protocol`
//!
//! Wire types and the codec are zero-knowledge of server internals — they
//! are consumed both by the server's accept loop and by the Rust SDK's
//! client. Putting them in this crate lets the SDK depend on them without
//! pulling in all of `nexus-server` (database manager, executor, auth).
//! Server-only pieces — the TCP accept loop and per-command dispatch —
//! stay inside `nexus-server::protocol::rpc`.

pub mod codec;
pub mod types;

pub use codec::{
    DecodeError, decode_frame, decode_frame_with_limit, encode_frame, read_request,
    read_request_with_limit, read_response, write_request, write_response,
};
pub use types::{NexusValue, Request, Response};

/// Reserved request id for server-initiated push frames.
///
/// Clients MUST NOT use this value for their own requests; if they do, the
/// server will refuse the request with a dedicated error so push demultiplexing
/// stays unambiguous.
pub const PUSH_ID: u32 = u32::MAX;

/// Default cap on a single frame's encoded body size (64 MiB).
///
/// Oversized frames almost always indicate a malformed or hostile peer; the
/// codec rejects them before allocating the body buffer so a bad length prefix
/// cannot be used to exhaust server memory.
pub const DEFAULT_MAX_FRAME_BYTES: usize = 64 * 1024 * 1024;
