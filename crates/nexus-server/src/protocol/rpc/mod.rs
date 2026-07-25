//! Server-side plumbing for the native binary RPC transport.
//!
//! Wire types and the codec live in [`nexus_protocol::rpc`] so the Rust
//! SDK can depend on them without dragging in the whole server. This
//! module hosts only server-specific pieces:
//!
//! - [`dispatch`] — command routing onto the shared `NexusServer` state.
//! - `server` — TCP accept loop, per-connection read/write tasks (added
//!   in Phase 7 of `phase1_nexus-rpc-binary-protocol`).

pub mod config;
pub mod dispatch;
pub mod metrics;
pub mod server;

pub use config::nexus_thunder_config;
pub use server::spawn_rpc_listener;

// phase10 — the wire value model and request/response frames now come
// straight from Thunder (wire v1 is byte-identical to the historical Nexus
// RPC wire). `NexusValue` is a thin alias so the dispatch tree + arg
// helpers keep their names unchanged; the async codec that the old
// hand-rolled accept loop used is gone with it (the Thunder listener owns
// framing). `nexus-protocol::rpc` becomes re-export shims for the
// out-of-server consumers in §4 and is deleted in phase11.
pub type NexusValue = thunder::Value;
pub use thunder::{PUSH_ID, Request, Response};
