# Proposal: Nexus ↔ Neo4j Comparative Benchmark Suite

## Why

Paridade funcional é necessária mas não suficiente. Quando um cliente
troca Neo4j por Nexus, a primeira pergunta não é "o Cypher funciona?"
(que é verificado pelos 300/300 diff tests); é "é igual ou mais
rápido?". Hoje não temos resposta quantitativa.

Consequências concretas:

- Regressões de performance só aparecem em produção dos clientes.
- A declaração "Nexus é read-heavy-friendly" é aspiracional, não
  medida. Não sabemos, função por função, onde estamos à frente ou
  atrás do Neo4j.
- Os oito tasks da Phase 6 (QPP, R-tree, FTS, APOC, composite indexes,
  etc.) adicionam ~300k linhas de código novo. Sem benchmark
  comparativo, otimizar é adivinhar.
- Clientes B2B pedem "show me the numbers". Produtos concorrentes
  (ArangoDB, Dgraph, TigerGraph) publicam comparativos com Neo4j; o
  silêncio de Nexus é interpretado como fraqueza.

Este task entrega um harness comparativo completo: mesmos dados,
mesmas queries, execução lado-a-lado contra Neo4j Community 5.x via
Docker, com relatório Markdown/JSON e CI gates impedindo regressões.
Cobertura planeada: **toda** a superfície de função, operação de
query, índice, e procedure shippada nas phases anteriores.

Valor adicional: o harness serve como **fonte de verdade** para o
`NEO4J_COMPATIBILITY_REPORT.md` — as percentagens de paridade deixam
de ser manualmente mantidas e passam a ser geradas pelo resultado dos
runs.

## What Changes

### Novo crate `nexus-bench`

Separado de `cargo bench` (que fica para microbenchmarks internos de
operadores). Este crate usa Criterion mas contra **engines vivos**
(Nexus em-processo + Neo4j via Bolt sobre TCP). Produz relatórios
estruturados, não apenas output console.

### Docker harness

`scripts/bench/docker-compose.yml` sobe Neo4j Community 5.15 em
porta isolada com config mínima (sem métricas externas, sem TLS,
cache fixo em 512 MiB). Nexus roda em-processo (driver library
direto) para não pagar o custo do HTTP frame.

**Mesma comparação em dois modos** para separar overhead de
transporte: Nexus "embedded" vs Nexus "REST" vs Neo4j "Bolt". A
métrica canonical é embedded-vs-Bolt (apples-to-apples no
lado do executor); REST é reportado para diagnóstico.

### Dataset catalogue

Três datasets versionados:

- **micro**   — 10k nodes, 50k rels, 5 labels, 3 props/node. Ideal
  para funções e operadores escalares.
- **social**  — LDBC SNB sf=0.1 (~1M nodes, ~5M rels). Ideal para
  traversals e agregações.
- **vector**  — 100k nodes com embedding 384-dim, 10 labels. Ideal
  para KNN/FTS/hybrid.

Geração reproduzível via semente fixa; armazenada em `tests/data/bench/`
como dumps binários (dump binário Nexus + Cypher script para Neo4j)
com hashes SHA-256 verificados.

### Query catalog (~450 benchmarks)

Estruturado em 12 categorias, cada uma com cenários:

1. **Scalar functions** (~120)    — uma query por função, comparativa.
2. **Aggregations** (~30)         — COUNT/SUM/AVG/COLLECT/percentile/stdev.
3. **Point reads** (~15)          — ID lookup, property lookup, index seek.
4. **Label scans** (~10)          — pure label, label+filter.
5. **Traversals** (~40)           — 1-hop, 2-hop, variable-length, QPP, shortestPath.
6. **Writes** (~30)               — CREATE/MERGE/SET/DELETE, batched, `IN TRANSACTIONS`.
7. **Indexes** (~25)              — bitmap, B-tree, KNN, R-tree, FTS, composite.
8. **Constraint enforcement** (~20) — UNIQUE, NOT NULL, NODE KEY, property-type.
9. **Subqueries** (~20)           — EXISTS, COUNT, COLLECT, CALL, nested.
10. **Procedures** (~80)          — db.*, dbms.*, apoc.coll/map/text/path/periodic, gds.*
11. **Temporal/spatial** (~30)    — date arithmetic, point.withinDistance, R-tree seek.
12. **Mixed/hybrid** (~30)        — RAG-style: vector + graph + text combined.

Cada cenário declara: dataset, query (Cypher), parameters, warmup
iterations, measured iterations, timeout, expected row count (para
catch de output divergence).

### Metrics

Por cenário: p50, p95, p99, throughput (ops/s), peak RSS delta, CPU
time. Comparação absoluta (Nexus ms) e ratio (Nexus/Neo4j).
Classificação:

