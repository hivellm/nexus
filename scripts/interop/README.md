# Thunder interop matrix

Release gate for the Thunder migration (phases 10–11). One `nexus-server` build,
every SDK, the same four steps — proof that a Thunder-based Nexus server and
every Nexus client still agree on the wire, including a pre-Thunder client still
in the wild (the `legacy` cell).

```
python scripts/interop/run-matrix.py            # every cell
python scripts/interop/run-matrix.py python go  # a subset
python scripts/interop/run-matrix.py --list     # what exists
python scripts/interop/run-matrix.py --server 127.0.0.1:25475   # external server
```

Build the server first: `cargo +nightly build --release -p nexus-server`.

The driver ([`run-matrix.py`](run-matrix.py)) owns the server (booted from
[`server-config.yml`](server-config.yml) on off-default ports with auth
REQUIRED), runs each cell, and renders a pass/fail matrix. It exits non-zero if
any cell that ran is red. A missing toolchain is an explicit **SKIP**, never a
pass. Override a cell's launcher with `NEXUS_INTEROP_<CELL>` (e.g.
`NEXUS_INTEROP_PHP=C:\php\php.exe`) when the name on PATH cannot be spawned.

## Client contract

Each cell is a standalone program under [`clients/`](clients) that speaks:

```
argv:   <host> <port> <user-or-key> <pass>
stdout: one `STEP <name> PASS|FAIL <detail>` line per step
exit:   0 iff every step passed
```

The driver never looks inside a client — adding a language is adding a
directory. Steps run in order; a client should print every step's line even
after one fails so the matrix shows exactly which cell broke.

### Steps

| Step | What it proves |
|------|----------------|
| `auth` | `PING` answers **before** auth; `STATS` is refused (typed `NOAUTH`) **before** `AUTH` and succeeds **after**. The pre-Thunder transports never sent `AUTH`, so an open server would have hidden that bug. |
| `cypher` | A `CYPHER` `CREATE (n:Interop {id: $id}) …` then `MATCH … RETURN` round-trips the id back — parameters, node values, and the result envelope all cross the wire intact. |
| `knn_bytes` | A raw little-endian `f32` vector carried as a **Bytes** value round-trips **byte-for-byte** (via `PING <bytes>` echo). Guards the non-UTF-8 corruption gotcha: the client also checks its own `Array<Float>` → f32-LE encoding matches the blob it sent. |
| `error` | A deliberately broken `CYPHER` yields a **typed server error** (not a transport crash), and the **same connection stays usable** for a following valid call. |

## Layout

```
scripts/interop/
├── server-config.yml     # the one config every cell is measured against
├── run-matrix.py         # driver: boots server, runs cells, renders matrix
├── clients/
│   ├── rust/             # nexus-graph-sdk RPC transport
│   ├── typescript/       # @hivehub/thunder via the SDK
│   ├── python/           # hivellm-thunder via nexus_sdk
│   ├── go/               # thunder-go via the Go SDK
│   ├── csharp/           # HiveLLM.Thunder via Nexus.SDK
│   ├── php/              # hivellm/thunder via the SDK
│   └── legacy/           # pre-Thunder wire replay (int-array Bytes, map frames)
```
