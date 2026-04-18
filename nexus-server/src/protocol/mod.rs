//! Alternative wire protocols for `nexus-server`.
//!
//! HTTP/REST remains the primary interface; modules under here layer
//! additional transports on top of the same `NexusServer` state so
//! operators and long-tail SDKs can pick the encoding that matches their
//! ecosystem. See `docs/specs/api-protocols.md` for the full matrix.

pub mod nexus_rpc;
pub mod resp3;