- ⭐ **Lead**          — Nexus < 0.8× Neo4j latency
- ✅ **Parity**        — 0.8× ≤ Nexus ≤ 1.2× Neo4j
- ⚠️ **Behind**         — 1.2× < Nexus ≤ 2× Neo4j
- 🚨 **Gap**            — Nexus > 2× Neo4j (regression budget exhausted)

### Reports

Três artefatos por run:

1. `bench/report.md` — tabela humana agrupada por categoria.
2. `bench/report.json` — estruturado, consumido pelo CI gate.
3. `bench/trace.sqlite` — série temporal histórica (uma linha por run
   por cenário) para detecção de regressão entre releases.

### CI integration

Nova workflow `.github/workflows/bench.yml`:

- Dispara em PRs com label `perf-check` ou em schedule semanal.
- Carrega o baseline de `bench/baselines/v<N>.json` (commitado).
- Executa o suite completo (ou um subset em PRs rápidos via label
  `perf-check-fast`).
- Falha se > 5% dos cenários regrediram ≥ 20% em relação ao baseline.
- Publica o report como comment no PR.

### Performance regression budget

Arquivo `bench/budget.toml` declara limites por cenário:

```toml
["traversals.fof_3_hop"]
max_latency_p95_ms = 80
max_ratio_vs_neo4j = 1.3
```

PRs que violam budget exigem override explícito do maintainer (label
`perf-exception` + justificativa na descrição).

**BREAKING**: nenhum. Este é um subsistema totalmente novo; não
altera comportamento de runtime.

## Impact

### Affected Specs

- NEW capability: `bench-harness`
- NEW capability: `bench-dataset-catalogue`
- NEW capability: `bench-query-catalogue`
- NEW capability: `bench-metrics-reporting`
- NEW capability: `bench-ci-gate`

### Affected Code

- `nexus-bench/Cargo.toml` (NEW crate)
- `nexus-bench/src/harness/mod.rs` (NEW, ~400 lines)
- `nexus-bench/src/harness/nexus_client.rs` (NEW, ~200 lines)
- `nexus-bench/src/harness/neo4j_client.rs` (NEW, ~250 lines, via `neo4rs`)
- `nexus-bench/src/datasets/mod.rs` (NEW, ~350 lines)
- `nexus-bench/src/datasets/micro.rs` (NEW, ~120 lines)
- `nexus-bench/src/datasets/social.rs` (NEW, ~200 lines, LDBC SNB generator)
- `nexus-bench/src/datasets/vector.rs` (NEW, ~150 lines)
- `nexus-bench/src/scenarios/scalar_fns.rs` (NEW, ~600 lines)
- `nexus-bench/src/scenarios/aggregations.rs` (NEW, ~250 lines)
- `nexus-bench/src/scenarios/traversals.rs` (NEW, ~400 lines)
- `nexus-bench/src/scenarios/writes.rs` (NEW, ~350 lines)
- `nexus-bench/src/scenarios/indexes.rs` (NEW, ~300 lines)
- `nexus-bench/src/scenarios/procedures.rs` (NEW, ~800 lines)
- `nexus-bench/src/scenarios/temporal_spatial.rs` (NEW, ~350 lines)
- `nexus-bench/src/scenarios/hybrid.rs` (NEW, ~400 lines)
- `nexus-bench/src/report/markdown.rs` (NEW, ~200 lines)
- `nexus-bench/src/report/json.rs` (NEW, ~150 lines)
- `nexus-bench/src/report/sqlite_trace.rs` (NEW, ~180 lines)
- `nexus-bench/src/budget.rs` (NEW, ~150 lines)
- `scripts/bench/docker-compose.yml` (NEW)
- `scripts/bench/run.sh` (NEW)
- `scripts/bench/compare-baselines.sh` (NEW)
- `.github/workflows/bench.yml` (NEW)
- `docs/benchmarks/README.md` (NEW)
- `docs/benchmarks/METHODOLOGY.md` (NEW)

### Dependencies

- Requires: Docker 24+ on the runner machine (for Neo4j container).
- External crates: `neo4rs = "0.8"` (Bolt client), `criterion = "0.5"`,
  `rusqlite = "0.31"` (trace store), `serde_json`.
- Optional: `ldbc-snb-datagen` via Docker for dataset generation.

### Unblocks

- Otimização orientada a dados de cada subsystem das phases anteriores.
- Publicação pública de comparativos de performance.
- Regression gates em cada PR de executor/índice.

### Timeline

- **Duration**: 3–4 semanas para o harness + categorias 1–6.
  Categorias 7–12 incrementais à medida que as features das phases 6.3–6.9
  são mergeadas.
- **Complexity**: Média — Criterion e drivers Bolt são mecânicos; a
  parte delicada é garantir que as medições sejam reproduzíveis
  (CPU pinning, warmup adequado, variância < 5% em CI).
- **Risk**: Baixa — nenhum código de produção muda; falhas do harness
  são de teste.
