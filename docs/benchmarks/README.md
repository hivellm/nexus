# Nexus benchmarks

Operator-facing guide to running the `nexus-bench` harness —
Nexus-only baseline and the comparative Nexus-vs-Neo4j flow.

The harness itself is documented in
[`crates/nexus-bench/README.md`](../../crates/nexus-bench/README.md);
this page is the "here is the Docker workflow, here is how the
parity report gets regenerated" side of the story.

## TL;DR

```bash
# 1. build release-mode binary with both transports enabled
cargo build --release -p nexus-bench --features live-bench,neo4j --bin nexus-bench

# 2. start Nexus (its RPC listener binds by default)
./target/release/nexus-server &

# 3. bring up the pinned Neo4j container
./scripts/bench/neo4j-up.sh

# 4. run the comparative harness
./target/release/nexus-bench \
    --rpc-addr 127.0.0.1:15475 \
    --neo4j-url bolt://127.0.0.1:17687 \
    --compare \
    --i-have-a-server-running \
    --load-dataset \
    --format both --output target/bench/report

# 5. patch the compat report's parity section from the JSON
./scripts/bench/update-parity.sh target/bench/report.json

# 6. tear Neo4j down
./scripts/bench/neo4j-down.sh
```

## Design principles

- **Both sides binary.** Nexus goes over its native MessagePack
  RPC, Neo4j over Bolt. HTTP is not a transport option — a
  `Nexus-HTTP ↔ Neo4j-Bolt` run measures JSON serialisation, not
  engine work.
- **Nothing spawns an engine.** Both servers must already be
  listening before the harness runs. The harness refuses to do
  anything destructive without `--i-have-a-server-running`.
- **Every knob has a ceiling.** See the ceilings table in the
  crate README — per-call timeouts, scenario iteration counts,
  multipliers are all clamped inside the library.
- **Debug builds are rejected.** Numbers from `cargo build`
  without `--release` are 10–100× slower than production and
  meaningless for comparison.

## Docker harness

The Neo4j container lives in `scripts/bench/`:

| File | Purpose |
|---|---|
| [`docker-compose.yml`](../../scripts/bench/docker-compose.yml) | Neo4j Community 5.15 pinned by image digest; ports remapped `7687→17687` / `7474→17474`; 256/512 MiB heap; 512 MiB page cache; `NEO4J_AUTH=none`; dedicated named volume. |
| [`neo4j-up.sh`](../../scripts/bench/neo4j-up.sh) | Idempotent start. Polls `http://localhost:17474` for up to 30 s; no-op when the container is already running. |
| [`neo4j-down.sh`](../../scripts/bench/neo4j-down.sh) | Idempotent stop. `compose down -v --remove-orphans` so the volume is dropped and the next start is a clean database. |
| [`smoke.sh`](../../scripts/bench/smoke.sh) | `up → cypher-shell RETURN 1 → down` lifecycle smoke. 30 s target on a warm image; the script exits 2 if it drifts past 60 s. Run it once after any change to the compose file. |

### Ports at a glance

| Service | Internal | Host |
|---|---|---|
| Nexus RPC | `:15475` (default per `RpcConfig::default()`) | `:15475` |
| Neo4j Bolt | `:7687` | `:17687` |
| Neo4j HTTP (admin) | `:7474` | `:17474` |

The Neo4j ports are deliberately remapped so a local Neo4j
Desktop install (which claims `7687` / `7474`) does not collide
with the bench container.

### Memory budget

The `docker-compose.yml` sizes Neo4j for a workstation, not
production: 256 MiB heap floor, 512 MiB heap ceiling, 512 MiB
page cache. The page cache is the only knob that moves the
needle for the read-dominated seed scenarios. Bump it in the
compose file if you add larger datasets in follow-up work.

## Parity report automation

`scripts/bench/update-parity.sh` is a thin shell wrapper around
`update-parity.py`; running it rewrites the **Benchmark Parity**
block in
[`docs/compatibility/NEO4J_COMPATIBILITY_REPORT.md`](../compatibility/NEO4J_COMPATIBILITY_REPORT.md)
from a `report.json` shaped per
[`src/report/json.rs`](../../crates/nexus-bench/src/report/json.rs).

The rewriter only touches the region fenced by
`<!-- BEGIN bench-parity ... -->` / `<!-- END bench-parity -->`,
so:

- Hand-written copy around the block is preserved.
- Running twice against the same report is idempotent (the
  second run prints `already up to date` and writes nothing).
- Missing markers is a hard error (exit code 2) — no silent
  append to the end of the file.

Exit codes:

| Code | Meaning |
|---|---|
| 0 | Rewrite applied or file already up to date. |
| 1 | Input or doc path missing / unreadable. |
| 2 | BEGIN / END markers not found in the doc. |
| 3 | `report.json` shape invalid (no `rows` field). |

