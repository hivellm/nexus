# Proposal: phase8_bolt-protocol-shim

## Why

Nexus deliberately uses bespoke binary RPC (MessagePack on port 15475) instead of Neo4j's Bolt protocol. This is a sound technical choice — Nexus's RPC is 3–10× lower latency than HTTP/JSON in benchmarks — but it has one massive cost: **every existing Neo4j driver in every language refuses to connect to Nexus**. Java apps using `neo4j-driver`, Python apps using `neo4j-python`, JS apps using `neo4j-driver` for Node, Spring Data Neo4j, Cypher-Shell, Bloom, every IDE plugin — none of these work with Nexus today. A Bolt-protocol shim on a separate port that translates Bolt messages into the Nexus executor's internal interface unlocks all of them with zero changes to client code.

## What Changes

- New `crates/nexus-server/src/transport/bolt/` module implementing Bolt v5 wire protocol on port 7687 (Neo4j default), opt-in via `--enable-bolt` flag.
- Bolt frames map to existing executor calls: `RUN` → `Engine::execute`, `BEGIN`/`COMMIT`/`ROLLBACK` → `transaction::begin/commit/abort`, `RESET` → connection reset, `GOODBYE` → close.
- PackStream → `NexusValue` codec (PackStream is Bolt's wire format; similar to MessagePack but Neo4j-specific).
- Auth: support `principal/credentials` Basic + Bearer (JWT) — map to Nexus API-key + JWT.
- Result-streaming: chunk records into Bolt result records, propagate `has_more` correctly.
- Test against the official `neo4j-driver` Python client to confirm wire-level compat.
- Document scope: read-only point + traversal queries first; mutating queries second; explicit gaps (multi-database routing, cluster routing) clearly stated.

## Impact

- Affected specs: `docs/specs/api-protocols.md` (add Bolt section), README transports.
- Affected code: new `crates/nexus-server/src/transport/bolt/`, server config changes.
- Breaking change: NO (opt-in via flag).
- User benefit: drop-in compatibility with the entire Neo4j driver ecosystem; "switch the URL, keep your code" migration story.
