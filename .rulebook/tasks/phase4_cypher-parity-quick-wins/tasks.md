## 1. Scalar functions
- [ ] 1.1 `randomUUID()` (v4, string)
- [ ] 1.2 String functions: `ascii()`, `chr()`, `lpad()`, `rpad()`, `normalize()` (NFC default + optional form arg)
- [ ] 1.3 Math: two-arg `log(x, base)`; `isNaN()`
- [ ] 1.4 List: `shuffle()`

## 2. Formats and verification
- [ ] 2.1 `elementId()` returns a Neo4j-5-style opaque stable string; `id()` keeps the integer; CHANGELOG documents the change
- [ ] 2.2 Verify `percentileDisc`/`percentileCont`/`stDev`/`stDevP` against Neo4j reference outputs; fix any divergence

## 3. Parser
- [ ] 3.1 Multiple comma-separated patterns in one CREATE: `CREATE (a:L), (b:L)`

## 4. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 4.1 Update or create documentation covering the implementation (cypher-subset.md + CHANGELOG)
- [ ] 4.2 Write tests covering the new behavior (each function: happy path + null + type-error; parity spot-check vs Neo4j semantics)
- [ ] 4.3 Run tests and confirm they pass (`cargo +nightly test -p nexus-core`, clippy zero warnings)
