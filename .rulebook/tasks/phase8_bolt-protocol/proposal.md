# Proposal: phase8_bolt-protocol

## Why

The single biggest ecosystem gap
([docs/nexus/01-compatibility-gaps.md](../../../docs/nexus/01-compatibility-gaps.md),
[03-performance.md](../../../docs/nexus/03-performance.md) bottleneck #8):
without Bolt, every driver-class client is incompatible — all 6 official
Neo4j language drivers, Neo4j Browser/Desktop, and the LangChain/LlamaIndex
`Neo4jGraph` connectors that dominate the RAG segment Nexus targets. Memgraph
implements Bolt precisely to inherit this ecosystem. Additionally, the
HTTP+JSON transport imposes a ~1ms fixed floor per query; Bolt's binary
framing removes it for driver clients.

This is the last phase of 2.5.0 and explicitly cuttable to 2.6 — every
preceding phase ships value without it.

## What Changes

Implement a Bolt v5.x server listener (new port, default 7687 for drop-in
compatibility):

- Handshake + version negotiation (v5.x range)
- Messages: HELLO/LOGON, RUN, PULL, DISCARD, BEGIN/COMMIT/ROLLBACK, RESET,
  GOODBYE; PackStream v2 serialization (incl. Node/Relationship/Path/temporal
  /spatial structures)
- Routing table stub (single-instance ROUTE response) so drivers using
  `neo4j://` URIs connect
- Auth bridge to the existing auth layer (basic auth token)
- Queries route through `Engine::execute_cypher_with_params` — the unified
  entry point from phase1 (prerequisite: write-path unification done)

## Impact

- Affected specs: specs/bolt/spec.md (this task)
- Affected code: new `crates/nexus-server/src/protocol/bolt/` module; server
  startup/config for the listener; docs + docker-compose port mapping
- Breaking change: NO (additive listener)
- User benefit: official Neo4j drivers, Neo4j Browser, and
  LangChain/LlamaIndex connectors work against Nexus out of the box —
  drop-in adoption path for the entire Neo4j ecosystem.

## Success criteria (gate)

Official neo4j-python-driver and neo4j-javascript-driver connect, run
parameterized reads+writes, stream results, and handle transactions against
Nexus; Neo4j Browser connects and renders a graph.
