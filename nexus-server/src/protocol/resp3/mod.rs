//! RESP3 compatibility layer — lets any RESP3 client (`redis-cli`, `iredis`,
//! RedisInsight, Jedis, redis-rb, Redix, ...) talk to Nexus using a Nexus
//! command vocabulary (CYPHER, NODE.*, REL.*, KNN.*, INGEST.*, ...).
//!
//! This is a **transport encoding**, not Redis emulation: `SET key value`
//! against the RESP3 port returns `-ERR unknown command 'SET' (Nexus is
//! a graph DB, see HELP)`. The KV semantics are deliberately absent.
//!
//! Full command reference: `docs/specs/resp3-nexus-commands.md`.

pub mod command;
pub mod parser;
pub mod server;
pub mod writer;

pub use parser::{ParseError, Resp3Value, parse_from_reader, parse_inline};
pub use server::spawn_resp3_listener;
pub use writer::{Resp3Writer, WriteError};
