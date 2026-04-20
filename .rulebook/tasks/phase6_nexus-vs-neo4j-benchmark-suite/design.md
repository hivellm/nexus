# Nexus vs Neo4j Benchmark Suite — Technical Design

## Scope

Um harness comparativo que executa o mesmo catálogo de queries contra
Nexus e Neo4j Community 5.15, mede latência e throughput, e emite
relatórios estruturados consumidos por um CI gate.

Não é um microbenchmark interno (esse é o `cargo bench` existente).
É um benchmark **externo, end-to-end, comparativo**, focado em
paridade e regressão.

## Arquitectura

```
        ┌──────────────────────────────────────────────┐
        │                 nexus-bench                  │
        │                                              │
        │   ┌──────────┐   ┌──────────┐  ┌──────────┐ │
        │   │ datasets │   │scenarios │  │  report  │ │
        │   └─────┬────┘   └────┬─────┘  └─────┬────┘ │
        │         │             │              │      │
        │         └──────┬──────┘              │      │
        │                ▼                     │      │
        │         ┌─────────────┐              │      │
        │         │   harness   │──────────────┘      │
        │         └─────┬───────┘                     │
        │               │                             │
        │     ┌─────────┴─────────┐                   │
        │     ▼                   ▼                   │
        │ ┌──────────┐      ┌──────────┐              │
        │ │  nexus   │      │  neo4j   │              │
        │ │(embedded)│      │  (bolt)  │              │
        │ └──────────┘      └────┬─────┘              │
        └────────────────────────┼────────────────────┘
                                 │
                         ┌───────▼────────┐
                         │ docker-compose │
                         │  Neo4j 5.15    │
                         └────────────────┘
```

## Engine clients

`BenchClient` trait:

```rust
trait BenchClient: Send + Sync {
    fn name(&self) -> &'static str;
    fn reset(&mut self) -> Result<()>;                    // drop all data
    fn load_dataset(&mut self, d: &Dataset) -> Result<()>;
    fn execute(&mut self, q: &str, params: &Params) -> Result<QueryResult>;
    fn metrics_snapshot(&self) -> EngineMetrics;          // RSS, CPU time
}
```

Três implementações:

- `NexusEmbedded` — importa `nexus_core::Engine` diretamente no
  processo. Sem HTTP. Usa a mesma API que os SDKs Rust chamam.
- `NexusRest` — opcional, mesmo binário em processo separado falando
  `/cypher`. Útil para quantificar o overhead do HTTP.
- `Neo4jBolt` — `neo4rs` client contra o container Docker.

A comparação canónica é `NexusEmbedded` vs `Neo4jBolt`. Dá vantagem
ao Nexus por não pagar transporte, mas é a comparação **correta** se
o cliente final do Nexus é o SDK embedded ou o próprio servidor (o
SDK embedded é o principal caso de uso para RAG). Comparar com REST
também é disponibilizado via flag `--transport rest`.

## Docker orchestration

`scripts/bench/docker-compose.yml`:

```yaml
services:
  neo4j:
    image: neo4j:5.15.0-community@sha256:<pinned>
    ports: ["17687:7687"]
    environment:
      NEO4J_AUTH: none
      NEO4J_server_memory_pagecache_size: 512M
      NEO4J_server_memory_heap_initial__size: 512M
      NEO4J_server_memory_heap_max__size: 512M
      NEO4J_dbms_security_procedures_unrestricted: apoc.*
    volumes:
      - neo4j-data:/data
```

O harness espera por `bolt://localhost:17687` estar pronto antes de
cada run, e executa `MATCH (n) DETACH DELETE n` no reset entre
cenários que exigem estado limpo.

## Datasets

### `micro`

Gerador determinístico (seed = 42):
- 10k nodes entre 5 labels com 3 props cada (INTEGER, STRING, FLOAT).
- 50k relationships de 3 types entre pares aleatórios.
- Inserção via bulk import nativo em cada engine.

### `social`

LDBC Social Network Benchmark, scale factor 0.1:
- ~1M Person, Post, Comment, Forum, Tag, Place nodes.
- ~5M relacionamentos entre eles.
- Usa o datagen oficial via Docker, output CSV.
- Load para Neo4j via `LOAD CSV` + `admin-import`.
- Load para Nexus via endpoint de ingest direto ou `CALL IN TRANSACTIONS`.

