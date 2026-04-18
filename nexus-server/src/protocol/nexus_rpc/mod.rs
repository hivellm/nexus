//! NexusRPC — the native binary transport for Nexus.
//!
//! NexusRPC is a length-prefixed MessagePack framing designed to replace
//! HTTP+JSON as the preferred SDK transport for high-throughput Cypher
//! traffic, bulk ingest, and KNN queries where the JSON tax is measurable.
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
//! `Request` carries a caller-chosen `id: u32` which the server echoes
//! back on the matching `Response`. The id `u32::MAX` is reserved for
//! server-initiated push frames (pubsub, streaming Cypher).
//!
//! See `docs/specs/rpc-wire-format.md` for the authoritative specification
//! and `docs/specs/api-protocols.md` for the transport matrix.
//!
//! ## Layout
//!
//! - [`types`] — [`NexusValue`], [`Request`], [`Response`].
//! - [`codec`] — length-prefix framing + async read/write helpers.
//! - [`server`] — TCP accept loop + per-connection dispatch.
//! - [`dispatch`] — command routing to graph/cypher/knn/ingest/schema/admin.
//!
//! The layout mirrors `synap_rpc` from the sister Synap project, with wire
//! types renamed to `NexusValue` for clarity in cross-project tooling.

pub mod codec;
pub mod types;

pub use codec::{
    DecodeError, decode_frame, encode_frame, read_request, read_response, write_request,
    write_response,
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
