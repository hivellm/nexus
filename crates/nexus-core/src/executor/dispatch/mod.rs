//! Main `Executor` dispatch: `execute`, `execute_inner`, planning helpers,
//! query-cache accessors, and `Default` impl.
//!
//! Every public path into the executor goes through [`Executor::execute`]
//! (the notification-draining wrapper) or [`Executor::execute_inner`]
//! (used directly by the `CallSubquery` operator to avoid double-attaching
//! planner notifications). The `Default` impl provides an isolated in-memory
//! executor for tests and benchmarks.
//!
//! The implementation is split across focused submodules:
//! - `execute`       — `execute` / `seed_scan_main_loop` (public entrypoint + scan seeding)
//! - `operator_loop`  — `execute_inner` (the per-operator dispatch loop)
//! - `support`        — query-cache accessors and the `COUNT(*)` short-circuit fast paths
//! - `planning`       — `parse_and_plan` / `plan_ast` / `ast_to_operators`
//! - `default`        — `impl Default for Executor`

mod default;
mod execute;
mod operator_loop;
mod planning;
mod support;
