## 1. Scalar functions
- [x] 1.1 `randomUUID()` (v4, string) — `fn_graph.rs`
- [x] 1.2 String functions: `ascii()`, `chr()`, `lpad()`, `rpad()`, `normalize()` (NFC default + NFD/NFKC/NFKD form arg; `unicode-normalization 0.1` added after verifying it was not already in the tree) — `fn_string.rs`
- [x] 1.3 Math: two-arg `log(x, base)`; `isNaN()` — `fn_math.rs`
- [x] 1.4 List: `shuffle()` — `fn_list.rs`, using the crate's existing RNG

## 2. Formats and verification
- [x] 2.1 `elementId()` returns a stable opaque string `"n:<node-id>"` / `"r:<relationship-id>"`; `id()` keeps the integer; CHANGELOG documents the format change
- [x] 2.2 `percentileDisc`/`percentileCont`/`stDev`/`stDevP` verified against hand-computed references (stDev [1,2,3,4]=1.2909944, stDevP=1.1180339, percentileCont([1..4],0.5)=2.5, percentileDisc([1..5],0.5)=3); fixes applied in `aggregate/core.rs` where divergent

## 3. Parser
- [x] 3.1 Multi-pattern CREATE `CREATE (a:L1), (b:L2)` (and `..., (a)-[r:T]->(b)`) — probe showed the parser already accepts comma-separated patterns; behavior covered by new tests through both `execute_cypher` and `execute_cypher_with_params` entry points (no parser change needed)

## 4. Tail (mandatory — enforced by rulebook v5.3.0)
- [x] 4.1 Update or create documentation covering the implementation — `docs/specs/cypher-subset.md` (string + math sections gained the 2.5.0 additions), CHANGELOG `[Unreleased — 2.5.0]` Added entries incl. the elementId format note
- [x] 4.2 Write tests covering the new behavior — `tests/phase4_cypher_parity_quick_wins_test.rs`: 35 tests (happy path + null + type-error per function, percentile references, multi-pattern CREATE)
- [x] 4.3 Run tests and confirm they pass — 35/35; nexus-core lib 2414 passed / 0 failed; clippy `--all-targets -D warnings` clean; fmt clean
