//! Library surface of the `nexus` CLI.
//!
//! The CLI is primarily a binary; this library exposure exists so
//! integration tests under `nexus-cli/tests/*` can reach the
//! [`client::NexusClient`] and related types without having to spawn
//! the binary as a subprocess for every assertion. The exposed set is
//! deliberately minimal — only the client and the transport plumbing
//! that sits behind it. Anything under `commands/` remains private to
//! the binary because those modules wire directly to the `OutputContext`
//! and would be meaningless to a library consumer.

pub mod client;
pub mod config;
pub mod endpoint;
pub mod rpc_transport;
