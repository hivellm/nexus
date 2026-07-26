# Thunder interop matrix

Release gate for the Thunder RPC migration (`phase10_thunder-server-migration`
/ `phase11_thunder-client-migration`). **One `nexus-server` build, every Nexus
SDK, plus a from-scratch pre-Thunder client — the same four wire steps.** It
proves the one thing no single-language SDK suite can: that a Thunder-based
Nexus server and every Nexus client still agree on the wire, a pre-Thunder
client included.

Harness lives in [`scripts/interop/`](../../scripts/interop/); see its
[README](../../scripts/interop/README.md) for the full client contract.

## How it works

The driver [`run-matrix.py`](../../scripts/interop/run-matrix.py) owns the
server: it boots `nexus-server` from
[`server-config.yml`](../../scripts/interop/server-config.yml) on off-default
ports (REST 25474 / RPC 25475 / RESP3 25476) with **authentication required**,
runs each `clients/<lang>/` cell, renders a pass/fail matrix, and exits non-zero
on any red cell. A missing toolchain is an explicit **SKIP**, never a silent
pass. Adding a language is adding a directory — the driver never looks inside a
client.

Each cell is a standalone program that speaks one contract:

```
argv:   <host> <port> <user> <pass>
stdout: one `STEP <name> PASS|FAIL <detail>` line per step
exit:   0 iff every step passed
```

## The four steps

| Step | What it proves |
|------|----------------|
| `auth` | `PING` answers **before** auth; `STATS` is refused (typed `NOAUTH`) **before** `AUTH` and succeeds **after**. The pre-Thunder transports never sent `AUTH`, so an open server would have hidden that bug in every SDK. |
| `cypher` | A `CREATE (n:Interop… {id: $id}) RETURN` then `MATCH … RETURN` round-trips the id back — parameters, node values, and the result envelope all cross the wire intact. |
| `knn_bytes` | A raw little-endian `f32` vector `[1.5, -2.5, 3.5, +Inf]` carried as a **Bytes** value round-trips **byte-for-byte** (via `PING <bytes>` echo). Guards the non-UTF-8 corruption gotcha — a transport that funnels Bytes through a string cannot pass. |
| `error` | A deliberately broken `CYPHER` yields a **typed server error** (not a transport crash), and the **same connection stays usable** for a following valid call. |

## Result — 7/7 green

| SDK | authenticate | CREATE/MATCH | f32-LE bytes | error round-trip | Transport |
|---|---|---|---|---|---|
| `rust` | ✅ | ✅ | ✅ | ✅ | nexus-graph-sdk RPC transport, thunder-rpc |
| `typescript` | ✅ | ✅ | ✅ | ✅ | @hivehub/thunder via the SDK's dist build |
| `python` | ✅ | ✅ | ✅ | ✅ | hivellm-thunder via nexus_sdk |
| `csharp` | ✅ | ✅ | ✅ | ✅ | HiveLLM.Thunder via Nexus.SDK |
| `go` | ✅ | ✅ | ✅ | ✅ | thunder-go via the Go SDK |
| `php` | ✅ | ✅ | ✅ | ✅ | hivellm/thunder via the SDK |
| `legacy` | ✅ | ✅ | ✅ | ✅ | pre-Thunder wire replay: int-array Bytes, map-shaped frames |

Each SDK cell drives its own migrated transport, so a green row is that
published SDK agreeing with a Thunder-based server on all four steps.

## The legacy (backward-compatibility) cell

[`clients/legacy/interop_legacy.py`](../../scripts/interop/clients/legacy/interop_legacy.py)
imports **no SDK**. It hand-writes the wire exactly as the SDKs emitted it
before the Thunder swap, so it keeps testing the old encoding even after every
SDK has moved on and no pre-Thunder build is left to check out:

- requests are **map-shaped** (`{"id", "command", "args"}`) rather than the
  array form Thunder emits;
- `Bytes` are an **array of integers** rather than MessagePack `bin`.

It stays green because the server's rmp-serde decode tolerates both shapes —
map-shaped Request frames (**WIRE-013**) and int-array `Bytes` (**WIRE-011**).

