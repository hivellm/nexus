# Compatibility test scripts

Neo4j-diff and Nexus-internal compatibility harnesses live in this
directory. This note only covers the transport-parity runner; see each
script's own header comment for everything else (Neo4j diff suite,
external-ID Docker tests, WAL replay tests).

## Per-transport write-path parity (`test-transport-parity.sh`)

**Release gate.** Runs a representative battery of write-path Cypher
queries over all three Nexus TCP transports — HTTP (15474), native RPC
(15475), RESP3 (15476) — against one already-running server, and fails if
any transport's result diverges from the others. This is the regression
net for the 2.4.0-era "five divergent write implementations" bug class
(`docs/nexus/02-bug-inventory.md`, bugs B1/B2/B3/B6): the 300-test Neo4j
diff suite only ever ran over HTTP, so transport-specific write bugs
(MERGE creating 0 relationships, `SET r.k` silently dropped, `$params`
dropped on RPC/RESP3) shipped undetected.

Usage:

```bash
# 1. Start a server (this script never manages its lifecycle):
./target/release/nexus-server
# 2. Build the CLI once (speaks HTTP + RPC with a shared --json shape):
cargo +nightly build --release --package nexus-cli
# 3. Run the parity battery:
./scripts/compatibility/test-transport-parity.sh
```

Requires `curl` and `docker` on the host; RESP3 is spoken via a disposable
`redis-cli --json` container and JSON normalization via a disposable `jq`
container, so no native `redis-cli`/`jq` install is needed. Exit code is
non-zero on any divergence — safe to wire into CI as a release gate
(tracked follow-up: task `phase7_benchmark-rebaseline` item 3.2, wiring the
full battery in).

Full detail — battery scope, exclusions with rationale, the isolation
strategy per test case — is documented in the script's own header comment.
