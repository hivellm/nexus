# One-shot preparsed_ast override bridges a "rewrite-upstream, re-parse-downstream" chokepoint

**Category**: architecture
**Tags**: cluster-mode, multi-tenant, ast-rewriting, executor, phase5_implement-cluster-mode

## Description

Cluster mode's catalog-prefix rewrite taught us a pattern that applies any time a mutation to a parsed AST done at layer A has to reach layer C, but layer B between them insists on re-parsing from the raw source string. In Nexus the concrete case is `Engine::execute_cypher_with_context` rewriting label names in the AST, only to have `Executor::execute(Query)` re-parse `Query.cypher` (the *original* unscoped string) and throw the rewrite away. The clean fix is a single-call-scoped override slot on the shared state: layer A installs its already-mutated AST there, layer B's "parse" step consumes it via `.take()` (so it can never leak into an unrelated subsequent call), and layer C sees the rewritten operators. Crucially the override is one-shot, guarded by an RAII drop on the caller side, so panics or early returns cannot leave stale state. The pattern avoids both (a) a deep refactor to plumb the AST through every handler in layer B, and (b) the alternative of re-serialising the mutated AST back to source — which would require a fully accurate pretty-printer that matches the parser's grammar bijectively, a maintenance burden you almost never want to sign up for.

## Example

// nexus-core/src/executor/shared.rs
pub struct ExecutorShared {
    // ...existing fields...
    preparsed_ast_override: Arc<parking_lot::Mutex<Option<CypherQuery>>>,
}

// nexus-core/src/executor/mod.rs — Executor::execute
let preparsed = self.shared.preparsed_ast_override.lock().take();
let operators = match preparsed {
    Some(ast) => self.plan_ast(&ast)?,         // upstream's mutated AST
    None      => self.parse_and_plan(&cypher)?, // classic string path
};

// nexus-core/src/engine/mod.rs — Engine::execute_cypher_with_context
let mut ast = parser.parse()?;
scope_query(&mut ast, ns, mode);
self.executor.install_preparsed_ast_override(Some(ast.clone()));
// RAII guard clears the slot on every return path, so a leftover
// override cannot leak into the NEXT caller's query.
struct OverrideGuard { executor: Executor }
impl Drop for OverrideGuard {
    fn drop(&mut self) {
        self.executor.install_preparsed_ast_override(None);
    }
}
let _guard = OverrideGuard { executor: self.executor.clone() };

## When to Use

When a middle layer in an existing pipeline re-parses from source, the layers above/below it cannot be refactored at once, and mutating the AST in place is much cheaper than writing a bijective pretty-printer. Typical shapes: query rewriting (multi-tenant scoping, column-level security rewrites, plan hint injection), macro expansion passes that want to keep source-line metadata intact, migration shims that intercept one specific call site without touching the rest of the pipeline. The key invariant — "one-shot, cleared on every return path" — keeps the override from turning into a hidden global.

## When NOT to Use

Do NOT reach for this when (a) the caller in question already has a clean AST-accepting entry point (just use it), (b) multiple concurrent callers might install overrides on the same shared state (the one-shot `.take()` serialises them, but the complexity stops being worth it — add a proper AST-accepting method instead), or (c) the re-parse step exists specifically to normalise input (e.g. plan-hint extraction happens before parsing for a reason; overriding from above bypasses the normalisation and can hide bugs).
