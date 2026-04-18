//! RESP3 compatibility layer — lets any RESP3 client (`redis-cli`, `iredis`,
//! RedisInsight, Jedis, redis-rb, Redix, ...) talk to Nexus using a Nexus
//! command vocabulary (CYPHER, NODE.*, REL.*, KNN.*, INGEST.*, ...).
//!
//! This is a **transport encoding**, not Redis emulation: `SET key value`
//! against the RESP3 port returns `-ERR unknown command 'SET' (Nexus is
//! a graph DB, see HELP)`. The KV semantics are deliberately absent.
//!
//! Wire-format codec (parser + writer) lives in [`nexus_protocol::resp3`]
//! so SDKs and third-party tools can reuse it without depending on the
//! server crate; only the command dispatch and TCP accept loop live here.
//!
//! Full command reference: `docs/specs/resp3-nexus-commands.md`.

pub mod command;
pub mod server;

// Re-export the shared wire codec so existing call sites that reference
// `super::parser::*` / `super::writer::*` keep compiling unchanged.
pub use nexus_protocol::resp3::{parser, writer};
pub use parser::{ParseError, Resp3Value, parse_from_reader, parse_inline};
pub use server::spawn_resp3_listener;
pub use writer::{Resp3Writer, WriteError};