### `vector`

- 100k nodes com embedding 384-dim (random unit vectors).
- 10 labels com distribuição 10% cada.
- 20k nodes também têm um corpus textual (leading paragraph da
  Wikipedia) para FTS.
- Serve os cenários "hybrid" (KNN + FTS + graph).

Hashes SHA-256 dos dumps são commitados em `tests/data/bench/*.sha256`
e verificados no início de cada run; mismatch é erro fatal.

## Scenario model

```rust
pub struct Scenario {
    pub id: &'static str,                  // "traversals.fof_3_hop"
    pub category: Category,
    pub dataset: DatasetId,
    pub query: &'static str,               // Cypher
    pub params: Params,
    pub warmup_iters: u32,                 // default 100
    pub measured_iters: u32,               // default 500
    pub timeout_ms: u64,                   // default 30_000
    pub expected: OutputExpectation,       // row count or checksum
}

pub enum OutputExpectation {
    AnyRows,                               // just checks non-error
    RowCount(usize),                       // exact row count
    Checksum(u64),                         // xxh3 over serialised rows
}
```

Cenários vivem em Rust, não em YAML, para que o compilador ajude a
evitar drift entre query e parâmetros.

## Measurement loop

```rust
fn run_scenario(client: &mut dyn BenchClient, s: &Scenario) -> Measurement {
    client.reset_if_required();
    for _ in 0..s.warmup_iters { client.execute(s.query, &s.params)?; }
    let samples = (0..s.measured_iters)
        .map(|_| time_one(&|| client.execute(s.query, &s.params)))
        .collect::<Vec<_>>();
    check_output(&samples[0], &s.expected)?;
    Measurement::from_samples(samples)
}
```

`time_one` usa `Instant::now()` com resolução ns. O processo é
pinado a um CPU específico via `taskset -c 2` (CI) ou
`sched_setaffinity` (Linux runtime). ASLR e turbo boost são
desativados no runner.

Variance sanity check: se stdev > 20% da média depois dos
measured_iters, o cenário é re-executado uma vez com 2× iterações.
Ainda variado → marcar como `⚠️ HIGH_VARIANCE` no report.

## Classification

Por cenário, para cada par (engine, métrica):

| Classe    | Critério                           | Significado                  |
|-----------|------------------------------------|------------------------------|
| ⭐ Lead   | Nexus p95 < 0.8× Neo4j p95         | Nexus mais rápido            |
| ✅ Parity | 0.8× ≤ ratio ≤ 1.2×                | Dentro da margem de ruído    |
| ⚠️ Behind  | 1.2× < ratio ≤ 2×                  | Atrás mas aceitável          |
| 🚨 Gap    | ratio > 2×                         | Budget excedido, ação requerida |
| ❌ Error  | Nexus failed or diverged           | Bloqueador de merge          |

## Reports

### Markdown (`bench/report.md`)

```markdown
## Traversals

| Scenario              | Nexus p95 | Neo4j p95 | Ratio  | Class |
|-----------------------|-----------|-----------|--------|-------|
| one_hop_by_label      | 0.18 ms   | 0.22 ms   | 0.82×  | ⭐    |
| two_hop_by_rel_type   | 1.4 ms    | 1.3 ms    | 1.08×  | ✅    |
| fof_3_hop             | 28 ms     | 19 ms     | 1.47×  | ⚠️    |
| qpp_bounded_1_5       | 44 ms     | 31 ms     | 1.42×  | ⚠️    |
```

### JSON (`bench/report.json`)

Consumido pelo CI gate:

```json
{
  "meta": {"nexus_version": "...", "neo4j_version": "5.15.0", "host": "...", "cpu": "..."},
  "scenarios": [
    {
      "id": "traversals.fof_3_hop",
      "category": "traversals",
      "nexus":  {"p50_ms": 22, "p95_ms": 28, "p99_ms": 33, "rss_mb": 420},
      "neo4j":  {"p50_ms": 17, "p95_ms": 19, "p99_ms": 21, "rss_mb": 780},
      "ratio":  1.47,
      "class":  "Behind"
    }
  ]
}
```

