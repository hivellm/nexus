# 6. RESP3 compatibility layer (not RESP2, not a custom debug protocol)

**Status**: proposed
**Date**: 2026-04-19
**Related Tasks**: phase1_nexus-resp3-compatibility, phase3_rpc-protocol-docs-benchmarks

## Context

Nexus needed a diagnostic port that mirrored Redis's interactive-tool ecosystem — `redis-cli`, `iredis`, Grafana's Redis data source, packet captures. The RPC port carries binary MessagePack that's not trivially human-readable; ops teams wanted a protocol they could poke at with `telnet` or `nc` to verify the server is up and answering Cypher. The options were RESP2 (Redis's classic line-protocol), RESP3 (the 2020 revision with typed maps/sets/doubles/true/false/bigints), or a bespoke debug protocol.

## Decision

Ship a **RESP3** listener on port 15476 (disabled by default) that accepts Nexus's command vocabulary and returns typed RESP3 responses. Explicitly reject Redis KV commands (`SET`, `GET`, `HSET`, …) with `-ERR unknown command 'SET' (Nexus is a graph DB, see HELP)` so users are never confused into thinking Nexus is a KV store.

## Alternatives Considered

- RESP2 — only integers, bulk strings, simple strings, errors, arrays. Forces us to stringify floats and booleans, which loses fidelity for Cypher return values and makes tail-reading in Grafana harder. RESP3 is a strict superset; the few tools still on RESP2 can downgrade via HELLO 2 (not implemented in v1).
- A custom line-protocol — would get us exactly the shape we want but gives up the ecosystem. `redis-cli` and `iredis` auto-complete, pipeline, and script against RESP; no tool does that against bespoke protocols.
- Expose only a REPL over the HTTP endpoint — insufficient because HTTP isn't interactive (no server push, no connection-local state) and introduces latency we don't need for diagnostic queries.
- Dual RESP2 + RESP3 — rejected for v1. The overlap isn't worth the maintenance cost; modern tools (`redis-cli` 6+, `iredis`, Grafana 9+) all speak RESP3 by default.

## Consequences

Pros: ops can `redis-cli -p 15476 HELLO 3` and immediately run `CYPHER "MATCH (n) RETURN count(n)"` to prove the server is live. Grafana's Redis data source tails connection counters out of the box. Packet captures at port 15476 render as plain text. Zero extra dependencies — the parser/writer is ~600 LOC and lives entirely in `nexus-protocol`. No user confusion: aggressive KV rejection pre-empts the "why doesn't SET work?" support thread.

Cons: RESP3 is Redis-flavoured — users expecting strict Redis semantics (TTLs, pipeline transactions, pub/sub) will be disappointed. The port is explicitly disabled in the default server config so operators must opt in, which raises the bar for accidental exposure. RESP3 is NOT a first-class SDK transport in 1.0.0 (Rust / TS / Python / Go / C# / PHP all throw a configuration error for `transport: 'resp3'`) — it's a tooling port, not a primary traffic path. Documented in `docs/specs/resp3-nexus-commands.md` and `docs/OPERATING_RPC.md`.