> **A pre-Thunder client still in the wild keeps working against the migrated
> server, with no casualties.** (Synap's equivalent cell documents one casualty
> — legacy pub/sub-over-RPC via the reserved push id — that Nexus does not
> exercise, so Nexus has none.)

## Untouched-surface regression proof

The migration touched only the RPC framing (`crates/nexus-server/src/protocol/rpc`)
and the SDK transports. Two independent checks confirm nothing else moved:

### Transport parity — 7/7 OK, 0 divergent

[`test-transport-parity.sh`](../../scripts/compatibility/test-transport-parity.sh)
runs the same write-path battery over all three front-ends against one running
server and diffs the normalised `{columns, rows}` envelopes:

```
Nexus transport-parity harness
Total: 7   OK: 7   DIVERGENT: 0
```

**HTTP == RPC(Thunder) == RESP3.** Direct proof that the Thunder RPC front-end
returns byte-identical results to the untouched HTTP and RESP3 paths (the RESP3
leg is driven by a dockerized `redis-cli --json` reaching the host's
`0.0.0.0:15476`, which doubles as the RESP3 smoke).

### Neo4j differential suite — 308 / 325

[`test-neo4j-nexus-compatibility-200.ps1`](../../scripts/compatibility/test-neo4j-nexus-compatibility-200.ps1)
executes every case against both a live Neo4j 5 and Nexus over HTTP and compares:

```
Total Tests:   325
Passed:        308
Failed:        2
Skipped:       15
```

The 2 failures are **pre-existing Cypher-executor write-path limitations, not
Thunder regressions**:

| Case | Query | Executor rejection |
|------|-------|--------------------|
| 15.08 | `MERGE (a:Product {name:'A'}) MERGE (b:Product {name:'B'}) RETURN a.name, b.name` | Multiple different variables in RETURN not supported for write queries |
| 15.12 | `MERGE (n:Product {name:'Unique'}) WITH n MERGE (n2:Product {name:'Unique'}) RETURN count(DISTINCT n)` | Unsupported clause in write query |

The executor guard behind these predates 2026-06-09 (long before the Thunder
migration), and both queries return the **identical** rejection over HTTP and
over RPC(Thunder) — so the transport is not the cause. The suite grew to 325
(from the 300-case milestone) as spatial diff scenarios were added; these two
MERGE gaps are executor-level and out of scope for the transport migration.

## Running it

Build the server first, then run the matrix:

```bash
cargo +nightly build --release -p nexus-server
python scripts/interop/run-matrix.py            # every cell
python scripts/interop/run-matrix.py python go  # a subset
python scripts/interop/run-matrix.py --list     # what exists
```

This is the **release gate for any change to the RPC transport or an SDK's
transport layer** — re-run it and keep it 7/7 green before merging such a
change. A cell whose toolchain is absent SKIPs (not a pass); wire the toolchain
in, or run that cell on a machine that has it.

**Windows / winget PHP gotcha.** The winget `PHP.PHP.8.3` `php.exe` (a ZTS
console build) refuses to launch a *script* under Python's `CreateProcess`
(`WinError 5`); point the override at the sibling `php-win.exe`:
`NEXUS_INTEROP_PHP=…\PHP.PHP.8.3_…\php-win.exe`. A normal `php` on PATH needs no
override.

### Reproducing the regression legs

The parity and Neo4j legs need a server on the **default** ports and external
services:

```bash
# live Neo4j 5 for the differential suite
docker run -d --name nexus-compat-neo4j -e NEO4J_AUTH=neo4j/password \
  -p 7474:7474 -p 7687:7687 neo4j:5

# nexus-server with all three listeners; RESP3 on 0.0.0.0 so a container reaches it
NEXUS_ADDR=127.0.0.1:15474 NEXUS_RPC_ENABLED=true NEXUS_RPC_ADDR=127.0.0.1:15475 \
NEXUS_RESP3_ENABLED=true NEXUS_RESP3_ADDR=0.0.0.0:15476 \
  ./target/release/nexus-server

powershell -ExecutionPolicy Bypass -File scripts/compatibility/test-neo4j-nexus-compatibility-200.ps1
NEXUS_CLI=./target/release/nexus scripts/compatibility/test-transport-parity.sh
```