### SQLite trace (`bench/trace.sqlite`)

Uma tabela por cenário com colunas `(run_id, ts, commit, nexus_p95,
neo4j_p95)`. Permite gráficos de série temporal e detecção de
tendências entre releases.

## Performance budget

```toml
# bench/budget.toml
default_max_ratio = 1.5                  # global ceiling

["traversals.one_hop_by_label"]
max_latency_p95_ms = 0.3
max_ratio_vs_neo4j = 1.1

["traversals.fof_3_hop"]
max_latency_p95_ms = 80
max_ratio_vs_neo4j = 1.3

["writes.bulk_unwind_10k"]
min_throughput_ops_per_sec = 15000
```

`nexus-bench check-budget` retorna código 0 se tudo dentro, 1 caso
contrário. No CI isso falha o build.

## CI gate

```yaml
# .github/workflows/bench.yml
on:
  pull_request:
    types: [labeled]
  schedule:
    - cron: "0 3 * * 1"

jobs:
  bench:
    if: github.event.label.name == 'perf-check' || github.event_name == 'schedule'
    runs-on: [self-hosted, bench-rig]
    steps:
      - uses: actions/checkout@v4
      - run: docker compose -f scripts/bench/docker-compose.yml up -d
      - run: cargo bench --package nexus-bench -- --output json > bench/report.json
      - run: nexus-bench check-budget bench/budget.toml bench/report.json
      - run: nexus-bench compare-baselines bench/baselines/latest.json bench/report.json
      - run: gh pr comment ${{ github.event.pull_request.number }} --body-file bench/report.md
```

O runner `bench-rig` é uma máquina dedicada (EC2 `c7i.4xlarge` ou
on-prem) sem outras cargas para manter variância baixa.

## Reproducibility

Para minimizar variância:

- Runner dedicado com SMT desligado, governador cpufreq = performance.
- Processo pinado com `taskset -c 2-5` (4 cores isolados do kernel).
- ASLR desativado via `echo 0 > /proc/sys/kernel/randomize_va_space`
  no setup do runner (documentado em METHODOLOGY.md).
- Docker container com `cpuset="6-9"` para não competir com o
  benchmark runner.
- Timeout absoluto por cenário (30s) para falhas rápidas.
- Cache de disco esvaziado entre datasets via `echo 3 > /proc/sys/vm/drop_caches`.

Tudo isso é automatizado em `scripts/bench/prepare-runner.sh`.

## Performance targets (initial)

Alvos de primeira linha, ajustados no primeiro run real:

| Categoria              | Alvo inicial                                    |
|------------------------|-------------------------------------------------|
| Scalar functions       | Parity ou Lead em ≥ 90% dos cenários            |
| Aggregations           | Parity em ≥ 85%                                 |
| Point reads            | Lead em ≥ 70% (nossa especialidade)             |
| Label scans            | Parity ou Lead em ≥ 80%                         |
| Traversals (≤ 3 hops)  | Parity em ≥ 80%                                 |
| Traversals (QPP)       | Behind aceite em ≥ 60% inicialmente             |
| Writes                 | Parity em ≥ 70%                                 |
| Indexes (B-tree, KNN)  | Parity ou Lead em ≥ 85%                         |
| R-tree, FTS            | Parity em ≥ 70% (novos subsystems)              |
| Procedures             | Behind aceite em ≥ 50% (APOC é código Java maduro) |

Budgets são revistos a cada release; regressão dentro do budget é OK.

## Out of scope

- Benchmarks de cluster distribuído (Phase 5 sharding). Esses têm
  seu próprio harness em `bench/cluster/`.
- Comparação com databases não-Cypher (PostgreSQL+AGE, ArangoDB).
- Benchmarks de storage engine isolados (cargo bench cobre).
- Profiling interno (flamegraph, `perf`) — complementar, não
  substituído por este task.

## Rollout

- **v1.4.0**: harness + datasets + cenários de função/traversal/writes.
- **v1.5.0**: cenários de procedures + constraints + hybrid.
- **v1.5.1**: CI gate on por default em PRs que tocam executor/index.
- **v1.6.0**: publicação pública do relatório comparativo como docs.
