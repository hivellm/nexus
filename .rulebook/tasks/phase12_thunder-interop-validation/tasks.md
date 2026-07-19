# Tasks: phase12_thunder-interop-validation

Release gate for the Thunder migration (phases 10–11): cross-language interop
matrix + legacy-wire compatibility + regression proof on untouched surfaces.
Adapted from Synap `scripts/interop/` (its 1.2.0 gate).

## 1. Harness
- [ ] 1.1 Create `scripts/interop/server-config.yml` (off-default ports, e.g. HTTP 25474 / RPC 25475 / RESP3 25476; auth REQUIRED with a fixed root user + api key) and `scripts/interop/run-matrix.py`: boot `nexus-server` from that config, run each language cell, render pass/fail matrix, exit non-zero on any FAIL; per-cell toolchain override env `NEXUS_INTEROP_<CELL>`, missing toolchain → explicit SKIP
- [ ] 1.2 Define the client contract in `scripts/interop/README.md`: `argv: <host> <port> <user-or-key> <pass>`, stdout `STEP <name> PASS|FAIL <detail>`, exit 0 on all-pass; steps: `auth` (STATS gated before/after AUTH — PING answers pre-auth), `cypher` (CREATE + MATCH round-trip), `knn_bytes` (raw f32-LE Bytes == Array<Float>, byte-exact), `error` (typed server error, connection stays usable)

## 2. Language cells (independent — parallelizable)
- [ ] 2.1 `scripts/interop/clients/rust/` — drives the Rust SDK transport directly
- [ ] 2.2 `scripts/interop/clients/python/` — drives `nexus_sdk` transport + thunder_rpc credentials directly
- [ ] 2.3 `scripts/interop/clients/typescript/` — drives the TS SDK transport directly
- [ ] 2.4 `scripts/interop/clients/go/` — drives the Go SDK transport directly
- [ ] 2.5 `scripts/interop/clients/csharp/` — drives the C# SDK transport directly
- [ ] 2.6 `scripts/interop/clients/php/` — drives the PHP SDK transport directly

## 3. Backward compatibility
- [ ] 3.1 `scripts/interop/clients/legacy/` — replays the pre-Thunder Nexus wire (map-shaped Request frames, Bytes as int-array) against the new server and asserts correct decode + execution; documents any accepted casualties (Synap's: legacy pub/sub-over-RPC)

## 4. Untouched-surface regression proof
- [ ] 4.1 Re-run the Neo4j HTTP compat suite (`scripts/compatibility/test-neo4j-nexus-compatibility-200.ps1`) — 300/300 must hold
- [ ] 4.2 Re-run `scripts/compatibility/test-transport-parity.sh` (HTTP vs RPC vs RESP3 envelopes still identical) and a RESP3 smoke against the migrated server

## 5. Tail (docs + tests — check or waive with tailWaiver)
- [ ] 5.1 Update or create documentation covering the implementation (record the full matrix + legacy cell + regression results in `docs/protocol/thunder-interop-matrix.md`; finalize `docs/specs/rpc-wire-format.md` / `docs/specs/sdk-transport.md` / `docs/specs/api-protocols.md` as post-migration state; CHANGELOG release notes)
- [ ] 5.2 Write tests covering the new behavior (the interop matrix itself is the test artifact — wire it into CI as a workflow or documented manual gate so future transport changes re-run it)
- [ ] 5.3 Run tests and confirm they pass (matrix all-PASS incl. legacy cell; Neo4j suite 300/300; transport parity green; workspace gate: `cargo +nightly fmt --all`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo +nightly test --workspace`)
