# Pure-value procedures route via dispatcher, engine-aware ones live on the executor

**Category**: cypher-surface
**Tags**: none

## Description

When adding a new `ns.*` procedure family to Nexus, split pure-value procedures (inputs → outputs, no engine access) into a `crate::ns` dispatch module mirroring `crate::apoc`, and keep engine-aware procedures as `pub(in crate::executor) fn execute_ns_xxx` methods on Executor.</description>
<content>Nexus's `execute_call_procedure` legacy path packs every procedure argument under a single `"arg"` key and passes an empty `Graph` — unusable for anything beyond single-arg pure-value procedures. APOC worked around this with `crate::apoc::dispatch(name, Vec<Value>) -> Result<Option<ApocResult>>`. Geospatial Slice A mirrors the pattern (`crate::spatial::dispatch`) and adds a second route for engine-aware work: procedures that need `ExecutorShared` (e.g., to probe `spatial_indexes`) get their own `execute_xxx` method on the Executor, dispatched by exact-name match before the pure-value fallback.\n\nPattern:\n1. `if procedure_name == "ns.engineAware" { return self.execute_ns_engine_aware(...); }`\n2. `if procedure_name.starts_with("ns.") { dispatch via crate::ns::dispatch(...) }`\n3. On `None` from the dispatcher, return `ERR_PROC_NOT_FOUND` with `crate::ns::list_procedures()` — don't fall through to the legacy `GraphProcedure` registry.\n\nSource: `phase6_opencypher-geospatial-predicates` slice A. Code: `crates/nexus-core/src/executor/operators/procedures.rs` dispatch block; `crates/nexus-core/src/spatial/mod.rs` dispatcher; `execute_spatial_nearest` / `execute_spatial_add_point` for the engine-aware path.</content>
</invoke>
