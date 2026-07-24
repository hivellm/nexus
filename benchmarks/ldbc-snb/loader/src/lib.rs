//! Shared library for the LDBC SNB loaders.
//!
//! The dataset description (`schema`) and the pipe-CSV parsing (`csv_source`)
//! are engine-agnostic and shared by both binaries; the write side differs:
//! `client` + `load` populate Nexus through `POST /ingest` (the `ldbc-load`
//! bin), while `neo4j` populates the Neo4j baseline through its Cypher HTTP
//! transaction endpoint (the `neo4j-load` bin). Both produce the SAME logical
//! graph — identical labels (including the `:Message` superlabel), the same
//! synthesized merge-foreign edges, and dates as epoch-millisecond integers —
//! so a query can be run against both and the results compared like for like.

pub mod client;
pub mod csv_source;
pub mod load;
pub mod neo4j;
pub mod schema;
