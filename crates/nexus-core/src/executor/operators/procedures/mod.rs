//! CALL procedure dispatch and all built-in procedure families.
//!
//! This module is a directory facade — implementation lives in the
//! submodules below. Every item that was previously reachable as
//! `crate::executor::operators::procedures::*` continues to be
//! reachable here through `pub use` re-exports where applicable
//! (the procedure methods themselves are `impl Executor` blocks and
//! are therefore accessed via the type, not via module paths).
//!
//! ## Submodule layout
//!
//! | File              | Contents                                              |
//! |-------------------|-------------------------------------------------------|
//! | `call.rs`         | `execute_call_procedure` — the procedure router       |
//! | `db_schema.rs`    | `db.labels`, `db.propertyKeys`, `db.relationshipTypes`, `db.schema`, `db.info` |
//! | `db_indexes.rs`   | `db.indexes`, `db.indexDetails`, `db.constraints`    |
//! | `dbms.rs`         | `dbms.*` procedures + `current_rfc3339_utc` helper   |
//! | `fts.rs`          | `db.index.fulltext.*` + `fts_autopopulate_node`       |
//! | `spatial_procs.rs`| `spatial.addPoint`, `spatial.nearest`, spatial hooks  |

mod call;
mod db_indexes;
mod db_schema;
mod dbms;
mod fts;
mod spatial_procs;