### Report JSON shape

```
{
  "schema_version": 1,
  "timestamp":       "2026-04-20T12:00:00Z",
  "nexus_version":   "1.0.0",
  "scenario_count":  25,
  "rows": [
    {
      "scenario_id":  "scalar.literal_int",
      "category":     "scalar",
      "nexus":        { "p50_us": 40,  "p95_us": 55,  ... },
      "neo4j":        { "p50_us": 120, "p95_us": 180, ... },
      "ratio_p50":    0.33,
      "classification": "Lead"
    },
    ...
  ]
}
```

`neo4j` is `null` and `ratio_p50` / `classification` are `null`
when the run is Nexus-only. The rewriter renders `—` in those
columns so the report still reads cleanly.

## Classification buckets

| Banner | Meaning | Ratio range (`nexus.p50 / neo4j.p50`) |
|---|---|---|
| ⭐ Lead | Nexus meaningfully faster. | `< 0.80` |
| ✅ Parity | Within 20 % either way. | `0.80 … 1.20` |
| ⚠️ Behind | Slower but within 2×. | `1.20 … 2.00` |
| 🚨 Gap | 2× slower or non-finite ratio. | `> 2.00` or NaN |

Exact thresholds live in
[`src/report/mod.rs`](../../crates/nexus-bench/src/report/mod.rs)
(`Classification::from_ratio`). Move them in code, not in docs.

## Integration tests

Two `#[ignore]` suites, nine tests total.

[`tests/live_compare.rs`](../../crates/nexus-bench/tests/live_compare.rs)
— five comparative tests, require both `NEXUS_BENCH_RPC_ADDR`
and `NEO4J_BENCH_URL`:

1. `both_health_probes_succeed` — HELLO + PING on Nexus, HELLO +
   `RETURN 1` on Neo4j.
2. `both_engines_accept_tiny_dataset` — single-CREATE literal
   applies on both.
3. `comparative_scalar_one_shot` — `RETURN 1 AS n` on both,
   asserts row-count parity and that `ComparativeRow` populates
   `ratio_p50` + `classification`.
4. `comparative_seed_catalogue_completes` — every seed scenario
   on both, per-scenario row-count parity.
5. `isolation_between_tests_works` — two reset → load cycles on
   both engines; asserts each reset zeroes the database. Locks
   the `BenchClient::reset` contract that the whole suite
   depends on for per-test fixture isolation.

[`tests/live_rpc.rs`](../../crates/nexus-bench/tests/live_rpc.rs)
— four Nexus-only tests, require just `NEXUS_BENCH_RPC_ADDR`:
`health_probe_succeeds`, `scalar_one_shot_returns_single_row`,
`seed_catalog_run_completes`, `isolation_between_loads_works`.

Each test calls `common::reset_single` / `common::reset_both`
(from
[`tests/common/mod.rs`](../../crates/nexus-bench/tests/common/mod.rs))
up front before loading `TinyDataset`, so the whole batch runs
cleanly in parallel against long-running servers without manual
wipes between iterations.

Arm them with the env vars + `-- --ignored`:

```bash
NEXUS_BENCH_RPC_ADDR=127.0.0.1:15475 \
NEO4J_BENCH_URL=bolt://127.0.0.1:17687 \
  cargo test -p nexus-bench --features live-bench,neo4j -- --ignored --test-threads=1
```

`--test-threads=1` serialises the suite so each test's `reset →
load → assert` cycle sees its own state. Without it two tests
can concurrently `CREATE` 100 nodes into the same backing
database and the strict post-load count assertions (100 nodes +
50 edges) race each other. True per-test parallel isolation is
out of scope here — it would need a per-test scratch database.

Each test short-circuits cleanly (`eprintln!` + `return`) when
its env vars are missing, so arming only one side still lets the
single-engine suite run without false failures.

## Troubleshooting

- **`cypher-shell` missing in smoke.sh** — the Neo4j image ships
  with `cypher-shell`; if `docker exec nexus-bench-neo4j
  cypher-shell` fails with `command not found`, the image digest
  in `docker-compose.yml` has drifted. Pull the tag again and
  refresh the digest.
- **`HELLO failed: id mismatch`** — the RPC listener reused an id
  across a stale connection. `neo4j-down.sh && neo4j-up.sh` on
  the Neo4j side, and restart the Nexus server on the Nexus side.
- **Parity block missing** — the update-parity script exits with
  code 2 when `<!-- BEGIN bench-parity -->` is not found in the
  compat report. Somebody removed the marker by hand; restore it
  from git.
- **Numbers look suspicious** — first check `cargo build
  --release`, not `cargo build`. The CLI refuses to run from a
  debug build, but a stale release binary from an older commit
  is still a valid candidate for "where did that regression come
  from".
