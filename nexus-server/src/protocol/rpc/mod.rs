//! Server-side plumbing for the native binary RPC transport.
//!
//! Wire types and the codec live in [`nexus_protocol::rpc`] so the Rust
//! SDK can depend on them without dragging in the whole server. This
//! module hosts only server-specific pieces:
//!
//! - [`dispatch`] — command routing onto the shared `NexusServer` state.
//! - `server` — TCP accept loop, per-connection read/write tasks (added
//!   in Phase 7 of `phase1_nexus-rpc-binary-protocol`).

pub mod dispatch;

// Re-export the shared wire surface for ergonomic intra-server use so
// handlers can write `use crate::protocol::rpc::NexusValue;` instead of
// reaching into `nexus_protocol` directly.
pub use nexus_protocol::rpc::{
    DEFAULT_MAX_FRAME_BYTES, DecodeError, NexusValue, PUSH_ID, Request, Response, decode_frame,
    decode_frame_with_limit, encode_frame, read_request, read_request_with_limit, read_response,
    write_request, write_response,
};
